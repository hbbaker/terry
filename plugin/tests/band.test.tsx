import { expect, mock, test } from 'claude-code/testing'
import type { ProcessRunResult } from 'claude-code'
import type { TestBody } from 'claude-code/testing'

const HOME = '/home/tester'
const REVIEW_KEY = 'b'.repeat(64)
const MANIFEST_PATH = `${HOME}/.claude/diffs/calls/toolu_01.json`
const PANE_MANIFEST = {
  version: 2,
  tool_use_id: 'toolu_01',
  command: 'git push origin main',
  stage: 'pane',
  review_key: REVIEW_KEY,
  sections: [{ title: 'git push: origin/main..main', notes: [], lint: [], patch_path: null }],
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

type Session = { clock: ReturnType<typeof mock.clock>; setStatus: (next: ProcessRunResult | Error) => void }

/** Starts the session with `terry status` answering `status`; `setStatus` changes later polls. */
async function startSession(
  ...[$, on, status]: [...Parameters<TestBody>, ProcessRunResult | Error]
): Promise<Session> {
  const clock = mock.clock(on)
  let answer = status
  on('command.register', () => ({ value: { command: 'terry' } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('process.run', () => (answer instanceof Error ? { deny: answer.message } : { value: answer }))
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
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('tool.call', () => {
    files[MANIFEST_PATH] = JSON.stringify(PANE_MANIFEST)
    return { result: {} }
  })
  const { clock } = await startSession($, on, ON)

  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: 'git push origin main' })
  await clock.advance(300)

  const band = await $.ui.mount(BAND)
  expect(await band.find({ type: 'Text', text: 'Terry holds a command for your review' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: '$ git push origin main' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: /◉ ▾ ◉/ })).toBeDefined()
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
