import { atom, read, update } from 'claude-code'
import type { EngineInterface, PluginOptions, Register } from 'claude-code'

import type { GateState, PaneDecision, Review, ReviewSection, ReviewStage } from '../types'
import { TERRY_ALERT, TERRY_CALM } from './art'
import { fitFiles, splitPatch } from './patch'
import { buildRows, lastColumn, toBlocks } from './rows'
import type { Row } from './rows'
import { lastOffset, nextColumn, nextOffset } from './scroll'

const PANE = 'terry-diff'
const PANE_TITLE = 'Diff review'
const COMMAND = 'terry'
// The gate (src/manifest.rs) writes one manifest per reviewed call; this is its format.
const MANIFEST_VERSION = 2
// The gate's review key is a SHA-256 hex digest; nothing else may name an approval file.
const REVIEW_KEY_PATTERN = /^[0-9a-f]{64}$/
// One drawing shows at most 100,000 characters; stay under it so no hunk is cut.
const MAX_DRAWN_CHARACTERS = 90_000
// Shown under a permission prompt for a gated call, which comes only when the gate found an
// approval file that the pane's Approve button did not write in this session.
const PANE_HINT = 'terry: this diff was not approved in the "Diff review" pane. Check it before you confirm.'
// Rows per mouse wheel tick when the userConfig field `scrollRows` is not set.
const DEFAULT_SCROLL_ROWS = 3
// Only commands that name git or gh can be gated; others skip the manifest watch.
const GIT_COMMAND_HINT = /\b(git|gh)\b/
const MANIFEST_POLL_MS = 250
// The gate's hook timeout is 90 s; a manifest that has not come by then is not coming.
const MANIFEST_WAIT_MS = 95_000
// How often the band asks `terry status`, so a `terry exclude` from any terminal shows soon.
const GATE_POLL_MS = 15_000
const GATE_STATUS_TIMEOUT_MS = 5_000
// `terry status` exits 1 for an excluded folder; a child killed by a signal also reads as 1.
const GATE_OFF_PREFIX = 'terry is off'
// Columns between the art and the band's words, and the fewest the words need beside the art.
const BAND_ART_GAP = 2
const BAND_MIN_TEXT_COLUMNS = 30
// Rows above the diff that do not scroll: the title, the buttons and the key hints while the gate
// waits for an answer; the status line and a blank row once the user answered; none otherwise.
const HEADER_ROWS_PENDING = 3
const HEADER_ROWS_ANSWERED = 2
// Columns one press of the left or right button moves the diff sideways.
const SIDEWAYS_STEP = 20
// Columns the pane's text starts in and the width assumed before the first drawing.
const DEFAULT_PANE_COLUMNS = 80
// A snowy owl's blue-white; the alert pose takes the theme's warning colour instead.
const CALM_COLOR = '#8FB8ED'
// One colour per layer of the branch chain, from the default branch down. Hex, because a theme has
// no name per layer; picked for a dark background.
const LAYER_COLORS = ['#8FB8ED', '#9BD39B', '#F2C879', '#E49AD0', '#7FD6D6', '#F29E8E']
const BRANCH_SEPARATOR = ' -> '

type ManifestSection = { title: string; notes: string[]; lint: string[]; patch_path: string | null }
type CallManifest = {
  version: number
  command: string
  stage: ReviewStage
  review_key: string
  sections: ManifestSection[]
}

const review = atom({ plugin: 'terry', key: 'review' } as const, null)
const promptReviewFor = atom({ plugin: 'terry', key: 'promptReviewFor' } as const, null)
const gate = atom({ plugin: 'terry', key: 'gate' } as const, 'unknown')
const branchChain = atom({ plugin: 'terry', key: 'branchChain' } as const, [])
const diffRow = atom({ plugin: 'terry', key: 'diffRow' } as const, 0)
const diffColumn = atom({ plugin: 'terry', key: 'diffColumn' } as const, 0)

async function diffsDir($: EngineInterface): Promise<string | null> {
  const home = await $.env.get('HOME')
  return home ? `${home}/.claude/diffs` : null
}

async function manifestPath($: EngineInterface, toolUseId: string): Promise<string | null> {
  const dir = await diffsDir($)
  if (dir === null) return null
  return `${dir}/calls/${toolUseId.replace(/[^A-Za-z0-9_-]/g, '_')}.json`
}

async function loadManifest($: EngineInterface, toolUseId: string): Promise<CallManifest | null> {
  const path = await manifestPath($, toolUseId)
  if (path === null || !(await $.fs.exists(path))) return null
  const manifest = JSON.parse(await $.fs.read(path)) as CallManifest
  return manifest.version === MANIFEST_VERSION ? manifest : null
}

async function loadReview($: EngineInterface, toolUseId: string): Promise<Review | null> {
  const manifest = await loadManifest($, toolUseId)
  if (manifest === null) return null

  let budget = MAX_DRAWN_CHARACTERS
  const sections: ReviewSection[] = []
  for (const section of manifest.sections) {
    const patch = section.patch_path ? await $.fs.read(section.patch_path) : ''
    const files = splitPatch(patch)
    const { shown, left } = fitFiles(files, budget)
    budget = left
    sections.push({
      title: section.title,
      notes: section.notes,
      lint: section.lint,
      patchPath: section.patch_path,
      files: shown,
      omittedFiles: files.length - shown.length,
    })
  }
  return {
    toolUseId,
    command: manifest.command,
    stage: manifest.stage,
    reviewKey: manifest.review_key,
    decision: 'pending',
    sections,
  }
}

function isApprovedInPane(shown: Review | null, reviewKey: string): boolean {
  return shown?.reviewKey === reviewKey && shown.decision === 'approved'
}

/**
 * Lets the gate pass this exact review on to the permission prompt (see src/approvals.rs):
 * `pane` after the user's Approve, so the prompt shows a summary; `prompt` where no window can
 * draw the pane, so the prompt shows the full review. False if the review cannot be named.
 */
async function writeApproval($: EngineInterface, held: Review, via: 'pane' | 'prompt'): Promise<boolean> {
  const dir = await diffsDir($)
  if (dir === null || !REVIEW_KEY_PATTERN.test(held.reviewKey)) return false
  await $.fs.write(`${dir}/approvals/${held.reviewKey}.json`, JSON.stringify({ via, command: held.command }))
  return true
}

/**
 * For a window that cannot draw the pane (an IDE): sends the review to the permission prompt,
 * and marks the held command, which `turn.complete` asks Claude to run again once the turn
 * that holds it ends.
 */
async function askForPromptReview($: EngineInterface, held: Review): Promise<void> {
  await $.ui.close({ id: PANE })
  if (!(await writeApproval($, held, 'prompt'))) return
  await update($, promptReviewFor, () => held.command)
}

function promptReviewRequest(command: string): string {
  return (
    `terry: the Diff review pane cannot be drawn in this window, so the permission prompt will ` +
    `show me the full diff. Run the same command again, exactly as before:\n${command}`
  )
}

/** Shows the call's review in the pane; with `onlyStage`, only a review of that stage. */
async function showReview($: EngineInterface, toolUseId: string, onlyStage?: ReviewStage): Promise<void> {
  const loaded = await loadReview($, toolUseId)
  if (loaded === null || (onlyStage && loaded.stage !== onlyStage)) return
  // The permission prompt holds the whole review; a pane beside it would only garble it.
  if (loaded.stage === 'prompt_only') return
  // Approve closed the pane so the permission prompt for the rerun draws alone; keep it closed.
  if (loaded.stage === 'prompt' && isApprovedInPane(await read($, review), loaded.reviewKey)) return
  await update($, review, () => loaded)
  // A checkout since the last poll must not leave an old chain beside this review.
  await refreshBranchChain($)
  await update($, diffRow, () => 0)
  await update($, diffColumn, () => 0)
  // In the pane stage no dialog is up, so the pane may take the keys for its hotkeys.
  const opened = await $.ui.open({
    id: PANE,
    title: PANE_TITLE,
    ...(loaded.stage === 'pane' ? { focus: true } : {}),
  })
  if (!opened.isPlaced) {
    // A terminal places it once it is wide enough or the person runs /terry; nothing else will.
    if (loaded.stage === 'pane' && !(await $.session.surfaces()).includes('terminal')) {
      await askForPromptReview($, loaded)
      return
    }
    $.ui.log(`terry: the diff pane is waiting (${opened.reason}); run /${COMMAND} to open it`)
    return
  }
  if (loaded.stage !== 'prompt') return
  // The line goes under the permission prompt; it is refused if the prompt is not up yet.
  try {
    $.ui.notice(toolUseId, PANE_HINT)
  } catch {
    // The pane is open either way.
  }
}

async function recordDecision($: EngineInterface, current: Review, decision: PaneDecision): Promise<void> {
  await update($, review, shown => (shown?.toolUseId === current.toolUseId ? { ...shown, decision } : shown))
}

/**
 * The Approve button: lets the gate pass this exact review on to the permission prompt, and
 * closes the pane, since a pane open beside that prompt garbles the terminal.
 */
async function approve($: EngineInterface, current: Review): Promise<void> {
  if (!(await writeApproval($, current, 'pane'))) return
  await recordDecision($, current, 'approved')
  await $.ui.close({ id: PANE })
  await $.prompt.submit({
    text: `terry: I approved the diff in the pane. Run the same command again, exactly as before:\n${current.command}`,
  })
}

/** The Reject button: puts a reply in the prompt box for the user to finish with a reason. */
async function reject($: EngineInterface, current: Review): Promise<void> {
  await recordDecision($, current, 'rejected')
  await $.prompt.fill({ text: `terry: I rejected the diff for: ${current.command}\nReason: ` })
}

/** The left and right buttons: moves the diff `direction` times `SIDEWAYS_STEP` columns. */
async function moveSideways($: EngineInterface, current: Review, direction: -1 | 1): Promise<void> {
  const last = lastColumn(buildRows(current, DEFAULT_PANE_COLUMNS))
  await update($, diffColumn, place => nextColumn(place, direction, SIDEWAYS_STEP, last))
}

async function readGateState($: EngineInterface): Promise<GateState> {
  try {
    const { exitCode, stdout } = await $.process.run(['terry', 'status'], { timeoutMs: GATE_STATUS_TIMEOUT_MS })
    if (exitCode === 0) return 'on'
    if (exitCode === 1 && stdout.startsWith(GATE_OFF_PREFIX)) return 'off'
    return 'unknown'
  } catch {
    // `terry` is not on PATH, or it did not finish in time.
    return 'unknown'
  }
}

/** The branch chain of the session's folder, by `terry branches`; empty when it cannot be read. */
async function readBranchChain($: EngineInterface): Promise<string[]> {
  try {
    const { exitCode, stdout } = await $.process.run(['terry', 'branches'], { timeoutMs: GATE_STATUS_TIMEOUT_MS })
    if (exitCode !== 0) return []
    return stdout.split('\n').filter(branch => branch !== '')
  } catch {
    // `terry` is not on PATH, or it did not finish in time.
    return []
  }
}

async function refreshBranchChain($: EngineInterface): Promise<void> {
  const chain = await readBranchChain($)
  await update($, branchChain, () => chain)
}

async function refreshGate($: EngineInterface): Promise<void> {
  const state = await readGateState($)
  await update($, gate, () => state)
  await refreshBranchChain($)
}

/** True while terry holds a command until the user answers in the pane. */
function isHolding(current: Review | null): current is Review {
  return current?.stage === 'pane' && current.decision === 'pending'
}

function scrollRowsFrom(options: PluginOptions): number {
  const rows = options['scrollRows']
  return typeof rows === 'number' && Number.isFinite(rows) && rows >= 1 ? Math.round(rows) : DEFAULT_SCROLL_ROWS
}

function headerRows(current: Review): number {
  if (current.stage !== 'pane') return 0
  return current.decision === 'pending' ? HEADER_ROWS_PENDING : HEADER_ROWS_ANSWERED
}

/** Rows of the diff the pane shows at once, under its fixed header. */
function windowRowsOf(current: Review, bodyRows: number): number {
  return Math.max(1, bodyRows - headerRows(current))
}

type RowStyle = { bold?: boolean; color?: string; dimColor?: boolean }

const ROW_STYLES: Record<Row['kind'], RowStyle> = {
  blank: {},
  command: { dimColor: true },
  title: { bold: true },
  note: { dimColor: true },
  'lint-pass': { color: 'success' },
  'lint-problem': { color: 'warning' },
  'lint-info': { dimColor: true },
  file: { bold: true, color: 'warning' },
  binary: { dimColor: true },
  added: {},
  removed: {},
  hunk: {},
  meta: {},
  context: {},
  omitted: { color: 'warning' },
}

export const register: Register = (on, options) => {
  const scrollRows = scrollRowsFrom(options)
  // The pane's text width at its last drawing; `ui.scroll` does not carry it.
  let paneColumns = DEFAULT_PANE_COLUMNS

  on('session.start', async ($, e, next) => {
    await $.command.register({
      name: COMMAND,
      description: 'Show the last diff the terry gate asked you to approve',
    })
    await refreshGate($)
    $.clock.every(GATE_POLL_MS, () => refreshGate($))
    return next(e)
  })

  on('command.run', { command: COMMAND }, async $ => {
    if ((await read($, review)) === null) return { text: 'Terry has not reviewed a diff in this session yet.' }
    await $.ui.open({ id: PANE, title: PANE_TITLE })
    return { text: 'Diff review pane opened.' }
  })

  // While a git or gh call waits (the gate runs, then the permission prompt), watch for
  // the manifest the gate writes, and show that call's diff in the pane.
  // This is not `classic.PreToolUse`: a built-in plugin above may pass that event on
  // past this one. A prompt-stage pane belongs to the call and closes once the call is
  // done; a pane-stage pane stays open for the user's Approve or Reject.
  on('tool.call', { tool: 'Bash' }, async ($, e, next) => {
    if (!GIT_COMMAND_HINT.test(e.command)) return next(e)
    const path = await manifestPath($, e.tool_use_id)
    if (path === null) return next(e)

    const startedAt = await $.clock.now()
    let isShown = false
    const watch = $.clock.every(MANIFEST_POLL_MS, async () => {
      if (isShown) return
      if ((await $.clock.now()) - startedAt > MANIFEST_WAIT_MS) return watch.cancel()
      if (!(await $.fs.exists(path))) return
      isShown = true
      watch.cancel()
      await showReview($, e.tool_use_id)
    })
    try {
      return await next(e)
    } finally {
      watch.cancel()
      // A pane-stage gate denies at once, often before the first poll; look one last time.
      if (!isShown && (await $.fs.exists(path))) {
        isShown = true
        await showReview($, e.tool_use_id, 'pane')
      }
      const current = await read($, review)
      if (current?.toolUseId === e.tool_use_id && current.stage === 'prompt') await $.ui.close({ id: PANE })
    }
  })

  // A prompt cannot be submitted while the turn that holds the command runs; send it after.
  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    const command = e.agentId === undefined ? await read($, promptReviewFor) : null
    if (command !== null) {
      await update($, promptReviewFor, () => null)
      void $.prompt.submit({ text: promptReviewRequest(command) })
    }
    return result
  })

  // The pane scrolls its own rows under a fixed header, so the engine's window never moves: this
  // hook answers without `next` and redraws from the pane's own place. Wheel ticks move
  // `scrollRows` rows; keys keep their row or page; Home and End jump.
  on('ui.scroll', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const current = await read($, review)
    if (current === null) return {}
    const totalRows = buildRows(current, paneColumns).length
    const windowRows = windowRowsOf(current, e.bodyRows)
    const move = { by: e.by, contentRows: e.contentRows, isWheel: e.pointer !== undefined }
    await update($, diffRow, place => {
      const moved = nextOffset(place, move, scrollRows, totalRows, windowRows)
      return Number.isInteger(moved) ? moved : place
    })
    return {}
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Button, Text, Code } = $.ui.resolve(e)
    const current = await read($, review)
    if (current === null) {
      return <Text dimColor>No diff yet. Terry shows one here when Claude asks to commit or open a PR.</Text>
    }

    paneColumns = e.props.bodyColumns
    const rows = buildRows(current, paneColumns)
    const windowRows = windowRowsOf(current, e.props.scroll.bodyRows)
    const first = Math.min(await read($, diffRow), lastOffset(rows.length, windowRows))
    const shown = rows.slice(first, first + windowRows)
    const column = Math.min(await read($, diffColumn), lastColumn(rows))
    const isPending = current.stage === 'pane' && current.decision === 'pending'

    return (
      <Box flexDirection="column">
        {isPending && (
          <Box flexDirection="column">
            <Text bold>Read the diff, then answer here. Claude waits for you.</Text>
            <Box flexDirection="row">
              <Button key="approve" hotkey="y" variant="primary" onPress={() => void approve($, current)}>
                Approve
              </Button>
              <Box marginLeft={2}>
                <Button key="reject" hotkey="n" onPress={() => void reject($, current)}>
                  Reject
                </Button>
              </Box>
              <Box marginLeft={2}>
                <Button key="left" hotkey="a" onPress={() => void moveSideways($, current, -1)}>
                  ◀
                </Button>
              </Box>
              <Box marginLeft={1}>
                <Button key="right" hotkey="d" onPress={() => void moveSideways($, current, 1)}>
                  ▶
                </Button>
              </Box>
              <Box marginLeft={2}>
                <Text dimColor>
                  rows {first + 1}-{first + shown.length} of {rows.length}
                  {column > 0 ? `, column ${column + 1}` : ''}
                </Text>
              </Box>
            </Box>
            <Text dimColor>y approve · n reject · a and d scroll sideways</Text>
          </Box>
        )}
        {current.stage === 'pane' && current.decision === 'approved' && (
          <Box flexDirection="column">
            <Text color="success">Approved. Claude runs the command again; confirm it in the permission prompt.</Text>
            <Box height={1} />
          </Box>
        )}
        {current.stage === 'pane' && current.decision === 'rejected' && (
          <Box flexDirection="column">
            <Text color="warning">Rejected. Add your reason in the prompt box and press Enter.</Text>
            <Box height={1} />
          </Box>
        )}
        {toBlocks(shown, column).map(block =>
          block.type === 'diff' ? (
            <Code format="diff" path={block.path} source={block.source} wrap="truncate-end" />
          ) : (
            <Text {...ROW_STYLES[block.row.kind]} wrap="truncate-end">
              {block.row.text === '' ? ' ' : block.row.text}
            </Text>
          ),
        )}
        {/* One row more than the window shows: the engine then has something to scroll, so its
            arrow, page and Home/End keys reach the `ui.scroll` hook above. */}
        <Text> </Text>
      </Box>
    )
  })

  // The band above the prompt: terry, at rest while the gate is on for this folder, and
  // alert, in the warning colour, while he holds a command for the user's review.
  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    if (e.props.hasSurvey || (await read($, gate)) !== 'on') return next(e)
    const { Box, Text } = $.ui.resolve(e)
    const current = await read($, review)
    const holding = isHolding(current)
    const chain = await read($, branchChain)

    // The branches from the default branch down, one colour per layer; the checked-out one is bold.
    const branches =
      chain.length === 0 ? null : (
        <Box flexDirection="row">
          {chain.map((branch, depth) => (
            <Box flexDirection="row">
              {depth > 0 && <Text dimColor>{BRANCH_SEPARATOR}</Text>}
              <Text bold={depth === chain.length - 1} color={LAYER_COLORS[depth % LAYER_COLORS.length]} wrap="truncate-end">
                {branch}
              </Text>
            </Box>
          ))}
        </Box>
      )

    const words = holding ? (
      <Box flexDirection="column">
        <Text bold color="warning">
          Terry holds a command for your review
        </Text>
        <Text dimColor wrap="truncate-end">
          $ {current.command}
        </Text>
        {branches}
        <Text dimColor>Answer in the Diff review pane (/{COMMAND} opens it).</Text>
      </Box>
    ) : (
      <Box flexDirection="column">
        <Text bold color={CALM_COLOR}>
          Terry is watching
        </Text>
        <Text dimColor>Commits and PRs wait for your review.</Text>
        {branches}
      </Box>
    )

    const art = holding ? TERRY_ALERT : TERRY_CALM
    const artColumns = Math.max(...art.map(line => line.length))
    const hasRoom =
      art.length <= e.props.maxRows && artColumns + BAND_ART_GAP + BAND_MIN_TEXT_COLUMNS <= e.props.bodyColumns
    if (!hasRoom) return words

    return (
      <Box flexDirection="row" alignItems="center">
        {/* Fixed width and no shrinking: the pane docking narrows the band, and a shrunk owl wraps. */}
        <Box flexDirection="column" marginRight={BAND_ART_GAP} flexShrink={0} width={artColumns}>
          {art.map(line => (
            <Text color={holding ? 'warning' : CALM_COLOR} wrap="truncate-end">
              {line}
            </Text>
          ))}
        </Box>
        {words}
      </Box>
    )
  })
}
