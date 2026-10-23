import type { ReviewFile } from '../types'

const FILE_HEADER = /^diff --git a\/(.+?) b\/(.+)$/
const HUNK_HEADER = '@@'

/**
 * Splits a `git diff` patch into one entry per file, keeping only the hunks, which is
 * the source a `Code` element under `format: 'diff'` parses.
 */
export function splitPatch(patch: string): ReviewFile[] {
  const files: ReviewFile[] = []
  let path: string | null = null
  let hunkLines: string[] = []

  const finishFile = () => {
    if (path !== null) files.push({ path, hunks: hunkLines.join('\n').trimEnd() })
  }

  for (const line of patch.split('\n')) {
    const header = FILE_HEADER.exec(line)
    if (header) {
      finishFile()
      path = header[2] ?? header[1] ?? ''
      hunkLines = []
    } else if (path !== null && (hunkLines.length > 0 || line.startsWith(HUNK_HEADER))) {
      hunkLines.push(line)
    }
  }
  finishFile()
  return files
}

/**
 * Keeps whole files, in order, while their hunks fit in `budget` characters, so a
 * drawing is never cut mid-hunk (a cut hunk does not parse as a diff).
 */
export function fitFiles(files: ReviewFile[], budget: number): { shown: ReviewFile[]; left: number } {
  const shown: ReviewFile[] = []
  let left = budget
  for (const file of files) {
    if (file.hunks.length > left) break
    left -= file.hunks.length
    shown.push(file)
  }
  return { shown, left }
}
