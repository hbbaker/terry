// Drag and drop upload: what the plugin does with a path the terminal pasted, and with the answer
// of `terry upload`. Only pure functions are here, so tests need no engine.

/** The part of a `prompt.edit` event that tells a paste from a key press. */
export type PromptEdit = {
  /** The key the user pressed; a paste has none. */
  key?: unknown
  /** What the edit put into the prompt box. */
  inputText: string
}

/** What `terry upload` answered, or why its answer could not be read. */
export type UploadResult = { ok: true; dest: string } | { ok: false; error: string }

// A path the way a terminal pastes a dropped file: a drive letter, a root, `~/` or a `file://` link.
const PATH_START = /^(?:[A-Za-z]:[\\/]|\/|~\/|file:\/\/)/
// One file in quotes. Two quoted paths (two files dropped at once) do not match, so a drop of
// several files is left to the prompt box.
const QUOTED_ONE = /^(?:"[^"\n]+"|'[^'\n]+')$/
const QUOTE_CHARACTER = /["']/
const ESCAPED_CHARACTER = /\\./g
const SEPARATORS = /[\\/]/
const QUOTE_MARKS = /^["']|["']$/g

/**
 * The path a paste put into the prompt box; null for a key press, for text that is no path, and
 * for a paste of several files. The path is returned as pasted: `terry upload` cleans it.
 */
export function droppedPath(edit: PromptEdit): string | null {
  if (edit.key !== undefined) return null
  const text = edit.inputText.trim()
  const isQuoted = QUOTED_ONE.test(text)
  const path = isQuoted ? text.slice(1, -1) : text
  // A quote that no backslash escapes, in a path that is not quoted, means several quoted files.
  const hasStrayQuote = !isQuoted && QUOTE_CHARACTER.test(text.replace(ESCAPED_CHARACTER, ''))
  return !text.includes('\n') && !hasStrayQuote && PATH_START.test(path) ? text : null
}

/** The file name of a pasted path, for the banner. */
export function uploadName(source: string): string {
  const bare = source.trim().replace(QUOTE_MARKS, '')
  return bare.split(SEPARATORS).filter(part => part !== '').pop() ?? bare
}

/** The answer of `terry upload`. A reply that is no JSON is an error with the text of stderr. */
export function parseUpload(stdout: string, stderr: string): UploadResult {
  try {
    const reply = JSON.parse(stdout) as { ok?: unknown; dest?: unknown; error?: unknown }
    if (reply.ok === true && typeof reply.dest === 'string') return { ok: true, dest: reply.dest }
    if (typeof reply.error === 'string') return { ok: false, error: reply.error }
  } catch {
    // Not JSON: `terry` failed before it could answer.
  }
  return { ok: false, error: stderr.trim() || 'terry upload gave no answer' }
}

/** The line the pane shows after an upload. */
export function uploadMessage(result: UploadResult): string {
  return result.ok ? `Uploaded ${result.dest}` : `Upload failed: ${result.error}`
}

/** The most rows the last answer may take. A failure holds the `scp` command to run instead. */
const MAX_NOTE_ROWS = 10

/** The answer cut to fit `MAX_NOTE_ROWS` rows of `columns` columns, with an ellipsis when it was cut. */
export function fitNote(note: string, columns: number): string {
  const room = MAX_NOTE_ROWS * Math.max(1, columns) - 1
  return note.length > room ? `${note.slice(0, room)}…` : note
}

/** Rows the answer takes when it wraps at `columns` columns; none when there is no answer. */
export function noteRows(note: string | null, columns: number): number {
  return note === null ? 0 : Math.min(MAX_NOTE_ROWS, Math.ceil(note.length / Math.max(1, columns)))
}

/** Rows the upload banner takes in the pane: the prompt and its buttons, and the last answer. */
export function bannerRows(hasPending: boolean, answerRows: number): number {
  return (hasPending ? 2 : 0) + answerRows
}
