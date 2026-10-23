import { expect, test } from 'claude-code/testing'

import { buildRows, chunkText, lastColumn, toBlocks } from '../hooks/rows'
import type { Review } from '../types'

const REVIEW: Review = {
  toolUseId: 'toolu_01',
  command: 'git commit -m x',
  stage: 'pane',
  reviewKey: 'a'.repeat(64),
  decision: 'pending',
  sections: [
    {
      title: 'git commit: Staged changes',
      notes: ['a note'],
      lint: ['LINT passed on added lines (ruff)'],
      patchPath: '/p.patch',
      files: [
        { path: 'app.py', hunks: '@@ -1 +1 @@\n-x = 1\n+x = 2' },
        { path: 'logo.png', hunks: '' },
      ],
      omittedFiles: 2,
    },
  ],
}

test('should cut a long line into rows no wider than the pane', () => {
  expect(chunkText('abcdefg', 3)).toEqual(['abc', 'def', 'g'])
})

test('should keep an empty line as one row', () => {
  expect(chunkText('', 10)).toEqual([''])
})

test('should turn a tab into spaces so the row width holds', () => {
  expect(chunkText('\tx', 10)).toEqual(['    x'])
})

test('should give one row per line of the review, in order, with the kind of each', () => {
  const rows = buildRows(REVIEW, 80)

  expect(rows.map(row => row.kind)).toEqual([
    'command',
    'blank',
    'title',
    'note',
    'lint-pass',
    'blank',
    'file',
    'hunk',
    'removed',
    'added',
    'blank',
    'file',
    'binary',
    'omitted',
  ])
  expect(rows[0]?.text).toBe('$ git commit -m x')
})

test('should keep a long diff line as one row, for the Code element to truncate', () => {
  const long = { ...REVIEW, sections: [{ ...REVIEW.sections[0]!, files: [{ path: 'a', hunks: '@@ -1 +1 @@\n+' + 'x'.repeat(25) }] }] }

  const rows = buildRows(long, 10).filter(row => row.kind === 'added')

  expect(rows.map(row => row.text.length)).toEqual([26])
})

test('should note the old and new line numbers each diff row stands at', () => {
  const rows = buildRows(REVIEW, 80).filter(row => row.kind === 'removed' || row.kind === 'added')

  expect(rows.map(row => [row.oldAt, row.newAt])).toEqual([
    [1, 1],
    [2, 1],
  ])
})

test('should give one Code block per file for the diff rows of the window', () => {
  const blocks = toBlocks(buildRows(REVIEW, 80))

  const diffs = blocks.filter(block => block.type === 'diff')
  expect(diffs.map(block => block.path)).toEqual(['app.py'])
  expect(diffs[0]?.source).toBe('@@ -1,1 +1,1 @@\n-x = 1\n+x = 2')
})

test('should count the header of a hunk from the rows the window shows', () => {
  const rows = buildRows(REVIEW, 80)
  const cutAfterRemoved = rows.slice(rows.findIndex(row => row.kind === 'hunk'), rows.findIndex(row => row.kind === 'added'))

  const diffs = toBlocks(cutAfterRemoved).filter(block => block.type === 'diff')

  expect(diffs[0]?.source).toBe('@@ -1,1 +1,0 @@\n-x = 1')
})

test('should start the Code block with a header when the window begins inside a hunk', () => {
  const rows = buildRows(REVIEW, 80)
  const fromAdded = rows.slice(rows.findIndex(row => row.kind === 'added'), rows.findIndex(row => row.kind === 'added') + 1)

  const diffs = toBlocks(fromAdded).filter(block => block.type === 'diff')

  expect(diffs[0]?.source).toBe('@@ -2,0 +1,1 @@\n+x = 2')
})

const WIDE: Review = {
  ...REVIEW,
  sections: [{ ...REVIEW.sections[0]!, files: [{ path: 'w.py', hunks: '@@ -1 +1 @@\n-' + 'o'.repeat(10) + '\n+' + '0123456789'.repeat(6) }], omittedFiles: 0 }],
}

test('should cut the left columns of each code line and keep its marker', () => {
  const blocks = toBlocks(buildRows(WIDE, 80), 5)

  const source = blocks.find(block => block.type === 'diff')?.source
  expect(source).toBe('@@ -1,1 +1,1 @@\n-ooooo\n+56789' + '0123456789'.repeat(5))
})

test('should leave the hunk header and the text of other rows as they are when the diff is cut', () => {
  const rows = buildRows(WIDE, 80)

  const texts = toBlocks(rows, 5).flatMap(block => (block.type === 'row' ? [block.row.text] : []))

  expect(texts).toContain('w.py')
})

test('should stop sideways scrolling when only the last columns of the longest line are left', () => {
  expect(lastColumn(buildRows(WIDE, 80))).toBe(60 - 20)
  expect(lastColumn(buildRows({ ...WIDE, sections: [] }, 80))).toBe(0)
})
