import { expect, test } from 'claude-code/testing'

import { activeLine, flattenRows, parseTree, repoPath, rowLook, topBar, withoutFirst } from '../hooks/tree'
import type { TreeMarks, TreeRow } from '../hooks/tree'

const TREE_JSON = JSON.stringify({
  root: '/work/app',
  branch: 'main',
  since: 'origin/main',
  tree: [
    { dir: 'src', children: [{ file: 'main.rs', status: 'modified', landed: true }] },
    { file: 'README.md', status: 'clean', landed: false },
  ],
})

const NO_MARKS: TreeMarks = {
  hasChanges: false,
  collapsed: new Set(),
  claudeTouched: new Set(),
  claudeActive: new Set(),
}

function file(path: string, status: TreeRow['status'], landed = false): TreeRow {
  const name = path.split('/').pop() ?? path
  return { path, name, depth: 0, guide: '├─ ', isDir: false, status, landed, changedBelow: 0 }
}

test('should read the root, the branch, the base and the files when terry tree prints its JSON', () => {
  const tree = parseTree(TREE_JSON)

  expect(tree?.root).toBe('/work/app')
  expect(tree?.branch).toBe('main')
  expect(tree?.since).toBe('origin/main')
  expect(tree?.nodes).toHaveLength(2)
})

test('should return null when the text is not JSON', () => {
  expect(parseTree('ERROR: not a repo')).toBeNull()
})

test('should return null when a file has a status terry does not use', () => {
  const text = JSON.stringify({
    root: '/w',
    branch: 'main',
    since: 'HEAD',
    tree: [{ file: 'a', status: 'renamed', landed: false }],
  })

  expect(parseTree(text)).toBeNull()
})

test('should return null when a field is missing', () => {
  expect(parseTree(JSON.stringify({ branch: 'main', since: 'HEAD', tree: [] }))).toBeNull()
  expect(parseTree(JSON.stringify({ root: '/w', branch: 'main', tree: [] }))).toBeNull()
})

test('should return null when a file does not say whether it landed', () => {
  const text = JSON.stringify({ root: '/w', branch: 'main', since: 'HEAD', tree: [{ file: 'a', status: 'clean' }] })

  expect(parseTree(text)).toBeNull()
})

test('should list each folder before its entries with the path from the repo root', () => {
  const rows = flattenRows(parseTree(TREE_JSON)?.nodes ?? [])

  expect(rows.map(row => row.path)).toEqual(['src', 'src/main.rs', 'README.md'])
  expect(rows.map(row => row.depth)).toEqual([0, 1, 0])
  expect(rows.map(row => row.isDir)).toEqual([true, false, false])
})

test('should give a path relative to the root when the file is inside the repo', () => {
  expect(repoPath('/work/app', '/work/app/src/main.rs')).toBe('src/main.rs')
})

test('should return null when the file is outside the repo', () => {
  expect(repoPath('/work/app', '/work/application/a.rs')).toBeNull()
  expect(repoPath('/work/app', '/tmp/a.rs')).toBeNull()
})

test('should draw a file plain when nothing changed', () => {
  expect(rowLook(file('a.rs', 'clean'), NO_MARKS)).toEqual({ glyph: '  ', style: {}, badge: '' })
})

test('should dim a clean file when another file changed', () => {
  const look = rowLook(file('a.rs', 'clean'), { ...NO_MARKS, hasChanges: true })

  expect(look.style.dimColor).toBe(true)
})

test('should colour a changed file by its status when no diff was reviewed in the session', () => {
  const marks = { ...NO_MARKS, hasChanges: true }

  const colors = (['added', 'modified', 'deleted', 'untracked'] as const).map(
    status => rowLook(file('a.rs', status), marks).style.color,
  )

  expect(new Set(colors).size).toBe(4)
})

test('should mark a file with the Claude glyph when Claude touched it', () => {
  const marks = { ...NO_MARKS, hasChanges: true, claudeTouched: new Set(['a.rs']) }

  expect(rowLook(file('a.rs', 'modified'), marks).glyph).toBe('✎ ')
  expect(rowLook(file('b.rs', 'modified'), marks).glyph).toBe('· ')
})

test('should draw a changed file bold until a commit lands it, then dim', () => {
  const marks = { ...NO_MARKS, hasChanges: true }

  expect(rowLook(file('a.rs', 'modified', false), marks).style).toMatchObject({ bold: true, dimColor: false })
  expect(rowLook(file('a.rs', 'modified', true), marks).style).toMatchObject({ bold: false, dimColor: true })
})

test('should join a row to its folder with guide lines, the last entry with a corner', () => {
  const nodes = [
    { dir: 'src', children: [{ file: 'a.rs', status: 'clean' as const, landed: false }, { file: 'b.rs', status: 'clean' as const, landed: false }] },
    { file: 'README.md', status: 'clean' as const, landed: false },
  ]

  const rows = flattenRows(nodes)

  expect(rows.map(row => row.guide)).toEqual(['├─ ', '│  ├─ ', '│  └─ ', '└─ '])
})

test('should leave blanks under the last folder where no guide line goes on', () => {
  const nodes = [{ dir: 'src', children: [{ file: 'a.rs', status: 'clean' as const, landed: false }] }]

  expect(flattenRows(nodes).map(row => row.guide)).toEqual(['└─ ', '   └─ '])
})

test('should put the status letter at the end of a changed file', () => {
  const marks = { ...NO_MARKS, hasChanges: true }

  const badges = (['added', 'modified', 'deleted', 'untracked', 'clean'] as const).map(
    status => rowLook(file('a.rs', status), marks).badge,
  )

  expect(badges).toEqual(['A', 'M', 'D', '?', ''])
})

test('should draw a folder bold with a folder mark', () => {
  const folder: TreeRow = { ...file('src', 'clean'), isDir: true }

  const look = rowLook(folder, NO_MARKS)

  expect(look.glyph).toBe('▾ ')
  expect(look.style.bold).toBe(true)
})

const NESTED = [
  {
    dir: 'src',
    children: [
      { file: 'a.rs', status: 'modified' as const, landed: false },
      { dir: 'deep', children: [{ file: 'b.rs', status: 'added' as const, landed: true }] },
    ],
  },
  { file: 'README.md', status: 'clean' as const, landed: false },
]

test('should leave out the entries of a folder when the folder is collapsed', () => {
  const rows = flattenRows(NESTED, new Set(['src']))

  expect(rows.map(row => row.path)).toEqual(['src', 'README.md'])
})

test('should keep the rest of the tree open when one inner folder is collapsed', () => {
  const rows = flattenRows(NESTED, new Set(['src/deep']))

  expect(rows.map(row => row.path)).toEqual(['src', 'src/a.rs', 'src/deep', 'README.md'])
})

test('should count the changed files of a folder at any depth', () => {
  const rows = flattenRows(NESTED)

  expect(rows.find(row => row.path === 'src')?.changedBelow).toBe(2)
  expect(rows.find(row => row.path === 'src/deep')?.changedBelow).toBe(1)
})

test('should mark a collapsed folder with a closed glyph and the count of changes it hides', () => {
  const marks = { ...NO_MARKS, hasChanges: true, collapsed: new Set(['src']) }
  const folder = flattenRows(NESTED, marks.collapsed)[0]!

  const look = rowLook(folder, marks)

  expect(look.glyph).toBe('▸ ')
  expect(look.badge).toBe('2')
})

test('should show no count on a collapsed folder when it hides no changed file', () => {
  const clean = [{ dir: 'src', children: [{ file: 'a.rs', status: 'clean' as const, landed: false }] }]
  const marks = { ...NO_MARKS, collapsed: new Set(['src']) }
  const folder = flattenRows(clean, marks.collapsed)[0]!

  expect(rowLook(folder, marks).badge).toBe('')
})

const BAR = { artRows: 4, artColumns: 11, wordRows: 3, bodyColumns: 40, bodyRows: 30 }

test('should put the owl beside his words when the words keep enough columns', () => {
  expect(topBar(BAR)).toEqual({ hasArt: true, isBeside: true, rows: 5 })
})

test('should put the owl above his words when the pane is narrow', () => {
  expect(topBar({ ...BAR, bodyColumns: 24 })).toEqual({ hasArt: true, isBeside: false, rows: 8 })
})

test('should leave the owl out when the pane is short', () => {
  expect(topBar({ ...BAR, bodyRows: 10 })).toEqual({ hasArt: false, isBeside: false, rows: 4 })
})

test('should size the bar by the owl when his words are shorter than he is', () => {
  expect(topBar({ ...BAR, wordRows: 2 }).rows).toBe(5)
})

test('should draw a changed file in the Claude colour and bold, with an orange mark, while Claude edits it', () => {
  const marks = { ...NO_MARKS, hasChanges: true, claudeActive: new Set(['a.rs']) }

  const look = rowLook(file('a.rs', 'modified', true), marks)

  expect(look.style).toEqual({ color: 'claude', bold: true })
  expect(look.glyph).toBe('✎ ')
  expect(look.glyphColor).toBe('claude')
  expect(look.badge).toBe('M')
  expect(look.badgeColor).toBe('warning')
})

test('should draw an unchanged file in the Claude colour while Claude is about to edit it', () => {
  const marks = { ...NO_MARKS, claudeActive: new Set(['a.rs']) }

  const look = rowLook(file('a.rs', 'clean'), marks)

  expect(look.style).toEqual({ color: 'claude', bold: true })
  expect(look.glyph).toBe('✎ ')
  expect(look.badge).toBe('')
})

test('should keep the status colour and an orange mark when Claude edited a file earlier', () => {
  const marks = { ...NO_MARKS, hasChanges: true, claudeTouched: new Set(['a.rs']) }

  const look = rowLook(file('a.rs', 'modified'), marks)

  expect(look.style.color).toBe('warning')
  expect(look.glyph).toBe('✎ ')
  expect(look.glyphColor).toBe('claude')
})

test('should leave the mark without a colour of its own when the change is not by Claude', () => {
  const marks = { ...NO_MARKS, hasChanges: true }

  const look = rowLook(file('a.rs', 'modified'), marks)

  expect(look.glyph).toBe('· ')
  expect(look.glyphColor).toBeUndefined()
})

test('should say nothing about editing when Claude edits no file', () => {
  expect(activeLine([])).toBeNull()
})

test('should name the file when Claude edits one', () => {
  expect(activeLine(['src/tree.rs'])).toBe('Claude is editing tree.rs')
})

test('should count the other files when Claude edits several', () => {
  expect(activeLine(['src/a.rs', 'src/b.rs', 'c.rs'])).toBe('Claude is editing a.rs +2')
})

test('should end one call at a time when two calls edit the same file', () => {
  expect(withoutFirst(['a.rs', 'a.rs', 'b.rs'], 'a.rs')).toEqual(['a.rs', 'b.rs'])
})

test('should leave the paths as they are when the file is not in them', () => {
  expect(withoutFirst(['a.rs'], 'b.rs')).toEqual(['a.rs'])
})
