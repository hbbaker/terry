import { expect, test } from 'claude-code/testing'

import { flattenRows, parseTree, repoPath, rowLook, topBar } from '../hooks/tree'
import type { TreeMarks, TreeRow } from '../hooks/tree'

const TREE_JSON = JSON.stringify({
  root: '/work/app',
  branch: 'main',
  tree: [
    { dir: 'src', children: [{ file: 'main.rs', status: 'modified' }] },
    { file: 'README.md', status: 'clean' },
  ],
})

const NO_MARKS: TreeMarks = {
  hasReviewed: false,
  collapsed: new Set(),
  claudeTouched: new Set(),
  reviewed: new Set(),
}

function file(path: string, status: TreeRow['status']): TreeRow {
  return { path, name: path.split('/').pop() ?? path, depth: 0, guide: '├─ ', isDir: false, status, changedBelow: 0 }
}

test('should read the root, the branch and the files when terry tree prints its JSON', () => {
  const tree = parseTree(TREE_JSON)

  expect(tree?.root).toBe('/work/app')
  expect(tree?.branch).toBe('main')
  expect(tree?.nodes).toHaveLength(2)
})

test('should return null when the text is not JSON', () => {
  expect(parseTree('ERROR: not a repo')).toBeNull()
})

test('should return null when a file has a status terry does not use', () => {
  const text = JSON.stringify({ root: '/w', branch: 'main', tree: [{ file: 'a', status: 'renamed' }] })

  expect(parseTree(text)).toBeNull()
})

test('should return null when a field is missing', () => {
  expect(parseTree(JSON.stringify({ branch: 'main', tree: [] }))).toBeNull()
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

test('should draw every file plain when no diff was reviewed yet', () => {
  expect(rowLook(file('a.rs', 'modified'), NO_MARKS)).toEqual({ glyph: '  ', style: {}, badge: '' })
})

test('should dim a clean file when a diff was reviewed', () => {
  const look = rowLook(file('a.rs', 'clean'), { ...NO_MARKS, hasReviewed: true })

  expect(look.style.dimColor).toBe(true)
})

test('should colour a changed file by its status when a diff was reviewed', () => {
  const marks = { ...NO_MARKS, hasReviewed: true }

  const colors = (['added', 'modified', 'deleted', 'untracked'] as const).map(
    status => rowLook(file('a.rs', status), marks).style.color,
  )

  expect(new Set(colors).size).toBe(4)
})

test('should mark a file with the Claude glyph when Claude touched it', () => {
  const marks = { ...NO_MARKS, hasReviewed: true, claudeTouched: new Set(['a.rs']) }

  expect(rowLook(file('a.rs', 'modified'), marks).glyph).toBe('✎ ')
  expect(rowLook(file('b.rs', 'modified'), marks).glyph).toBe('· ')
})

test('should draw a changed file bold until its diff is approved, then dim', () => {
  const unreviewed = { ...NO_MARKS, hasReviewed: true }
  const reviewed = { ...unreviewed, reviewed: new Set(['a.rs']) }

  expect(rowLook(file('a.rs', 'modified'), unreviewed).style).toMatchObject({ bold: true, dimColor: false })
  expect(rowLook(file('a.rs', 'modified'), reviewed).style).toMatchObject({ bold: false, dimColor: true })
})

test('should join a row to its folder with guide lines, the last entry with a corner', () => {
  const nodes = [
    { dir: 'src', children: [{ file: 'a.rs', status: 'clean' as const }, { file: 'b.rs', status: 'clean' as const }] },
    { file: 'README.md', status: 'clean' as const },
  ]

  const rows = flattenRows(nodes)

  expect(rows.map(row => row.guide)).toEqual(['├─ ', '│  ├─ ', '│  └─ ', '└─ '])
})

test('should leave blanks under the last folder where no guide line goes on', () => {
  const nodes = [{ dir: 'src', children: [{ file: 'a.rs', status: 'clean' as const }] }]

  expect(flattenRows(nodes).map(row => row.guide)).toEqual(['└─ ', '   └─ '])
})

test('should put the status letter at the end of a changed file when a diff was reviewed', () => {
  const marks = { ...NO_MARKS, hasReviewed: true }

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
      { file: 'a.rs', status: 'modified' as const },
      { dir: 'deep', children: [{ file: 'b.rs', status: 'added' as const }] },
    ],
  },
  { file: 'README.md', status: 'clean' as const },
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
  const marks = { ...NO_MARKS, hasReviewed: true, collapsed: new Set(['src']) }
  const folder = flattenRows(NESTED, marks.collapsed)[0]!

  const look = rowLook(folder, marks)

  expect(look.glyph).toBe('▸ ')
  expect(look.badge).toBe('2')
})

test('should show no count on a collapsed folder when no diff was reviewed yet', () => {
  const marks = { ...NO_MARKS, collapsed: new Set(['src']) }
  const folder = flattenRows(NESTED, marks.collapsed)[0]!

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
