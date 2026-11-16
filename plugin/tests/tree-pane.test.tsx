import { expect, mock, test } from 'claude-code/testing'
import type { ProcessRunResult } from 'claude-code'
import type { TestBody } from 'claude-code/testing'

import { DEFAULT_DISPLAY } from '../hooks/settings'

const HOME = '/home/tester'

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
    since: 'origin/dev',
    tree: [
      { dir: 'src', children: [{ file: 'main.rs', status: 'modified', landed: false }] },
      { file: 'README.md', status: 'clean', landed: false },
    ],
  }),
}

/** What `terry tree` answers for a repo where nothing changed since the base. */
const CLEAN_TREE: ProcessRunResult = {
  ...ON,
  stdout: JSON.stringify({
    root: '/work',
    branch: 'dev',
    since: 'origin/dev',
    tree: [{ file: 'README.md', status: 'clean', landed: false }],
  }),
}
/** What `terry diff src/main.rs` answers. */
const FILE_DIFF: ProcessRunResult = {
  ...ON,
  stdout: ['diff --git a/src/main.rs b/src/main.rs', '--- a/src/main.rs', '+++ b/src/main.rs', '@@ -1 +1 @@', '-old', '+new', ''].join('\n'),
}
/** What `terry upload` answers when the file landed in `src`. */
const UPLOADED: ProcessRunResult = { ...ON, stdout: JSON.stringify({ ok: true, dest: 'src/shot.png' }) }
/** The paste a terminal makes when a file of a Windows desktop is dropped on it. */
const DROP = {
  origin: { kind: 'composer' },
  text: '',
  cursor: 0,
  start: 0,
  end: 0,
  inputText: '"D:\\Downloads\\shot.png"',
} as const
const BRANCHES: ProcessRunResult = { ...ON, stdout: 'main\ndev\n' }

/** What `terry config` answers for these display settings. */
function configOf(change: Record<string, unknown> = {}): ProcessRunResult {
  return { ...ON, stdout: JSON.stringify({ ...DEFAULT_DISPLAY, ...change }) }
}

type SessionOptions = {
  /** What `terry status` answers. */
  status?: ProcessRunResult
  /** The panes the engine reports as open. */
  panes?: readonly { id: string; title: string; isShown: boolean; isFocused: boolean; isPlaced: boolean }[]
  /** Collects the id of every pane the plugin opens. */
  opened?: string[]
  /** What `terry tree` answers. */
  tree?: ProcessRunResult
  /** The core of `tool.call` waits for this before it answers: a call that is still running. */
  hold?: Promise<void>
  /** What `terry config` answers. */
  config?: ProcessRunResult
  /** Collects every line the plugin writes to the log. */
  logs?: string[]
  /** What `terry diff` answers. */
  diff?: ProcessRunResult
  /** What `terry upload` answers. */
  upload?: ProcessRunResult
  /** Collects the argv of every `terry upload` call. */
  uploads?: string[][]
  /** Collects the argv of every process the plugin runs. */
  calls?: string[][]
}

/** Starts the session with the files of a repo on `dev`. */
async function startSession(
  ...[$, on, { status = ON, panes = [], opened = [], tree = TREE, hold = Promise.resolve(), config = configOf(), logs = [], diff = FILE_DIFF, upload = UPLOADED, uploads = [], calls = [] } = {}]: [
    ...Parameters<TestBody>,
    SessionOptions?,
  ]
): Promise<ReturnType<typeof mock.clock>> {
  const clock = mock.clock(on)
  on('env.get', () => ({ value: HOME }))
  on('command.register', () => ({ value: { command: 'terry' } }))
  on('ui.open', (_, e) => (opened.push(`${e.id}:${e.columns ?? ''}`), { value: { isPlaced: true } }))
  on('ui.panes', () => ({ value: panes }))
  on('ui.log', (_, e) => (logs.push(e.text), { value: undefined }))
  on('turn.complete', (_, e) => ({ text: e.answer }))
  // The engine's own band, drawn when the plugin passes: an empty box.
  on('ui.render', ($, e) => {
    const { Box } = $.ui.resolve(e)
    return <Box />
  })
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  // The engine's own edit: the typed or pasted text goes into the prompt box.
  on('prompt.edit', (_, e) => ({ text: e.text + e.inputText, cursor: e.cursor + e.inputText.length }))
  on('process.run', (_, e) => {
    calls.push([...e.argv])
    if (e.argv.includes('upload')) return (uploads.push([...e.argv]), { value: upload })
    if (e.argv.includes('tree')) return { value: tree }
    if (e.argv.includes('config')) return { value: config }
    if (e.argv.includes('diff')) return { value: diff }
    return { value: e.argv.includes('branches') ? BRANCHES : status }
  })
  on('tool.call', async () => {
    await hold
    return { result: {} }
  })
  await $.session.start({ cwd: '/work', surface: 'terminal', isInteractive: true })
  return clock
}

test('should draw the files without colour when nothing changed since the base', async ($, on) => {
  await startSession($, on, { tree: CLEAN_TREE })

  const pane = await $.ui.mount(TREE_PANE)

  const readme = await pane.find({ type: 'Text', text: /README\.md/ })
  expect(readme).toBeDefined()
  expect(readme?.props.color).toBeUndefined()
  expect(readme?.props.dimColor).toBeFalsy()
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

test('should colour a changed file by its status with no diff reviewed in the session', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount(TREE_PANE)

  const modified = await pane.find({ type: 'Text', text: /main\.rs/ })
  expect(modified?.props.color).toBe('warning')
  expect(modified?.props.bold).toBe(true)
  expect(await pane.find({ type: 'Text', text: '·' })).toBeDefined()
  expect((await pane.find({ type: 'Text', text: /README\.md/ }))?.props.dimColor).toBe(true)
  await pane.unmount()
})

test('should draw a file dim when a commit landed its change', async ($, on) => {
  const landed = {
    ...ON,
    stdout: JSON.stringify({
      root: '/work',
      branch: 'dev',
      since: 'origin/dev',
      tree: [{ file: 'main.rs', status: 'modified', landed: true }],
    }),
  }
  await startSession($, on, { tree: landed })

  const pane = await $.ui.mount(TREE_PANE)

  const modified = await pane.find({ type: 'Text', text: /main\.rs/ })
  expect(modified?.props.color).toBe('warning')
  expect(modified?.props.dimColor).toBe(true)
  expect(modified?.props.bold).toBeFalsy()
  await pane.unmount()
})

test('should say how many files changed and what they changed since', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: '1 changed since origin/dev' })).toBeDefined()
  await pane.unmount()
})

test('should keep the file in the Claude colour with an orange mark after the call, until the turn ends', async ($, on) => {
  await startSession($, on)

  await $.tool.call({
    tool: 'Edit',
    tool_use_id: 'toolu_02',
    file_path: '/work/src/main.rs',
    old_string: 'a',
    new_string: 'b',
  })

  const during = await $.ui.mount(TREE_PANE)
  expect((await during.find({ type: 'Text', text: '✎' }))?.props.color).toBe('claude')
  expect((await during.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('claude')
  await during.unmount()

  await $.turn.complete({ reason: 'answer', answer: 'done', durationMs: 1, isAborted: false, turnId: 'turn_01' })
  const after = await $.ui.mount(TREE_PANE)
  expect((await after.find({ type: 'Text', text: '✎' }))?.props.color).toBe('claude')
  expect((await after.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('warning')
  await after.unmount()
})

test('should show the file in the Claude colour while an Edit call runs, and in the top bar', async ($, on) => {
  let finish = () => {}
  const hold = new Promise<void>(resolve => (finish = resolve))
  const clock = await startSession($, on, { hold })

  const call = $.tool.call({
    tool: 'Edit',
    tool_use_id: 'toolu_03',
    file_path: '/work/src/main.rs',
    old_string: 'a',
    new_string: 'b',
  })
  await clock.advance(10)
  const during = await $.ui.mount(TREE_PANE)

  expect((await during.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('claude')
  expect(await during.find({ type: 'Text', text: '✎ Claude is editing main.rs' })).toBeDefined()
  await during.unmount()

  finish()
  await call
  await $.turn.complete({ reason: 'answer', answer: 'done', durationMs: 1, isAborted: false, turnId: 'turn_01' })
  const after = await $.ui.mount(TREE_PANE)

  expect(await after.find({ type: 'Text', text: /Claude is editing/ })).toBeUndefined()
  await after.unmount()
})

test('should stop showing the file as edited when the turn ends, also after a failed call', async ($, on) => {
  const hold = Promise.reject(new Error('the call failed'))
  hold.catch(() => undefined)
  const clock = await startSession($, on, { hold })

  await $.tool
    .call({
      tool: 'Edit',
      tool_use_id: 'toolu_04',
      file_path: '/work/src/main.rs',
      old_string: 'a',
      new_string: 'b',
    })
    .catch(() => undefined)
  await clock.advance(10)
  await $.turn.complete({ reason: 'answer', answer: 'done', durationMs: 1, isAborted: false, turnId: 'turn_01' })

  const pane = await $.ui.mount(TREE_PANE)
  expect(await pane.find({ type: 'Text', text: /Claude is editing/ })).toBeUndefined()
  await pane.unmount()
})

test('should draw the status letter right after the name, with no row that grows', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount(TREE_PANE)

  expect(JSON.stringify(await pane.drawn())).not.toContain('"flexGrow"')
  expect(await pane.find({ type: 'Text', text: 'M' })).toBeDefined()
  await pane.unmount()
})

type Drawn = { type: string; props?: Record<string, unknown>; children?: Array<Drawn | string> }

/** Every element in the drawn tree, depth first. */
function elementsOf(node: Drawn | string): Drawn[] {
  if (typeof node === 'string') return []
  return [node, ...(node.children ?? []).flatMap(elementsOf)]
}

test('should lay a file row out flat, so the names line up whatever the mark is', async ($, on) => {
  await startSession($, on)

  const pane = await $.ui.mount(TREE_PANE)

  // A row is the guide, a mark box two columns wide, the name and the letter, side by side. A
  // wrapper around the mark and the name (a fragment) made the engine stack them in a column.
  const drawn = (await pane.drawn()) as Drawn
  const fileRows = elementsOf(drawn).filter(element => (element.children ?? []).some(child => typeof child !== 'string' && child.props?.['width'] === 2))
  expect(fileRows.length).toBeGreaterThan(0)
  for (const row of fileRows) {
    const widths = (row.children ?? []).map(child => (typeof child === 'string' ? undefined : child.props?.['width']))
    expect(widths.indexOf(2)).toBe(1)
    expect((row.children ?? []).length).toBeGreaterThanOrEqual(3)
  }
  // The mark is one character in its own box; no trailing space that a renderer could trim.
  expect(await pane.find({ type: 'Text', text: /^· $/ })).toBeUndefined()
  expect(await pane.find({ type: 'Text', text: /^✎ $/ })).toBeUndefined()
  expect(await pane.find({ type: 'Text', text: /^ M$/ })).toBeUndefined()
  expect(await pane.find({ type: 'Text', text: /^·$/ })).toBeDefined()
  await pane.unmount()
})

test('should say terry cannot run, not that it is off, when terry status fails', async ($, on) => {
  await startSession($, on, { status: { ...ON, exitCode: 2, stdout: '' } })

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: /^Cannot run `terry status`/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: 'Terry is off in this folder.' })).toBeUndefined()
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

test('should open the file diff pane when a modified file is pressed', async ($, on) => {
  const opened: string[] = []
  await startSession($, on, { opened })
  const pane = await $.ui.mount(TREE_PANE)

  await pane.press({ key: 'src/main.rs' })

  expect(opened.some(entry => entry.startsWith('terry-file-diff'))).toBe(true)
  await pane.unmount()
})

test('should not open a diff when a clean file is pressed', async ($, on) => {
  const opened: string[] = []
  await startSession($, on, { opened })
  const pane = await $.ui.mount(TREE_PANE)

  await pane.press({ key: 'README.md' }).catch(() => undefined)

  expect(opened.some(entry => entry.startsWith('terry-file-diff'))).toBe(false)
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
    landed: false,
  }))
  await startSession($, on, {
    tree: { ...TREE, stdout: JSON.stringify({ root: '/work', branch: 'dev', since: 'origin/dev', tree: many }) },
  })

  const pane = await $.ui.mount({ ...TREE_PANE, props: { ...TREE_PANE.props, scroll: { offset: 0, bodyRows: 20 } } })

  expect(await pane.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  // 20 rows less the top bar (6) leaves 14 rows of the tree: file-00 to file-13.
  expect(await pane.find({ type: 'Text', text: /file-00\.rs/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /file-13\.rs/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /file-14\.rs/ })).toBeUndefined()
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

test('should call terry by the name in the settings in the pane', async ($, on) => {
  await startSession($, on, { config: configOf({ name: 'Hedwig' }) })

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: 'Hedwig is watching' })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await pane.unmount()
})

test('should call terry by the name in the settings in the band', async ($, on) => {
  await startSession($, on, { config: configOf({ name: 'Hedwig' }) })

  const band = await $.ui.mount(BAND)

  expect(await band.find({ type: 'Text', text: 'Hedwig is watching' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: /Terry/ })).toBeUndefined()
  await band.unmount()
})

test('should say the name in the settings when the folder is excluded', async ($, on) => {
  await startSession($, on, { status: OFF, config: configOf({ name: 'Hedwig' }) })

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: 'Hedwig is off in this folder.' })).toBeDefined()
  await pane.unmount()
})

test('should draw a changed file in the colour from the settings', async ($, on) => {
  const colors = { ...DEFAULT_DISPLAY.colors, modified: '#ABCDEF' }
  await startSession($, on, { config: configOf({ colors }) })

  const pane = await $.ui.mount(TREE_PANE)

  expect((await pane.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('#ABCDEF')
  await pane.unmount()
})

test('should draw the owl, his line and the branches in the colours from the settings', async ($, on) => {
  const colors = { ...DEFAULT_DISPLAY.colors, owl: '#112233', branches: ['#445566', '#778899'] }
  await startSession($, on, { config: configOf({ colors }) })

  const pane = await $.ui.mount(TREE_PANE)

  expect((await pane.find({ type: 'Text', text: 'Terry is watching' }))?.props.color).toBe('#112233')
  expect((await pane.find({ type: 'Text', text: /◠▾◠/ }))?.props.color).toBe('#112233')
  expect((await pane.find({ type: 'Text', text: 'main' }))?.props.color).toBe('#445566')
  expect((await pane.find({ type: 'Text', text: '↳ dev' }))?.props.color).toBe('#778899')
  await pane.unmount()
})

test('should ask for the width in the settings when it opens the tree pane', async ($, on) => {
  const opened: string[] = []
  await startSession($, on, { opened, config: configOf({ paneColumns: 40 }) })

  expect(opened).toContain('terry-tree:40')
})

test('should use the default look when terry config fails', async ($, on) => {
  const opened: string[] = []
  await startSession($, on, { opened, config: { ...ON, exitCode: 2, stdout: 'usage: terry ...' } })

  const pane = await $.ui.mount(TREE_PANE)

  expect(await pane.find({ type: 'Text', text: 'Terry is watching' })).toBeDefined()
  expect(opened).toContain('terry-tree:34')
  await pane.unmount()
})

test('should log a problem in the config file once, not at every poll', async ($, on) => {
  const logs: string[] = []
  const config = configOf({ problems: ['display.name: must be 1 to 24 characters, not 30; using the default'] })
  const clock = await startSession($, on, { logs, config })

  await clock.advance(15_000)
  await clock.advance(15_000)

  expect(logs.filter(line => line.startsWith('config: display.name'))).toHaveLength(1)
})

const TWO_FILES: ProcessRunResult = {
  ...ON,
  stdout: JSON.stringify({
    root: '/work',
    branch: 'dev',
    since: 'origin/dev',
    tree: [
      { file: 'a.rs', status: 'modified', landed: false },
      { file: 'b.rs', status: 'modified', landed: false },
    ],
  }),
}

async function edit($: Parameters<TestBody>[0], file: string): Promise<void> {
  await $.tool.call({ tool: 'Edit', tool_use_id: `toolu_${file}`, file_path: `/work/${file}`, old_string: 'a', new_string: 'b' })
}

test('should move the Claude colour to the next file when Claude starts another edit', async ($, on) => {
  await startSession($, on, { tree: TWO_FILES })

  await edit($, 'a.rs')
  await edit($, 'b.rs')

  const pane = await $.ui.mount(TREE_PANE)
  expect((await pane.find({ type: 'Text', text: /b\.rs/ }))?.props.color).toBe('claude')
  expect((await pane.find({ type: 'Text', text: /a\.rs/ }))?.props.color).toBe('warning')
  expect(await pane.find({ type: 'Text', text: '✎ Claude is editing b.rs' })).toBeDefined()
  expect((await pane.find({ type: 'Text', text: /^✎$/ }))?.props.color).toBe('claude')
  await pane.unmount()
})

test('should keep the file in the Claude colour when only a subagent turn ends', async ($, on) => {
  await startSession($, on)
  await edit($, 'src/main.rs')

  await $.turn.complete({ reason: 'answer', answer: 'done', durationMs: 1, isAborted: false, turnId: 'turn_02', agentId: 'sub_1' })

  const pane = await $.ui.mount(TREE_PANE)
  expect((await pane.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('claude')
  await pane.unmount()
})

test('should scroll the tree to the file Claude edits when it is out of view', async ($, on) => {
  const many = Array.from({ length: 40 }, (_, index) => ({
    file: `file-${String(index).padStart(2, '0')}.rs`,
    status: 'clean',
    landed: false,
  }))
  await startSession($, on, {
    tree: { ...TREE, stdout: JSON.stringify({ root: '/work', branch: 'dev', since: 'origin/dev', tree: many }) },
  })
  const pane = await $.ui.mount({ ...TREE_PANE, props: { ...TREE_PANE.props, scroll: { offset: 0, bodyRows: 20 } } })
  expect(await pane.find({ type: 'Text', text: /file-00\.rs/ })).toBeDefined()

  await edit($, 'file-30.rs')

  expect((await pane.find({ type: 'Text', text: /file-30\.rs/ }))?.props.color).toBe('claude')
  expect(await pane.find({ type: 'Text', text: /file-00\.rs/ })).toBeUndefined()
  await pane.unmount()
})

test('should leave the scroll place alone when the file Claude edits is in view', async ($, on) => {
  const many = Array.from({ length: 40 }, (_, index) => ({
    file: `file-${String(index).padStart(2, '0')}.rs`,
    status: 'clean',
    landed: false,
  }))
  await startSession($, on, {
    tree: { ...TREE, stdout: JSON.stringify({ root: '/work', branch: 'dev', since: 'origin/dev', tree: many }) },
  })
  const pane = await $.ui.mount({ ...TREE_PANE, props: { ...TREE_PANE.props, scroll: { offset: 0, bodyRows: 20 } } })

  await edit($, 'file-03.rs')

  expect(await pane.find({ type: 'Text', text: /file-00\.rs/ })).toBeDefined()
  expect((await pane.find({ type: 'Text', text: /file-03\.rs/ }))?.props.color).toBe('claude')
  await pane.unmount()
})

test('should ask which folder gets the file when a file is dropped on the terminal', async ($, on) => {
  await startSession($, on)
  const pane = await $.ui.mount(TREE_PANE)

  await $.prompt.edit(DROP)

  expect(await pane.find({ type: 'Text', text: /Upload shot\.png to:/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /repo root/ })).toBeDefined()
  await pane.unmount()
})

test('should leave the prompt box as it was when a file is dropped', async ($, on) => {
  await startSession($, on)

  const result = await $.prompt.edit({ ...DROP, text: 'look at ', cursor: 8 })

  expect(result).toEqual({ text: 'look at ', cursor: 8 })
})

test('should put the typed key in the prompt box when the edit is a key press', async ($, on) => {
  await startSession($, on)

  const result = await $.prompt.edit({ ...DROP, key: { key: 'a' }, text: 'hi', cursor: 2, inputText: 'a' })

  expect(result).toEqual({ text: 'hia', cursor: 3 })
})

test('should ask for nothing when the user types a path key by key', async ($, on) => {
  await startSession($, on)
  const pane = await $.ui.mount(TREE_PANE)

  await $.prompt.edit({ ...DROP, key: { key: '/' }, inputText: '/' })

  expect(await pane.find({ type: 'Text', text: /Upload/ })).toBeUndefined()
  await pane.unmount()
})

test('should upload the file into the folder and say so when a folder row is pressed after a drop', async ($, on) => {
  const uploads: string[][] = []
  await startSession($, on, { uploads })
  const pane = await $.ui.mount(TREE_PANE)
  await $.prompt.edit(DROP)

  await pane.press({ key: 'src' })

  expect(uploads).toEqual([['terry', 'upload', '"D:\\Downloads\\shot.png"', 'src']])
  expect(await pane.find({ type: 'Text', text: /Uploaded src\/shot\.png/ })).toBeDefined()
  expect(await pane.find({ type: 'Text', text: /Upload shot\.png to:/ })).toBeUndefined()
  await pane.unmount()
})

test('should keep the folder open when a folder row is pressed to receive a file', async ($, on) => {
  await startSession($, on)
  const pane = await $.ui.mount(TREE_PANE)
  await $.prompt.edit(DROP)

  await pane.press({ key: 'src' })

  expect(await pane.find({ type: 'Text', text: /main\.rs/ })).toBeDefined()
  await pane.unmount()
})

test('should upload to the repo root when the root button is pressed', async ($, on) => {
  const uploads: string[][] = []
  await startSession($, on, { uploads })
  const pane = await $.ui.mount(TREE_PANE)
  await $.prompt.edit(DROP)

  await pane.press({ key: 'upload-root' })

  expect(uploads.map(argv => argv.at(-1))).toEqual(['.'])
  await pane.unmount()
})

test('should upload nothing and drop the prompt when Cancel is pressed', async ($, on) => {
  const uploads: string[][] = []
  await startSession($, on, { uploads })
  const pane = await $.ui.mount(TREE_PANE)
  await $.prompt.edit(DROP)

  await pane.press({ key: 'upload-cancel' })

  expect(uploads).toEqual([])
  expect(await pane.find({ type: 'Text', text: /Upload shot\.png to:/ })).toBeUndefined()
  await pane.unmount()
})

test('should show the error when terry upload fails', async ($, on) => {
  const failed: ProcessRunResult = { ...ON, exitCode: 2, stdout: JSON.stringify({ ok: false, error: 'cannot read it from the desktop' }) }
  await startSession($, on, { upload: failed })
  const pane = await $.ui.mount(TREE_PANE)
  await $.prompt.edit(DROP)

  await pane.press({ key: 'src' })

  expect(await pane.find({ type: 'Text', text: /Upload failed: cannot read it from the desktop/ })).toBeDefined()
  await pane.unmount()
})

test('should read the tree again when an upload worked', async ($, on) => {
  const calls: string[][] = []
  await startSession($, on, { calls })
  const pane = await $.ui.mount(TREE_PANE)
  await $.prompt.edit(DROP)
  const treeReadsBefore = calls.filter(argv => argv.includes('tree')).length

  await pane.press({ key: 'src' })

  expect(calls.filter(argv => argv.includes('tree')).length).toBeGreaterThan(treeReadsBefore)
  await pane.unmount()
})
