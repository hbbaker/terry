import { atom, read, update } from 'claude-code'
import type { EngineInterface, PluginOptions, Register } from 'claude-code'

import type { DisplaySettings, GateState, PaneDecision, Review, ReviewSection, ReviewStage, TreeSnapshot } from '../types'
import { TERRY_ALERT, TERRY_CALM, TERRY_SMALL_ALERT, TERRY_SMALL_CALM } from './art'
import { fitFiles, splitPatch } from './patch'
import { buildRows, lastColumn, toBlocks } from './rows'
import type { Row } from './rows'
import { lastOffset, nextColumn, nextOffset } from './scroll'
import { FALLBACK_BINARY, binaryFromSettings } from './binary'
import { DEFAULT_DISPLAY, branchColor, parseDisplay } from './settings'
import {
  TOP_BAR_ART_GAP,
  activeLine,
  flattenRows,
  parseTree,
  repoPath,
  rowLook,
  topBar,
  withoutFirst,
} from './tree'
import type { TreeMarks } from './tree'
import { bannerRows, droppedPath, fitNote, noteRows, parseUpload, uploadMessage, uploadName } from './upload'
import type { UploadResult } from './upload'

const PANE = 'terry-diff'
const PANE_TITLE = 'Diff review'
// The mark on the Close button of a diff pane.
const CLOSE_MARK = '✕'
// The file tree pane stays open beside the transcript; the diff pane opens as a second tab.
const TREE_PANE = 'terry-tree'
const TREE_TITLE = 'Files'
// The diff of one file, opened by a click on a changed file in the tree. It only shows the diff:
// no gate waits on it, so it has no Approve or Reject.
const FILE_DIFF_PANE = 'terry-file-diff'
// Rows kept above the file Claude is on when the tree scrolls to it.
const REVEAL_ROWS_ABOVE = 2
// Blank columns at the left edge of the tree pane.
const TREE_PADDING = 2
// Columns of a file row's mark (`✎`, `·`, or none), so the names line up whatever the mark is.
const TREE_MARK_COLUMNS = 2
const TREE_TIMEOUT_MS = 10_000
// Columns one level of the branch chain moves a line to the right.
const BRANCH_INDENT = '  '
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
// A file can be large and the desktop slow; `terry upload` itself gives up after 300 s.
const UPLOAD_TIMEOUT_MS = 310_000
// The repo root, as a folder argument of `terry upload`.
const ROOT_FOLDER = '.'
const CANCEL_MARK = '✕'
// `terry status` exits 1 for an excluded folder; a child killed by a signal also reads as 1.
const GATE_OFF_PREFIX = 'terry is off'
// Shown while `terry status` cannot run at all, which is not the same as terry being off.
const UNKNOWN_GATE_MESSAGE =
  'Cannot run `terry status`. Run ./install.sh again, so the hooks in ~/.claude/settings.json name the terry binary.'
// Columns between the art and the band's words, and the fewest the words need beside the art.
const BAND_ART_GAP = 2
const BAND_MIN_TEXT_COLUMNS = 30
// Rows above the diff that do not scroll: the title, the buttons and the key hints while the gate
// waits for an answer; the status line and a blank row once the user answered; none otherwise.
const HEADER_ROWS_PENDING = 3
const HEADER_ROWS_ANSWERED = 3
// The row that holds only the Close button, in a pane with no other header.
const HEADER_ROWS_CLOSE_ONLY = 1
// Columns one press of the left or right button moves the diff sideways.
const SIDEWAYS_STEP = 20
// Columns the pane's text starts in and the width assumed before the first drawing.
const DEFAULT_PANE_COLUMNS = 80
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
const binary = atom({ plugin: 'terry', key: 'binary' } as const, FALLBACK_BINARY as string)
const branchChain = atom({ plugin: 'terry', key: 'branchChain' } as const, [])
// The name, colours and width come from `terry config` (the config file's `display` section).
const display = atom({ plugin: 'terry', key: 'display' } as const, DEFAULT_DISPLAY)
const tree = atom({ plugin: 'terry', key: 'tree' } as const, null)
const claudeTouched = atom({ plugin: 'terry', key: 'claudeTouched' } as const, [])
const claudeActive = atom({ plugin: 'terry', key: 'claudeActive' } as const, [])
const claudeLast = atom({ plugin: 'terry', key: 'claudeLast' } as const, null)
const collapsed = atom({ plugin: 'terry', key: 'collapsed' } as const, [])
const treeRow = atom({ plugin: 'terry', key: 'treeRow' } as const, 0)
const diffRow = atom({ plugin: 'terry', key: 'diffRow' } as const, 0)
const diffColumn = atom({ plugin: 'terry', key: 'diffColumn' } as const, 0)
const fileDiff = atom({ plugin: 'terry', key: 'fileDiff' } as const, null)
const fileDiffRow = atom({ plugin: 'terry', key: 'fileDiffRow' } as const, 0)
const uploadSource = atom({ plugin: 'terry', key: 'uploadSource' } as const, null)
const uploadNote = atom({ plugin: 'terry', key: 'uploadNote' } as const, null)

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
  await refreshTree($)
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
  await refreshTree($)
  await $.ui.close({ id: PANE })
  await $.prompt.submit({
    text: `terry: I approved the diff in the pane. Run the same command again, exactly as before:\n${current.command}`,
  })
}

/** The Reject button: puts a reply in the prompt box for the user to finish with a reason. */
async function reject($: EngineInterface, current: Review): Promise<void> {
  await recordDecision($, current, 'rejected')
  await refreshTree($)
  await $.prompt.fill({ text: `terry: I rejected the diff for: ${current.command}\nReason: ` })
}

/** The left and right buttons: moves the diff `direction` times `SIDEWAYS_STEP` columns. */
async function moveSideways($: EngineInterface, current: Review, direction: -1 | 1): Promise<void> {
  const last = lastColumn(buildRows(current, DEFAULT_PANE_COLUMNS))
  await update($, diffColumn, place => nextColumn(place, direction, SIDEWAYS_STEP, last))
}

/** The Close button of a diff pane: the tab title is plain text, so the click lands here. */
function closeButton($: EngineInterface, paneId: string, Button: ReturnType<EngineInterface['ui']['resolve']>['Button']) {
  return (
    <Button key="close" hotkey="x" onPress={() => void $.ui.close({ id: paneId })}>
      {`${CLOSE_MARK} Close`}
    </Button>
  )
}

/** The click on a folder: closes it, or opens it when it is closed. */
async function toggleFolder($: EngineInterface, path: string): Promise<void> {
  await update($, collapsed, paths => (paths.includes(path) ? paths.filter(closed => closed !== path) : [...paths, path]))
}

/** `terry upload`: puts the file a drop named into a folder of the repo. */
async function runUpload($: EngineInterface, source: string, folder: string): Promise<UploadResult> {
  try {
    const { stdout, stderr } = await $.process.run([await read($, binary), 'upload', source, folder], { timeoutMs: UPLOAD_TIMEOUT_MS })
    return parseUpload(stdout, stderr)
  } catch {
    // The binary cannot be run, or it did not finish in time.
    return { ok: false, error: 'terry upload did not finish' }
  }
}

/** The click on a folder (or the root) while a dropped file waits: uploads it there, then reads the tree. */
async function uploadTo($: EngineInterface, folder: string): Promise<void> {
  const source = await read($, uploadSource)
  if (source === null) return
  await update($, uploadSource, () => null)
  const result = await runUpload($, source, folder)
  await update($, uploadNote, () => uploadMessage(result))
  if (result.ok) await refreshTree($)
}

/** The Cancel button: the dropped file is not uploaded. */
async function cancelUpload($: EngineInterface): Promise<void> {
  await update($, uploadSource, () => null)
}

/** The patch of one file by `terry diff`; null when it cannot be read. */
async function readFileDiff($: EngineInterface, path: string): Promise<string | null> {
  try {
    const { exitCode, stdout, isStdoutTruncated } = await $.process.run([await read($, binary), 'diff', path], {
      timeoutMs: TREE_TIMEOUT_MS,
    })
    return exitCode === 0 && !isStdoutTruncated ? stdout : null
  } catch {
    // The binary cannot be run, or it did not finish in time.
    return null
  }
}

/** The click on a changed file: shows its diff in the file diff pane. */
async function openFileDiff($: EngineInterface, path: string): Promise<void> {
  const patch = await readFileDiff($, path)
  if (patch === null) {
    $.ui.log(`terry: cannot read the diff of ${path}`)
    return
  }
  const { shown } = fitFiles(splitPatch(patch), MAX_DRAWN_CHARACTERS)
  const section: ReviewSection = {
    title: `Diff of ${path}`,
    notes: [],
    lint: [],
    patchPath: null,
    files: shown,
    omittedFiles: 0,
  }
  await update($, fileDiff, () => ({
    toolUseId: '',
    command: '',
    stage: 'prompt_only',
    reviewKey: '',
    decision: 'pending',
    sections: [section],
  }))
  await update($, fileDiffRow, () => 0)
  await $.ui.open({ id: FILE_DIFF_PANE, title: path })
}

/**
 * What `$.ui.open` is told for the tree pane. The width is a request: the person's own width for
 * the pane wins, and a docked pane is never wider than 45% of the terminal.
 */
function treePaneOpen(settings: DisplaySettings) {
  return { id: TREE_PANE, title: TREE_TITLE, columns: settings.paneColumns } as const
}

/** The display settings by `terry config`; null when it cannot be read. */
async function readDisplay($: EngineInterface): Promise<DisplaySettings | null> {
  try {
    const { exitCode, stdout } = await $.process.run([await read($, binary), 'config'], { timeoutMs: GATE_STATUS_TIMEOUT_MS })
    return exitCode === 0 ? parseDisplay(stdout) : null
  } catch {
    // The binary cannot be run, or it did not finish in time.
    return null
  }
}

/**
 * Reads the settings again. Settings that cannot be read leave the last ones in place. A problem
 * in the config file that was not there before goes to the log, once.
 */
async function refreshDisplay($: EngineInterface): Promise<void> {
  const settings = await readDisplay($)
  if (settings === null) return
  const before = await read($, display)
  if (JSON.stringify(before) === JSON.stringify(settings)) return
  await update($, display, () => settings)
  for (const problem of settings.problems) {
    if (!before.problems.includes(problem)) $.ui.log(`config: ${problem}`)
  }
}

/** The repo's file tree by `terry tree`; null when it cannot be read. */
async function readTree($: EngineInterface): Promise<TreeSnapshot | null> {
  try {
    const { exitCode, stdout, isStdoutTruncated } = await $.process.run([await read($, binary), 'tree'], {
      timeoutMs: TREE_TIMEOUT_MS,
    })
    return exitCode === 0 && !isStdoutTruncated ? parseTree(stdout) : null
  } catch {
    // The binary cannot be run, or it did not finish in time.
    return null
  }
}

/** Reads the tree again; a tree that cannot be read leaves the last good one in place. */
async function refreshTree($: EngineInterface): Promise<void> {
  const snapshot = await readTree($)
  if (snapshot !== null) await update($, tree, () => snapshot)
}

/** After Claude's Edit or Write: remembers the file as Claude's, then reads the tree so its status shows. */
async function noteClaudeEdit($: EngineInterface, filePath: string): Promise<void> {
  const snapshot = await read($, tree)
  const path = snapshot === null ? null : repoPath(snapshot.root, filePath)
  if (path !== null) {
    await update($, claudeTouched, paths => (paths.includes(path) ? paths : [...paths, path]))
  }
  await refreshTree($)
}

// The rows the tree pane's top bar took at its last drawing, and the rows of its body. Only a
// drawing knows them; `ui.scroll` does not carry the first, and the scroll to a file needs both.
let treeBarRows = 0
let treeBodyRows = 0

/**
 * Scrolls the tree so the file Claude is on shows, when it is out of view. A file in view leaves
 * the scroll place alone, so a user who scrolls elsewhere is moved only when Claude moves on.
 */
async function revealInTree($: EngineInterface, filePath: string): Promise<void> {
  const snapshot = await read($, tree)
  const path = snapshot === null ? null : repoPath(snapshot.root, filePath)
  if (snapshot === null || path === null) return
  const rows = flattenRows(snapshot.nodes, new Set(await read($, collapsed)))
  const index = rows.findIndex(row => row.path === path)
  if (index === -1) return
  const windowRows = Math.max(1, treeBodyRows - treeBarRows)
  const first = await read($, treeRow)
  if (index >= first && index < first + windowRows) return
  await update($, treeRow, () => Math.min(Math.max(0, index - REVEAL_ROWS_ABOVE), lastOffset(rows.length, windowRows)))
}

/**
 * Runs an Edit or Write call of Claude's. The file is Claude's, and active, from the moment the
 * call starts (also while it waits for the user's permission) until it ends, even if it fails; it
 * stays Claude's colour after that, as the latest file, until another call starts or the turn
 * ends. The tree scrolls to the file.
 */
async function whileEditing<T>($: EngineInterface, filePath: string, run: () => Promise<T> | T): Promise<T> {
  const snapshot = await read($, tree)
  const path = snapshot === null ? null : repoPath(snapshot.root, filePath)
  if (path !== null) {
    await update($, claudeActive, paths => [...paths, path])
    await update($, claudeLast, () => path)
    await revealInTree($, filePath)
  }
  try {
    return await run()
  } finally {
    if (path !== null) await update($, claudeActive, paths => withoutFirst(paths, path))
    await noteClaudeEdit($, filePath)
    // A file Claude just created is in the tree only now.
    await revealInTree($, filePath)
  }
}

/**
 * The binary the gate's hooks run, read from `~/.claude/settings.json`, where `terry install`
 * writes it; the pane then runs the same terry and does not need it on PATH. `terry` (looked up
 * on PATH) when no hook names one, or the file it names is gone.
 */
async function readBinary($: EngineInterface): Promise<string> {
  try {
    const home = await $.env.get('HOME')
    if (!home) return FALLBACK_BINARY
    const settingsPath = `${home}/.claude/settings.json`
    if (!(await $.fs.exists(settingsPath))) return FALLBACK_BINARY
    const path = binaryFromSettings(await $.fs.read(settingsPath))
    return path !== null && (await $.fs.exists(path)) ? path : FALLBACK_BINARY
  } catch {
    // The settings cannot be read.
    return FALLBACK_BINARY
  }
}

/** Reads the binary again, so a new `./install.sh` shows without a restart. */
async function refreshBinary($: EngineInterface): Promise<void> {
  const path = await readBinary($)
  await update($, binary, () => path)
}

async function readGateState($: EngineInterface): Promise<GateState> {
  try {
    const { exitCode, stdout } = await $.process.run([await read($, binary), 'status'], { timeoutMs: GATE_STATUS_TIMEOUT_MS })
    if (exitCode === 0) return 'on'
    if (exitCode === 1 && stdout.startsWith(GATE_OFF_PREFIX)) return 'off'
    return 'unknown'
  } catch {
    // The binary cannot be run, or it did not finish in time.
    return 'unknown'
  }
}

/** The branch chain of the session's folder, by `terry branches`; empty when it cannot be read. */
async function readBranchChain($: EngineInterface): Promise<string[]> {
  try {
    const { exitCode, stdout } = await $.process.run([await read($, binary), 'branches'], { timeoutMs: GATE_STATUS_TIMEOUT_MS })
    if (exitCode !== 0) return []
    return stdout.split('\n').filter(branch => branch !== '')
  } catch {
    // The binary cannot be run, or it did not finish in time.
    return []
  }
}

async function refreshBranchChain($: EngineInterface): Promise<void> {
  const chain = await readBranchChain($)
  await update($, branchChain, () => chain)
}

async function refreshGate($: EngineInterface): Promise<void> {
  await refreshBinary($)
  const state = await readGateState($)
  await update($, gate, () => state)
  await refreshDisplay($)
  await refreshBranchChain($)
  await refreshTree($)
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
  if (current.stage !== 'pane') return HEADER_ROWS_CLOSE_ONLY
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
    // A reload while a call ran leaves its file marked active; nothing is running now.
    await update($, claudeActive, () => [])
    await update($, claudeLast, () => null)
    await refreshBinary($)
    await refreshDisplay($)
    const settings = await read($, display)
    await $.command.register({
      name: COMMAND,
      description: `Open the ${settings.name} file tree, and show the last diff the gate asked you to approve`,
    })
    await refreshGate($)
    // Opened once, here: a pane the user closes stays closed. /terry opens it again.
    if ((await read($, gate)) === 'on') await $.ui.open(treePaneOpen(await read($, display)))
    $.clock.every(GATE_POLL_MS, () => refreshGate($))
    return next(e)
  })

  // A file dropped on the terminal arrives as one paste into the prompt box, not as key presses.
  // The hook answers with the prompt as it was, so the path never lands in the box, and the Files
  // pane asks which folder gets the file.
  on('prompt.edit', async ($, e, next) => {
    const result = await next(e)
    const path = droppedPath(e)
    if (path === null || (await read($, gate)) !== 'on') return result
    await update($, uploadSource, () => path)
    await update($, uploadNote, () => null)
    await $.ui.open(treePaneOpen(await read($, display)))
    return { text: e.text, cursor: e.cursor }
  })

  on('command.run', { command: COMMAND }, async $ => {
    // Asks again, so a fix since the last poll (a new ./install.sh) shows at once.
    await refreshGate($)
    const settings = await read($, display)
    await $.ui.open(treePaneOpen(settings))
    if ((await read($, review)) === null) {
      // The pane opens either way, to show why it has no files; the answer must not say otherwise.
      const gateState = await read($, gate)
      if (gateState === 'unknown') return { text: `File tree opened, without files. ${UNKNOWN_GATE_MESSAGE}` }
      if (gateState === 'off') return { text: `File tree opened, without files. ${settings.name} is off in this folder.` }
      return { text: `File tree opened. ${settings.name} has not reviewed a diff in this session yet.` }
    }
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

  // Whatever a Bash call does to the files, the tree reads them again once the call is done.
  on('tool.call', { tool: 'Bash' }, async ($, e, next) => {
    const result = await next(e)
    await refreshTree($)
    return result
  })

  // Claude's own edits: the tree shows the file in Claude's colour while the call runs, and
  // keeps it marked as Claude's after.
  on('tool.call', { tool: 'Edit' }, ($, e, next) => whileEditing($, e.file_path, () => next(e)))
  on('tool.call', { tool: 'Write' }, ($, e, next) => whileEditing($, e.file_path, () => next(e)))

  // A prompt cannot be submitted while the turn that holds the command runs; send it after.
  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    // The turn is over: Claude is on no file now. A subagent's turn ending does not say so.
    if (e.agentId === undefined) await update($, claudeLast, () => null)
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
      const { name } = await read($, display)
      return <Text dimColor>{`No diff yet. ${name} shows one here when Claude asks to commit or open a PR.`}</Text>
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
        {!isPending && closeButton($, PANE, Button)}
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

  on('ui.scroll', { component: 'Pane', requestId: FILE_DIFF_PANE }, async ($, e) => {
    const current = await read($, fileDiff)
    if (current === null) return {}
    const totalRows = buildRows(current, paneColumns).length
    const windowRows = windowRowsOf(current, e.bodyRows)
    const move = { by: e.by, contentRows: e.contentRows, isWheel: e.pointer !== undefined }
    await update($, fileDiffRow, place => {
      const moved = nextOffset(place, move, scrollRows, totalRows, windowRows)
      return Number.isInteger(moved) ? moved : place
    })
    return {}
  })

  on('ui.render', { component: 'Pane', requestId: FILE_DIFF_PANE }, async ($, e) => {
    const { Box, Button, Text, Code } = $.ui.resolve(e)
    const current = await read($, fileDiff)
    if (current === null) return <Text dimColor>Click a changed file in the file tree to see its diff.</Text>

    paneColumns = e.props.bodyColumns
    const rows = buildRows(current, paneColumns)
    const windowRows = windowRowsOf(current, e.props.scroll.bodyRows)
    const first = Math.min(await read($, fileDiffRow), lastOffset(rows.length, windowRows))
    const hasChanges = current.sections.some(section => section.files.length > 0)

    return (
      <Box flexDirection="column">
        {closeButton($, FILE_DIFF_PANE, Button)}
        {!hasChanges && <Text dimColor>No changes in this file.</Text>}
        {toBlocks(rows.slice(first, first + windowRows)).map(block =>
          block.type === 'diff' ? (
            <Code format="diff" path={block.path} source={block.source} wrap="truncate-end" />
          ) : (
            <Text {...ROW_STYLES[block.row.kind]} wrap="truncate-end">
              {block.row.text === '' ? ' ' : block.row.text}
            </Text>
          ),
        )}
        {/* One row more than the window shows, so the engine's scroll keys reach `ui.scroll`. */}
        <Text> </Text>
      </Box>
    )
  })

  // The tree pane scrolls its own rows under the top bar, as the diff pane does, so the bar
  // stays where it is; this hook answers without `next` and redraws from the pane's own place.
  on('ui.scroll', { component: 'Pane', requestId: TREE_PANE }, async ($, e) => {
    const snapshot = await read($, tree)
    if (snapshot === null) return {}
    const totalRows = flattenRows(snapshot.nodes, new Set(await read($, collapsed))).length
    const windowRows = Math.max(1, e.bodyRows - treeBarRows)
    const move = { by: e.by, contentRows: e.contentRows, isWheel: e.pointer !== undefined }
    await update($, treeRow, place => {
      const moved = nextOffset(place, move, scrollRows, totalRows, windowRows)
      return Number.isInteger(moved) ? moved : place
    })
    return {}
  })

  // The tree pane: a top bar that stays put, terry (at rest, or alert while he holds a command)
  // with the branch chain, and under it the files. The changed ones are coloured by status, marked
  // by who changed them, and bold until they are committed. `terry tree` counts the changes from
  // where the branch last met what is published, so landed commits stay marked until a push.
  on('ui.render', { component: 'Pane', requestId: TREE_PANE }, async ($, e) => {
    const { Box, Button, Text } = $.ui.resolve(e)
    const settings = await read($, display)
    const gateState = await read($, gate)
    if (gateState === 'unknown') return <Text dimColor>{UNKNOWN_GATE_MESSAGE}</Text>
    if (gateState === 'off') return <Text dimColor>{`${settings.name} is off in this folder.`}</Text>
    const current = await read($, review)
    const holding = isHolding(current)
    const chain = await read($, branchChain)
    const snapshot = await read($, tree)
    const allRows = snapshot === null ? [] : flattenRows(snapshot.nodes)
    const changedCount = allRows.filter(row => !row.isDir && row.status !== 'clean').length
    const lastPath = await read($, claudeLast)
    const activePaths = [...new Set([...(await read($, claudeActive)), ...(lastPath === null ? [] : [lastPath])])]
    const editing = activeLine(activePaths)
    const marks: TreeMarks = {
      palette: settings.colors,
      hasChanges: changedCount > 0,
      collapsed: new Set(await read($, collapsed)),
      claudeTouched: new Set(await read($, claudeTouched)),
      claudeActive: new Set(activePaths),
    }

    const columns = e.props.bodyColumns - TREE_PADDING
    const art = holding ? TERRY_SMALL_ALERT : TERRY_SMALL_CALM
    const wordRows = 1 + (holding ? 1 : 0) + chain.length + (editing !== null ? 1 : 0) + (changedCount > 0 ? 1 : 0)
    const bar = topBar({
      artRows: art.length,
      artColumns: Math.max(...art.map(line => line.length)),
      wordRows,
      bodyColumns: columns,
      bodyRows: e.props.scroll.bodyRows,
    })
    // The upload banner sits under the top bar, so scrolling counts its rows as part of the bar.
    const pendingSource = await read($, uploadSource)
    const note = await read($, uploadNote)
    const banner = bannerRows(pendingSource !== null, noteRows(note, columns))
    treeBarRows = bar.rows + banner
    treeBodyRows = e.props.scroll.bodyRows

    const rows = snapshot === null ? [] : flattenRows(snapshot.nodes, marks.collapsed)
    const windowRows = Math.max(1, e.props.scroll.bodyRows - treeBarRows)
    const first = Math.min(await read($, treeRow), lastOffset(rows.length, windowRows))
    const shown = rows.slice(first, first + windowRows)

    const owl = bar.hasArt && (
      <Box flexDirection="column" flexShrink={0} marginRight={bar.isBeside ? TOP_BAR_ART_GAP : 0}>
        {art.map(line => (
          <Text color={holding ? settings.colors.alert : settings.colors.owl} wrap="truncate-end">
            {line}
          </Text>
        ))}
      </Box>
    )

    const words = (
      <Box flexDirection="column" flexShrink={1}>
        {holding ? (
          <Text bold color={settings.colors.alert} wrap="truncate-end">
            {`${settings.name} holds a command`}
          </Text>
        ) : (
          <Text bold color={settings.colors.owl} wrap="truncate-end">
            {`${settings.name} is watching`}
          </Text>
        )}
        {holding && (
          <Text dimColor wrap="truncate-end">
            Answer in the Diff review tab.
          </Text>
        )}
        {/* One branch a line, each one level further in: the pane is too narrow for a row of them. */}
        {chain.map((branch, depth) => (
          <Text bold={depth === chain.length - 1} color={branchColor(settings.colors, depth)} wrap="truncate-end">
            {depth === 0 ? branch : `${BRANCH_INDENT.repeat(depth - 1)}↳ ${branch}`}
          </Text>
        ))}
        {editing !== null && (
          <Text bold color={settings.colors.claude} wrap="truncate-end">
            {`✎ ${editing}`}
          </Text>
        )}
        {changedCount > 0 && (
          <Text dimColor wrap="truncate-end">
            {`${changedCount} changed since ${snapshot?.since ?? ''}`}
          </Text>
        )}
      </Box>
    )

    return (
      <Box flexDirection="column" marginLeft={TREE_PADDING}>
        <Box flexDirection={bar.isBeside ? 'row' : 'column'} {...(bar.isBeside ? { alignItems: 'center' as const } : {})}>
          {owl}
          {words}
        </Box>
        <Text dimColor wrap="truncate-end">
          {'─'.repeat(columns)}
        </Text>
        {pendingSource !== null && (
          <Box flexDirection="column">
            <Text bold color={settings.colors.claude} wrap="truncate-end">
              {`Upload ${uploadName(pendingSource)} to:`}
            </Text>
            <Box flexDirection="row">
              <Button key="upload-root" plain onPress={() => void uploadTo($, ROOT_FOLDER)}>
                <Text>[repo root]</Text>
              </Button>
              <Box marginLeft={2}>
                <Button key="upload-cancel" plain onPress={() => void cancelUpload($)}>
                  <Text dimColor>{`${CANCEL_MARK} Cancel`}</Text>
                </Button>
              </Box>
            </Box>
          </Box>
        )}
        {note !== null && <Text dimColor>{fitNote(note, columns)}</Text>}
        {snapshot === null ? (
          <Text dimColor>No file tree yet.</Text>
        ) : (
          shown.map(row => {
            const look = rowLook(row, marks)
            const label = `${row.name}${row.isDir ? '/' : ''}`
            const mark = look.glyph.trim()
            const guide = (
              <Box flexShrink={0}>
                <Text dimColor>{row.guide}</Text>
              </Box>
            )
            const badge = (
              <Box flexShrink={0} marginLeft={1}>
                <Text color={look.badgeColor} bold>
                  {look.badge}
                </Text>
              </Box>
            )
            // Only the name may shrink (and is cut with an ellipsis), so the names line up: the
            // guide, the mark and the letter keep their widths. A row has no fragment: the
            // engine would lay its children out in a column.
            if (row.isDir) {
              return (
                <Box flexDirection="row">
                  {guide}
                  <Box flexShrink={1}>
                    {/* A click on a folder closes or opens it; Tab and Enter do the same. While a
                        dropped file waits, the click puts the file in the folder instead. */}
                    <Button
                      key={row.path}
                      plain
                      onPress={() => void (pendingSource !== null ? uploadTo($, row.path) : toggleFolder($, row.path))}
                    >
                      <Text {...look.style} wrap="truncate-end">
                        {`${look.glyph}${label}`}
                      </Text>
                    </Button>
                  </Box>
                  {look.badge !== '' && badge}
                </Box>
              )
            }
            return (
              <Box flexDirection="row">
                {guide}
                {/* The mark has a box of its own, TREE_MARK_COLUMNS wide, whatever it is. */}
                <Box width={TREE_MARK_COLUMNS} flexShrink={0}>
                  {mark !== '' && (
                    <Text {...(look.glyphColor ? { color: look.glyphColor, bold: true } : look.style)}>{mark}</Text>
                  )}
                </Box>
                <Box flexShrink={1}>
                  {/* A click on a changed file opens its diff; a file with no change is not a button. */}
                  {row.status === 'clean' ? (
                    <Text {...look.style} wrap="truncate-end">
                      {label}
                    </Text>
                  ) : (
                    <Button key={row.path} plain onPress={() => void openFileDiff($, row.path)}>
                      <Text {...look.style} wrap="truncate-end">
                        {label}
                      </Text>
                    </Button>
                  )}
                </Box>
                {look.badge !== '' && badge}
              </Box>
            )
          })
        )}
        {/* One row more than the window shows: the engine then has something to scroll, so its
            arrow, page and Home/End keys reach the `ui.scroll` hook above. */}
        <Text> </Text>
      </Box>
    )
  })

  // The band above the prompt: terry, at rest while the gate is on for this folder, and
  // alert, in the warning colour, while he holds a command for the user's review. The tree pane
  // carries terry and the branch chain when it is drawn; the band is for a terminal too narrow
  // to place it.
  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    if (e.props.hasSurvey || (await read($, gate)) !== 'on') return next(e)
    if ((await $.ui.panes()).some(pane => pane.id === TREE_PANE && pane.isPlaced)) return next(e)
    const { Box, Text } = $.ui.resolve(e)
    const current = await read($, review)
    const holding = isHolding(current)
    const chain = await read($, branchChain)
    const settings = await read($, display)

    // The branches from the default branch down, one colour per layer; the checked-out one is bold.
    const branches =
      chain.length === 0 ? null : (
        <Box flexDirection="row">
          {chain.map((branch, depth) => (
            <Box flexDirection="row">
              {depth > 0 && <Text dimColor>{BRANCH_SEPARATOR}</Text>}
              <Text bold={depth === chain.length - 1} color={branchColor(settings.colors, depth)} wrap="truncate-end">
                {branch}
              </Text>
            </Box>
          ))}
        </Box>
      )

    const words = holding ? (
      <Box flexDirection="column">
        <Text bold color={settings.colors.alert}>
          {`${settings.name} holds a command for your review`}
        </Text>
        <Text dimColor wrap="truncate-end">
          $ {current.command}
        </Text>
        {branches}
        <Text dimColor>Answer in the Diff review pane (/{COMMAND} opens it).</Text>
      </Box>
    ) : (
      <Box flexDirection="column">
        <Text bold color={settings.colors.owl}>
          {`${settings.name} is watching`}
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
            <Text color={holding ? settings.colors.alert : settings.colors.owl} wrap="truncate-end">
              {line}
            </Text>
          ))}
        </Box>
        {words}
      </Box>
    )
  })
}
