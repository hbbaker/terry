import { expect, mock, test } from 'claude-code/testing'
import type { ProcessRunResult } from 'claude-code'
import type { TestBody } from 'claude-code/testing'

const HOME = '/home/tester'
const TREE_PANE = 'terry-tree'
const REVIEW_KEY = 'b'.repeat(64)
const MANIFEST_PATH = `${HOME}/.claude/diffs/calls/toolu_01.json`
const PANE_MANIFEST = {
  version: 2,
  tool_use_id: 'toolu_01',
  command: 'git commit -m x',
  stage: 'pane',
  review_key: REVIEW_KEY,
  sections: [{ title: 'git commit: Staged changes', notes: [], lint: [], patch_path: null }],
}

const BAND = {
  plugin: 'terry',
  component: 'AbovePrompt',
  surface: 'terminal',
  props: {
    hasSurvey: false,
    isWorking: false,
    maxRows: 20,
    bodyColumns: 100,
    scroll: { offset: 0, bodyRows: 19 },
    view: {},
  },
} as const

const ON: ProcessRunResult = {
  exitCode: 0,
  stdout: 'terry is on in /work\n',
  stderr: '',
  isStdoutTruncated: false,
  isStderrTruncated: false,
}
const OFF: ProcessRunResult = { ...ON, exitCode: 1, stdout: 'terry is off in /work because /work is excluded\n' }
const KILLED: ProcessRunResult = { ...ON, exitCode: 1, stdout: '' }

const NO_BRANCHES: ProcessRunResult = { ...ON, stdout: '' }

/** What `terry branches` prints for a chain: one branch per line, root first. */
function branchesOf(...chain: string[]): ProcessRunResult {
  return { ...ON, stdout: chain.map(branch => `${branch}\n`).join('') }
}

type Session = { clock: ReturnType<typeof mock.clock>; setStatus: (next: ProcessRunResult | Error) => void }

/**
 * Starts the session with `terry status` answering `status` and `terry branches` answering
 * `branches`; `setStatus` changes later polls.
 */
async function startSession(
  ...[$, on, status, branches = NO_BRANCHES]: [...Parameters<TestBody>, ProcessRunResult | Error, (ProcessRunResult | Error)?]
): Promise<Session> {
  const clock = mock.clock(on)
  let answer = status
  on('command.register', () => ({ value: { command: 'terry' } }))
  // A terminal too narrow for the tree pane: it waits undrawn, so the band is drawn.
  on('ui.open', (_, e) => ({ value: e.id === TREE_PANE ? { isPlaced: false, reason: 'too-narrow' } : { isPlaced: true } }))
  on('ui.panes', () => ({ value: [] }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('process.run', (_, e) => {
    const result = e.argv.includes('branches') ? branches : answer
    return result instanceof Error ? { deny: result.message } : { value: result }
  })
  // The engine's own band, drawn when the plugin passes: an empty box.
  on('ui.render', ($, e) => {
    const { Box } = $.ui.resolve(e)
    return <Box />
  })
  await $.session.start({ cwd: '/work', surface: 'terminal', isInteractive: true })
  return { clock, setStatus: next => (answer = next) }
}

test('should show terry watching when the gate is on for the folder', async ($, on) => {
  await startSession($, on, ON)

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: /◠ ▾ ◠/ })).toBeDefined()
  await band.unmount()
})

test('should show each branch of the chain in its own colour when the folder is in a repo', async ($, on) => {
  await startSession($, on, ON, branchesOf('main', 'dev', 'subfeature'))

  const band = await $.ui.mount(BAND)

  const main = await band.find({ type: 'Text', text: 'main' })
  const dev = await band.find({ type: 'Text', text: 'dev' })
  const subfeature = await band.find({ type: 'Text', text: 'subfeature' })
  expect(main?.props.color).toBeDefined()
  expect(new Set([main?.props.color, dev?.props.color, subfeature?.props.color]).size).toBe(3)
  expect(subfeature?.props.bold).toBe(true)
  expect(main?.props.bold).toBeFalsy()
  await band.unmount()
})

test('should show no branch row when terry branches prints nothing', async ($, on) => {
  await startSession($, on, ON, NO_BRANCHES)

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: ' -> ' })).toBeUndefined()
  expect(await band.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  await band.unmount()
})

test('should keep the band when terry branches fails', async ($, on) => {
  await startSession($, on, ON, new Error('spawn terry ENOENT'))

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  await band.unmount()
})

test('should show the branch chain under the held command when terry waits for a review', async ($, on) => {
  const files: Record<string, string> = {}
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', (_, e) => ({ value: e.path in files }))
  on('fs.read', (_, e) => ({ value: files[e.path] ?? '' }))
  on('tool.call', () => {
    files[MANIFEST_PATH] = JSON.stringify(PANE_MANIFEST)
    return { result: {} }
  })
  const { clock } = await startSession($, on, ON, branchesOf('main', 'dev'))

  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: 'git commit -m x' })
  await clock.advance(300)

  const band = await $.ui.mount(BAND)
  expect(await band.find({ type: 'Text', text: 'Terry holds a command for your review' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: 'dev' })).toBeDefined()
  await band.unmount()
})

test('should hide the band when the folder is excluded', async ($, on) => {
  await startSession($, on, OFF)

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})

test('should hide the band when terry is not installed', async ($, on) => {
  await startSession($, on, new Error('spawn terry ENOENT'))

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})

test('should hide the band when terry status is killed', async ($, on) => {
  await startSession($, on, KILLED)

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})

test('should hide the band when the folder is excluded during the session', async ($, on) => {
  const { clock, setStatus } = await startSession($, on, ON)

  setStatus(OFF)
  await clock.advance(15_000)

  const band = await $.ui.mount(BAND)
  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})

test('should show the held command when terry waits for a review in the pane', async ($, on) => {
  const files: Record<string, string> = {}
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', (_, e) => ({ value: e.path in files }))
  on('fs.read', (_, e) => ({ value: files[e.path] ?? '' }))
  on('tool.call', () => {
    files[MANIFEST_PATH] = JSON.stringify(PANE_MANIFEST)
    return { result: {} }
  })
  const { clock } = await startSession($, on, ON)

  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: 'git commit -m x' })
  await clock.advance(300)

  const band = await $.ui.mount(BAND)
  expect(await band.find({ type: 'Text', text: 'Terry holds a command for your review' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: '$ git commit -m x' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: /◉ ▾ ◉/ })).toBeDefined()
  await band.unmount()
})

test('should keep the owl at full width when the band is narrow', async ($, on) => {
  await startSession($, on, ON)

  const band = await $.ui.mount({ ...BAND, props: { ...BAND.props, bodyColumns: 60 } })

  const drawn = JSON.stringify(await band.drawn())
  expect(drawn).toContain('"flexShrink":0')
  expect(await band.find({ type: 'Text', text: /◠ ▾ ◠/ })).toBeDefined()
  await band.unmount()
})

test('should show only the words when the band is too short for the art', async ($, on) => {
  await startSession($, on, ON)

  const band = await $.ui.mount({ ...BAND, props: { ...BAND.props, maxRows: 3 } })

  expect(await band.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: /◠ ▾ ◠/ })).toBeUndefined()
  await band.unmount()
})

test('should yield the band to a survey', async ($, on) => {
  await startSession($, on, ON)

  const band = await $.ui.mount({ ...BAND, props: { ...BAND.props, hasSurvey: true } })

  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})
