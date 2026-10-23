import { expect, mock, test } from 'claude-code/testing'
import type { RenderSurface } from 'claude-code'
import type { TestBody } from 'claude-code/testing'

const HOME = '/home/tester'
const PATCH_PATH = `${HOME}/.claude/diffs/work-main-commit-1.patch`
const PATCH = [
  'diff --git a/app.py b/app.py',
  '--- a/app.py',
  '+++ b/app.py',
  '@@ -1 +1 @@',
  '-print("v1")',
  '+print("v2")',
  '',
].join('\n')
const REVIEW_KEY = 'a'.repeat(64)
const MANIFEST = {
  version: 2,
  tool_use_id: 'toolu_01',
  command: 'git commit -m "v2"',
  stage: 'prompt',
  review_key: REVIEW_KEY,
  sections: [
    {
      title: 'git commit: Staged changes',
      notes: [],
      lint: ['LINT passed on added lines (ruff, black)'],
      patch_path: PATCH_PATH,
    },
  ],
}
const MANIFEST_PATH = `${HOME}/.claude/diffs/calls/toolu_01.json`
const APPROVAL_PATH = `${HOME}/.claude/diffs/approvals/${REVIEW_KEY}.json`

const PANE = {
  plugin: 'terry',
  component: 'Pane',
  requestId: 'terry-diff',
  props: {
    title: 'Diff review',
    isFocused: false,
    bodyColumns: 80,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 40 },
    view: {},
  },
} as const

test('should say there is no diff when nothing was reviewed', async $ => {
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })

  expect((await ui.find({ type: 'Text', text: /No diff yet/ }))?.text).toContain(
    'commit or open a PR',
  )
  await ui.unmount()
})

test('should open the pane with the full diff once the gate asks about a commit', async ($, on) => {
  // These hooks stand for the engine beneath the plugin. The core `tool.call` stands for
  // the gate and the permission prompt: it writes the gate's manifest, then waits for
  // the person's answer until the test releases it.
  const clock = mock.clock(on)
  const files: Record<string, string> = { [PATCH_PATH]: PATCH }
  const opened: string[] = []
  const closed: string[] = []
  let answerPrompt = () => {}
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', (_, e) => ({ value: e.path in files }))
  on('fs.read', (_, e) => ({ value: files[e.path] ?? '' }))
  on('ui.open', (_, e) => (opened.push(e.id), { value: { isPlaced: true } }))
  on('ui.close', (_, e) => (closed.push(e.id), { value: undefined }))
  on('tool.call', async () => {
    files[MANIFEST_PATH] = JSON.stringify(MANIFEST)
    await new Promise<void>(resolve => (answerPrompt = resolve))
    return { result: {} }
  })

  const call = $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: 'git commit -m "v2"' })
  await clock.advance(300)

  expect(opened).toEqual(['terry-diff'])
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect(await ui.find({ type: 'Text', text: 'git commit: Staged changes' })).toBeDefined()
  expect(await ui.find({ type: 'Text', text: /LINT passed/ })).toBeDefined()
  expect(await ui.find({ type: 'Text', text: 'app.py' })).toBeDefined()
  await ui.unmount()

  answerPrompt()
  await call
  expect(closed).toEqual(['terry-diff'])
})

/**
 * Runs one git call through the plugin while the core `tool.call` (standing for the gate)
 * writes a pane-stage manifest and denies at once, before the plugin's first poll, as the
 * gate does before an approval. A later call is the rerun after an approval: the gate writes
 * a prompt-stage manifest and asks, and the call waits until the test answers the prompt.
 * `where` says which surfaces the session draws on and whether they place the pane.
 */
async function reviewInPane(
  ...[$, on, where = { isPlaced: true, surfaces: ['terminal'] }]: [
    ...Parameters<TestBody>,
    { isPlaced: boolean; surfaces: RenderSurface[]; patch?: string }?,
  ]
) {
  const clock = mock.clock(on)
  const files: Record<string, string> = { [PATCH_PATH]: where.patch ?? PATCH }
  const closed: string[] = []
  const submitted: string[] = []
  const filled: string[] = []
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', (_, e) => ({ value: e.path in files }))
  on('fs.read', (_, e) => ({ value: files[e.path] ?? '' }))
  on('fs.write', (_, e) => ((files[e.path] = e.text), { value: undefined }))
  const opened: string[] = []
  let answerPrompt = () => {}
  on('ui.open', (_, e) => {
    opened.push(e.id)
    return { value: where.isPlaced ? { isPlaced: true } : { isPlaced: false, reason: 'no surface places panes' } }
  })
  on('session.surfaces', () => ({ value: where.surfaces }))
  on('turn.complete', (_, e) => ({ text: e.answer }))
  on('ui.close', (_, e) => (closed.push(e.id), { value: undefined }))
  on('prompt.submit', (_, e) => (submitted.push(e.text), { text: e.text }))
  on('prompt.fill', (_, e) => (filled.push(e.text), { isFilled: true, text: e.text }))
  on('tool.call', async (_, e) => {
    const isRerun = e.tool_use_id !== 'toolu_01'
    files[`${HOME}/.claude/diffs/calls/${e.tool_use_id}.json`] = JSON.stringify({
      ...MANIFEST,
      tool_use_id: e.tool_use_id,
      stage: isRerun ? 'prompt' : 'pane',
    })
    if (isRerun) await new Promise<void>(resolve => (answerPrompt = resolve))
    return { result: {} }
  })

  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: MANIFEST.command })
  await clock.advance(300)
  return { files, opened, closed, submitted, filled, clock, answerPrompt: () => answerPrompt() }
}

test('should keep the pane open with Approve and Reject when the gate asks for a pane review', async ($, on) => {
  const { closed } = await reviewInPane($, on)

  expect(closed).toEqual([])
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect(await ui.find({ key: 'approve' })).toBeDefined()
  expect(await ui.find({ key: 'reject' })).toBeDefined()
  expect(await ui.find({ type: 'Text', text: 'app.py' })).toBeDefined()
  await ui.unmount()
})

const LONG_PATCH = [
  'diff --git a/big.py b/big.py',
  '--- a/big.py',
  '+++ b/big.py',
  '@@ -0,0 +1,100 @@',
  ...Array.from({ length: 100 }, (_, index) => `+LINE_${index} = ${index}`),
  '',
].join('\n')
const SHORT_PANE = { ...PANE, props: { ...PANE.props, scroll: { offset: 0, bodyRows: 20 } } }

test('should draw Approve and Reject above the first diff rows when the pane opens', async ($, on) => {
  await reviewInPane($, on, { isPlaced: true, surfaces: ['terminal'], patch: LONG_PATCH })
  const ui = await $.ui.mount({ ...SHORT_PANE, surface: 'terminal' })

  const drawn = JSON.stringify(await ui.drawn())

  expect(drawn.indexOf('"key":"approve"')).toBeGreaterThan(-1)
  expect(drawn.indexOf('"key":"approve"')).toBeLessThan(drawn.indexOf('+LINE_0 = 0'))
  expect(await ui.find({ type: 'Text', text: /rows 1-\d+ of \d+/ })).toBeDefined()
  await ui.unmount()
})

test('should draw only as many diff rows as fit under the header', async ($, on) => {
  await reviewInPane($, on, { isPlaced: true, surfaces: ['terminal'], patch: LONG_PATCH })
  const ui = await $.ui.mount({ ...SHORT_PANE, surface: 'terminal' })

  const lines = await ui.findAll({ type: 'Text', text: /^\+LINE_/ })

  // 20 rows less the 3 of the header: the command, a blank, the title and the lint line take
  // the first rows of the 17, so fewer than 17 diff lines show.
  expect(lines.length).toBeGreaterThan(0)
  expect(lines.length).toBeLessThanOrEqual(17)
  await ui.unmount()
})

test('should write the approval, close the pane and tell Claude to run the command again when Approve is pressed', async ($, on) => {
  const { files, closed, submitted } = await reviewInPane($, on)

  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  await ui.press({ key: 'approve' })

  expect(JSON.parse(files[APPROVAL_PATH] ?? '{}').via).toBe('pane')
  expect(closed).toEqual(['terry-diff'])
  expect(submitted).toHaveLength(1)
  expect(submitted[0]).toContain('git commit -m "v2"')
  expect(await ui.find({ key: 'approve' })).toBeUndefined()
  await ui.unmount()
})

test('should keep the pane closed during the permission prompt of the approved rerun', async ($, on) => {
  const { opened, clock, answerPrompt } = await reviewInPane($, on)
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  await ui.press({ key: 'approve' })
  await ui.unmount()

  const rerun = $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_02', command: MANIFEST.command })
  await clock.advance(300)

  expect(opened).toEqual(['terry-diff'])
  answerPrompt()
  await rerun
})

test('should send the review to the permission prompt after the turn when no attached surface can draw the pane', async ($, on) => {
  const { files, closed, submitted } = await reviewInPane($, on, { isPlaced: false, surfaces: ['vscode'] })
  expect(JSON.parse(files[APPROVAL_PATH] ?? '{}').via).toBe('prompt')
  expect(submitted).toEqual([])

  await $.turn.complete({
    reason: 'answer',
    answer: 'Please review the diff in the pane.',
    durationMs: 1000,
    isAborted: false,
    turnId: 'turn_01',
  })

  expect(submitted).toHaveLength(1)
  expect(submitted[0]).toContain('permission prompt will show me the full diff')
  expect(submitted[0]).toContain('git commit -m "v2"')
  expect(closed).toEqual(['terry-diff'])
})

test('should leave the pane waiting for /terry when a narrow terminal cannot place it yet', async ($, on) => {
  const { files, submitted } = await reviewInPane($, on, { isPlaced: false, surfaces: ['terminal'] })

  expect(APPROVAL_PATH in files).toBe(false)
  expect(submitted).toEqual([])
})

test('should not open the pane when the gate reviews the call in the permission prompt alone', async ($, on) => {
  const clock = mock.clock(on)
  const files: Record<string, string> = { [PATCH_PATH]: PATCH }
  const opened: string[] = []
  let answerPrompt = () => {}
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', (_, e) => ({ value: e.path in files }))
  on('fs.read', (_, e) => ({ value: files[e.path] ?? '' }))
  on('ui.open', (_, e) => (opened.push(e.id), { value: { isPlaced: true } }))
  on('tool.call', async () => {
    files[MANIFEST_PATH] = JSON.stringify({ ...MANIFEST, stage: 'prompt_only' })
    await new Promise<void>(resolve => (answerPrompt = resolve))
    return { result: {} }
  })

  const call = $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: MANIFEST.command })
  await clock.advance(300)

  expect(opened).toEqual([])
  answerPrompt()
  await call
})

test('should put a reject reply in the prompt box and write no approval when Reject is pressed', async ($, on) => {
  const { files, submitted, filled } = await reviewInPane($, on)

  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  await ui.press({ key: 'reject' })

  expect(Object.keys(files).some(path => path.includes('/approvals/'))).toBe(false)
  expect(submitted).toEqual([])
  expect(filled[0]).toContain('rejected')
  await ui.unmount()
})

test('should not watch for a manifest when the command names neither git nor gh', async ($, on) => {
  const clock = mock.clock(on)
  const checked: string[] = []
  on('fs.exists', (_, e) => (checked.push(e.path), { value: false }))
  on('tool.call', () => ({ result: {} }))

  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_02', command: 'ls -la' })
  await clock.advance(1000)

  expect(checked).toEqual([])
})

test('should leave the pane closed when the gate writes no manifest', async ($, on) => {
  const clock = mock.clock(on)
  const opened: string[] = []
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', () => ({ value: false }))
  on('ui.open', (_, e) => (opened.push(e.id), { value: { isPlaced: true } }))
  on('tool.call', () => ({ result: {} }))

  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_03', command: 'git status' })
  await clock.advance(1000)

  expect(opened).toEqual([])
})
