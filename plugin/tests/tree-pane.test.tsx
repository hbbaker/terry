import { expect, mock, test } from 'claude-code/testing'
import type { ProcessRunResult } from 'claude-code'
import type { TestBody } from 'claude-code/testing'

const HOME = '/home/tester'
const MANIFEST_PATH = `${HOME}/.claude/diffs/calls/toolu_01.json`
const MANIFEST = {
  version: 2,
  tool_use_id: 'toolu_01',
  command: 'git commit -m x',
  stage: 'pane',
  review_key: 'c'.repeat(64),
  sections: [{ title: 'git commit: Staged changes', notes: [], lint: [], patch_path: null }],
}

const TREE_PANE = {
  plugin: 'terry',
  component: 'Pane',
  requestId: 'terry-tree',
  surface: 'terminal',
  props: {
    title: 'Files',
    isFocused: false,
    bodyColumns: 40,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 40 },
    view: {},
  },
} as const

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

const TREE: ProcessRunResult = {
  ...ON,
  stdout: JSON.stringify({
    root: '/work',
    branch: 'dev',
    tree: [
      { dir: 'src', children: [{ file: 'main.rs', status: 'modified' }] },
      { file: 'README.md', status: 'clean' },
    ],
  }),
}
const BRANCHES: ProcessRunResult = { ...ON, stdout: 'main\ndev\n' }

type SessionOptions = {
  /** What `terry status` answers. */
  status?: ProcessRunResult
  /** The panes the engine reports as open. */
  panes?: readonly { id: string; title: string; isShown: boolean; isFocused: boolean; isPlaced: boolean }[]
  /** Collects the id of every pane the plugin opens. */
  opened?: string[]
  /** What `terry tree` answers. */
  tree?: ProcessRunResult
}

/** Starts the session with the files of a repo on `dev`. */
async function startSession(
  ...[$, on, { status = ON, panes = [], opened = [], tree = TREE } = {}]: [...Parameters<TestBody>, SessionOptions?]
): Promise<ReturnType<typeof mock.clock>> {
  const clock = mock.clock(on)
  const files: Record<string, string> = {}
  on('env.get', () => ({ value: HOME }))
  on('fs.exists', (_, e) => ({ value: e.path in files }))
  on('fs.read', (_, e) => ({ value: files[e.path] ?? '' }))
  on('command.register', () => ({ value: { command: 'terry' } }))
  on('ui.open', (_, e) => (opened.push(`${e.id}:${e.columns ?? ''}`), { value: { isPlaced: true } }))
  on('ui.panes', () => ({ value: panes }))
  // The engine's own band, drawn when the plugin passes: an empty box.
  on('ui.render', ($, e) => {
    const { Box } = $.ui.resolve(e)
    return <Box />
  })
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('process.run', (_, e) => {
    if (e.argv.includes('tree')) return { value: tree }
    return { value: e.argv.includes('branches') ? BRANCHES : status }
  })
  // The core of `tool.call` stands for the gate: a git command leaves its manifest.
  on('tool.call', (_, e) => {
    if (e.tool === 'Bash') files[MANIFEST_PATH] = JSON.stringify(MANIFEST)
    return { result: {} }
  })
  await $.session.start({ cwd: '/work', surface: 'terminal', isInteractive: true })
  return clock
}

/** Lets the gate hold a commit, so a diff was reviewed in this session. */
async function reviewACommit($: Parameters<TestBody>[0], clock: ReturnType<typeof mock.clock>): Promise<void> {
  await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_01', command: 'git commit -m x' })
  await clock.advance(300)
}

test('should draw the files without colour when no diff was reviewed yet', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount(TREE_PANE)

  const modified = await pane.find({ type: 'Text', text: /main\.rs/ })
  expect(modified).toBeDefined()
  expect(modified?.props.color).toBeUndefined()
  expect(await pane.find({ type: 'Text', text: /src\// })).toBeDefined()
  await pane.unmount()
})

test('should draw terry and the branch chain above the files', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /◠▾◠/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: 'main' })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: '↳ dev' })).toBeDefined()
  await pane.unmount()
})

test('should colour a changed file by its status when a diff was reviewed', async ($, on) => {
  const clock = await startSession($, on)
  await reviewACommit($, clock)

  const pane = await $.ui.mount(TREE_PANE)

  const modified = await pane.find({ type: 'Text', text: /main\.rs/ })
  expect(modified?.props.color).toBe('warning')
  expect(modified?.text).toContain('· main.rs')
  expect((await pane.find({ type: 'Text', text: /README\.md/ }))?.props.dimColor).toBe(true)
  await pane.unmount()
})

test('should mark a file as Claude edited when Claude edits it after a diff was reviewed', async ($, on) => {
  const clock = await startSession($, on)
  await reviewACommit($, clock)

  await $.tool.call({
    tool: 'Edit',
    tool_use_id: 'toolu_02',
    file_path: '/work/src/main.rs',
    old_string: 'a',
    new_string: 'b',
  })

  const pane = await $.ui.mount(TREE_PANE)
  expect((await pane.find({ type: 'Text', text: /main\.rs/ }))?.text).toContain('✎ main.rs')
  await pane.unmount()
})

test('should say terry is off when the folder is excluded', async ($, on) => {
  await startSession($, on, { status: OFF })

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: 'Terry is off in this folder.' })).toBeDefined()
  await pane.unmount()
})

test('should open the tree pane once when the session starts and the gate is on', async ($, on) => {
  const opened: string[] = []
  await startSession($, on, { opened })

  expect(opened.filter(entry => entry.startsWith('terry-tree'))).toHaveLength(1)
})

test('should leave the band to the tree pane when the tree pane is drawn', async ($, on) => {
  const panes = [{ id: 'terry-tree', title: 'Files', isShown: true, isFocused: false, isPlaced: true }]
  await startSession($, on, { panes })

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})

test('should hide the files of a folder when its row is pressed, and show them when pressed again', async ($, on) => {
  await startSession($, on)
  const pane = await $.ui.mount(TREE_PANE)
  expect(await pane.find({ type: 'Text', text: /main\.rs/ })).toBeDefined()

  await pane.press({ key: 'src' })

  expect(await pane.find({ type: 'Text', text: /main\.rs/ })).toBeUndefined()
  expect(await pane.find({ type: 'Text', text: /▸ src\// })).toBeDefined()

  await pane.press({ key: 'src' })

  expect(await pane.find({ type: 'Text', text: /main\.rs/ })).toBeDefined()
  await pane.unmount()
})

test('should ask for a narrow pane when it opens the tree pane', async ($, on) => {
  const opened: string[] = []
  await startSession($, on, { opened })

  expect(opened).toContain('terry-tree:34')
})

test('should keep the top bar and draw only the rows that fit under it when the tree is longer than the pane', async ($, on) => {
  const many = Array.from({ length: 40 }, (_, index) => ({
    file: `file-${String(index).padStart(2, '0')}.rs`,
    status: 'clean',
  }))
  await startSession($, on, {
    tree: { ...TREE, stdout: JSON.stringify({ root: '/work', branch: 'dev', tree: many }) },
  })

  const pane = await $.ui.mount({ ...TREE_PANE, props: { ...TREE_PANE.props, scroll: { offset: 0, bodyRows: 20 } } })

  expect(await pane.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /file-00\.rs/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /file-39\.rs/ })).toBeUndefined()
  await pane.unmount()
})

test('should draw terry beside his words when the pane is wide', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount({ ...TREE_PANE, props: { ...TREE_PANE.props, bodyColumns: 40 } })

  expect(JSON.stringify(await pane.drawn())).toContain('"alignItems":"center"')
  await pane.unmount()
})

test('should draw terry above his words when the pane is narrow', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount({ ...TREE_PANE, props: { ...TREE_PANE.props, bodyColumns: 24 } })

  expect(JSON.stringify(await pane.drawn())).not.toContain('"alignItems":"center"')
  expect(await pane.find({ type: 'Text', text: /◠▾◠/ })).toBeDefined()
  await pane.unmount()
})
