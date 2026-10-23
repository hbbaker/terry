import { expect, test } from 'claude-code/testing'

import { fitFiles, splitPatch } from '../hooks/patch'

const PATCH = [
  'diff --git a/app.py b/app.py',
  'index 1111111..2222222 100644',
  '--- a/app.py',
  '+++ b/app.py',
  '@@ -1 +1 @@',
  '-print("v1")',
  '+print("v2")',
  'diff --git a/logo.png b/logo.png',
  'Binary files a/logo.png and b/logo.png differ',
  'diff --git a/src/new.ts b/src/new.ts',
  'new file mode 100644',
  '--- /dev/null',
  '+++ b/src/new.ts',
  '@@ -0,0 +1 @@',
  '+export const x = 1',
  '',
].join('\n')

test('should split a patch into one entry per file with only its hunks', async () => {
  const files = splitPatch(PATCH)

  expect(files.map(file => file.path)).toEqual(['app.py', 'logo.png', 'src/new.ts'])
  expect(files[0]?.hunks).toBe('@@ -1 +1 @@\n-print("v1")\n+print("v2")')
  expect(files[2]?.hunks).toBe('@@ -0,0 +1 @@\n+export const x = 1')
})

test('should give an empty hunk text when a file has no hunks', async () => {
  expect(splitPatch(PATCH)[1]?.hunks).toBe('')
})

test('should keep whole files only while they fit in the budget', async () => {
  const files = splitPatch(PATCH)
  const firstSize = files[0]?.hunks.length ?? 0

  const { shown } = fitFiles(files, firstSize + 1)

  expect(shown.map(file => file.path)).toEqual(['app.py', 'logo.png'])
})
