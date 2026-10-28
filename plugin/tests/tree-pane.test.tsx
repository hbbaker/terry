import { expect, mock, test } from 'claude-code/testing'
import type { ProcessRunResult } from 'claude-code'
import type { TestBody } from 'claude-code/testing'

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
  /** The core of `tool.call` waits for this before it answers: a call that is still running. */
  hold?: Promise<void>
}

/** Starts the session with the files of a repo on `dev`. */
async function startSession(
  ...[$, on, { status = ON, panes = [], opened = [], tree = TREE, hold = Promise.resolve() } = {}]: [
    ...Parameters<TestBody>,
    SessionOptions?,
  ]
): Promise<ReturnType<typeof mock.clock>> {
  const clock = mock.clock(on)
  on('env.get', () => ({ value: HOME }))
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

test('should mark a file as Claude edited when Claude edits it', async ($, on) => {
  await startSession($, on)

  await $.tool.call({
    tool: 'Edit',
    tool_use_id: 'toolu_02',
    file_path: '/work/src/main.rs',
    old_string: 'a',
    new_string: 'b',
  })

  const pane = await $.ui.mount(TREE_PANE)
  expect((await pane.find({ type: 'Text', text: '✎' }))?.props.color).toBe('claude')
  expect((await pane.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('warning')
  await pane.unmount()
})

test('should show the file in the Claude colour while an Edit call runs, and keep the mark after', async ($, on) => {
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
  const after = await $.ui.mount(TREE_PANE)

  expect((await after.find({ type: 'Text', text: /main\.rs/ }))?.props.color).toBe('warning')
  expect((await after.find({ type: 'Text', text: '✎' }))?.props.color).toBe('claude')
  expect(await after.find({ type: 'Text', text: /Claude is editing/ })).toBeUndefined()
  await after.unmount()
})

test('should stop showing the file as edited when the Edit call fails', async ($, on) => {
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
