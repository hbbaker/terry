import { expect, test } from 'claude-code/testing'

import { buildRows, chunkText } from '../hooks/rows'
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

test('should count a wrapped diff line as several rows', () => {
  const long = { ...REVIEW, sections: [{ ...REVIEW.sections[0]!, files: [{ path: 'a', hunks: '@@ -1 +1 @@\n+' + 'x'.repeat(25) }] }] }

  const rows = buildRows(long, 10).filter(row => row.kind === 'added')

  expect(rows.map(row => row.text.length)).toEqual([10, 10, 6])
})
