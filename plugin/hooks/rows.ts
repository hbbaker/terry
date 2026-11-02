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
  | 'meta'
  | 'omitted'

/** The row kinds that belong to a file's diff, which the pane draws with a `Code` element. */
export const DIFF_KINDS: readonly RowKind[] = ['hunk', 'added', 'removed', 'context', 'meta']

/**
 * One terminal row. A diff row also names its file and the old and new line numbers it
 * stands at, which the pane needs to start a `Code` element in the middle of a hunk.
 */
export type Row = { kind: RowKind; text: string; path?: string; oldAt?: number; newAt?: number }

const HUNK_HEADER = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/

const TAB_WIDTH = 4

/**
 * Rows of the command the pane shows above the diff. A commit message in a heredoc can run to
 * dozens of lines, and the diff is what the user came to read; the permission prompt that follows
 * shows the whole command.
 */
export const MAX_COMMAND_ROWS = 3

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
  if (line.startsWith('\\')) return 'meta'
  if (line.startsWith('+')) return 'added'
  if (line.startsWith('-')) return 'removed'
  return 'context'
}

/**
 * One row per diff line, uncut: the `Code` element that draws them truncates a long line, so
 * one line stays one row. Each row carries the line numbers it stands at.
 */
function diffRows(path: string, hunks: string): Row[] {
  let oldAt = 0
  let newAt = 0
  return hunks.split('\n').map(line => {
    const kind = diffKind(line)
    const header = HUNK_HEADER.exec(line)
    if (header) {
      oldAt = Number(header[1])
      newAt = Number(header[2])
    }
    const row: Row = { kind, text: line, path, oldAt, newAt }
    if (kind === 'context' || kind === 'removed') oldAt += 1
    if (kind === 'context' || kind === 'added') newAt += 1
    return row
  })
}

/**
 * The command that led to the review, as at most `MAX_COMMAND_ROWS` rows and, when it is longer,
 * one more row that says how much is left out. Each line of the command starts its own row, and a
 * line wider than the pane takes more rows.
 */
function commandRows(command: string, columns: number): Row[] {
  const pieces = `$ ${command}`.split('\n').flatMap(line => chunkText(line, columns))
  const shown: Row[] = pieces.slice(0, MAX_COMMAND_ROWS).map(text => ({ kind: 'command', text }))
  const hidden = pieces.length - MAX_COMMAND_ROWS
  if (hidden > 0) shown.push({ kind: 'command', text: `… ${hidden} more row${hidden === 1 ? '' : 's'} of the command not shown` })
  return shown
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

  // A review of no command (one file's diff) has no command rows to draw.
  if (review.command !== '') rows.push(...commandRows(review.command, columns))
  for (const section of review.sections) {
    add('blank', '')
    add('title', section.title)
    for (const note of section.notes) add('note', note)
    for (const line of section.lint) add(lintKind(line), line)
    for (const file of section.files) {
      add('blank', '')
      add('file', file.path)
      if (file.hunks === '') add('binary', '(binary file, rename or mode change)')
      else rows.push(...diffRows(file.path, file.hunks))
    }
    if (section.omittedFiles > 0) {
      add('omitted', `${section.omittedFiles} more file(s) not drawn. Full diff: delta < ${section.patchPath}`)
    }
  }
  return rows
}

/** What the pane draws for a stretch of its window: one text row, or one run of diff rows. */
export type Block = { type: 'row'; row: Row } | { type: 'diff'; path: string; source: string }

/** Columns of the longest line that stay in view when the pane is scrolled as far right as it goes. */
const MIN_VISIBLE_TAIL = 20

/** The furthest the diff scrolls right: until the longest line shows only its last columns. */
export function lastColumn(rows: Row[]): number {
  const longest = rows
    .filter(row => row.kind === 'added' || row.kind === 'removed' || row.kind === 'context')
    .reduce((widest, row) => Math.max(widest, Array.from(row.text).length - 1), 0)
  return Math.max(0, longest - MIN_VISIBLE_TAIL)
}

/** A diff row's text without its first `column` columns; the +, - or space marker stays. */
function cutLeft(row: Row, column: number): string {
  const isCode = row.kind === 'added' || row.kind === 'removed' || row.kind === 'context'
  if (!isCode || column === 0 || row.text === '') return row.text
  const [marker = '', ...rest] = Array.from(row.text)
  return marker + rest.slice(column).join('')
}

const HUNK_CONTEXT = /^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@(.*)$/

/** A hunk header for the rows of `group`, whose first row stands at the old and new line given. */
function hunkHeader(group: Row[], first: Row, context: string): string {
  const oldCount = group.filter(row => row.kind === 'context' || row.kind === 'removed').length
  const newCount = group.filter(row => row.kind === 'context' || row.kind === 'added').length
  return `@@ -${first.oldAt ?? 1},${oldCount} +${first.newAt ?? 1},${newCount} @@${context}`
}

/**
 * The diff rows of one file as the source a `Code` element reads under `format: 'diff'`. That
 * element refuses a hunk whose header counts do not match its lines, so every hunk the window
 * shows, cut or whole, gets a header counted from the rows that are shown.
 */
function diffSource(run: Row[], column: number): string {
  const groups: { header: Row | undefined; context: string; rows: Row[] }[] = []
  for (const row of run) {
    if (row.kind === 'hunk') {
      groups.push({ header: row, context: HUNK_CONTEXT.exec(row.text)?.[1] ?? '', rows: [] })
    } else if (groups.length === 0) {
      groups.push({ header: undefined, context: '', rows: [row] })
    } else {
      groups[groups.length - 1]?.rows.push(row)
    }
  }
  const lines: string[] = []
  for (const group of groups) {
    const first = group.header ?? group.rows[0]
    if (first === undefined) continue
    lines.push(hunkHeader(group.rows, first, group.context))
    for (const row of group.rows) lines.push(cutLeft(row, column))
  }
  return lines.join('\n')
}

/** Groups the rows of the window: each file's diff rows become one block, so one `Code`. */
export function toBlocks(shown: Row[], column = 0): Block[] {
  const blocks: Block[] = []
  let run: Row[] = []
  const flush = () => {
    const [first] = run
    if (first !== undefined) blocks.push({ type: 'diff', path: first.path ?? '', source: diffSource(run, column) })
    run = []
  }
  for (const row of shown) {
    if (!DIFF_KINDS.includes(row.kind)) {
      flush()
      blocks.push({ type: 'row', row })
      continue
    }
    if (run[0] !== undefined && run[0].path !== row.path) flush()
    run.push(row)
  }
  flush()
  return blocks
}
