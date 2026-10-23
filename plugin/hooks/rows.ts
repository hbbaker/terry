import type { Review } from '../types'

/** What a pane row shows, so the drawing can colour it; one row is one terminal line. */
export type RowKind =
  | 'blank'
  | 'command'
  | 'title'
  | 'note'
  | 'lint-pass'
  | 'lint-problem'
  | 'lint-info'
  | 'file'
  | 'binary'
  | 'added'
  | 'removed'
  | 'hunk'
  | 'context'
  | 'omitted'

export type Row = { kind: RowKind; text: string }

const TAB_WIDTH = 4

/** Cuts `text` into pieces of at most `width` characters, so every piece fills one row. */
export function chunkText(text: string, width: number): string[] {
  const characters = Array.from(text.replaceAll('\t', ' '.repeat(TAB_WIDTH)))
  if (characters.length === 0) return ['']
  const pieces: string[] = []
  for (let start = 0; start < characters.length; start += width) {
    pieces.push(characters.slice(start, start + width).join(''))
  }
  return pieces
}

function lintKind(line: string): RowKind {
  if (line.startsWith('LINT passed')) return 'lint-pass'
  return line.startsWith('LINT') ? 'lint-problem' : 'lint-info'
}

function diffKind(line: string): RowKind {
  if (line.startsWith('@@')) return 'hunk'
  if (line.startsWith('+')) return 'added'
  if (line.startsWith('-')) return 'removed'
  return 'context'
}

/**
 * The review as terminal rows, `columns` wide: the pane draws a window of them under its own
 * fixed header, so a long line is cut into rows here and the row count is exact.
 */
export function buildRows(review: Review, columns: number): Row[] {
  const rows: Row[] = []
  const add = (kind: RowKind, text: string) => {
    for (const piece of chunkText(text, columns)) rows.push({ kind, text: piece })
  }

  add('command', `$ ${review.command}`)
  for (const section of review.sections) {
    add('blank', '')
    add('title', section.title)
    for (const note of section.notes) add('note', note)
    for (const line of section.lint) add(lintKind(line), line)
    for (const file of section.files) {
      add('blank', '')
      add('file', file.path)
      if (file.hunks === '') add('binary', '(binary file, rename or mode change)')
      else for (const line of file.hunks.split('\n')) add(diffKind(line), line)
    }
    if (section.omittedFiles > 0) {
      add('omitted', `${section.omittedFiles} more file(s) not drawn. Full diff: delta < ${section.patchPath}`)
    }
  }
  return rows
}
