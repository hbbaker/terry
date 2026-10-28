import type { Palette, TreeNode, TreeSnapshot, TreeStatus } from '../types'

const TREE_STATUSES: readonly string[] = ['clean', 'added', 'modified', 'deleted', 'untracked']
// After the status colour, the letter that says it at the right edge of a row.
const STATUS_BADGES: Record<Exclude<TreeStatus, 'clean'>, string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  untracked: '?',
}
// Before a name: a folder's mark, who changed a file, or blanks that keep the names in line.
const GLYPH_FOLDER_OPEN = '▾ '
const GLYPH_FOLDER_CLOSED = '▸ '
const GLYPH_CLAUDE = '✎ '
const GLYPH_USER = '· '
const GLYPH_NONE = '  '
// The lines that join a row to its folder: before the row's own name, and passed down to its children.
const GUIDE_MIDDLE = '├─ '
const GUIDE_LAST = '└─ '
const GUIDE_PASS = '│  '
const GUIDE_BLANK = '   '

/** One line of the drawn tree: a file, or a folder, `depth` folders below the repo root. */
export type TreeRow = {
  path: string
  name: string
  depth: number
  /** The lines that join the row to its folders, ending in `├─ ` or `└─ `. */
  guide: string
  isDir: boolean
  status: TreeStatus
  /** True for a file that a commit since the base changed, with nothing newer uncommitted. */
  landed: boolean
  /** For a folder, how many changed files it holds at any depth; 0 for a file. */
  changedBelow: number
}

export type RowStyle = { bold?: boolean; color?: string; dimColor?: boolean }

/**
 * How a row is drawn: the glyph before its name (in `glyphColor` when it has one, else in the
 * name's style), the name's text style, and the badge after the name.
 */
export type RowLook = { glyph: string; glyphColor?: string; style: RowStyle; badge: string; badgeColor?: string }

/** What the rows need: the colours, whether any file changed, who touched which file, and which folders are closed. */
export type TreeMarks = {
  palette: Palette
  hasChanges: boolean
  collapsed: ReadonlySet<string>
  /** Files Claude's Edit or Write changed in this session. */
  claudeTouched: ReadonlySet<string>
  /** Files an Edit or Write call of Claude's is on right now. */
  claudeActive: ReadonlySet<string>
}

function isNode(value: unknown): value is TreeNode {
  if (typeof value !== 'object' || value === null) return false
  const node = value as Record<string, unknown>
  if (typeof node['file'] === 'string') {
    return typeof node['status'] === 'string' && TREE_STATUSES.includes(node['status']) && typeof node['landed'] === 'boolean'
  }
  return typeof node['dir'] === 'string' && Array.isArray(node['children']) && node['children'].every(isNode)
}

/** Reads the JSON `terry tree` prints; null when it is not that shape. */
export function parseTree(text: string): TreeSnapshot | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return null
  }
  if (typeof parsed !== 'object' || parsed === null) return null
  const { root, branch, since, tree } = parsed as Record<string, unknown>
  if (typeof root !== 'string' || typeof branch !== 'string' || typeof since !== 'string') return null
  if (!Array.isArray(tree) || !tree.every(isNode)) return null
  return { root, branch, since, nodes: tree }
}

/** How many changed files `nodes` hold, at any depth. */
function countChanged(nodes: readonly TreeNode[]): number {
  return nodes.reduce((total, node) => {
    if ('dir' in node) return total + countChanged(node.children)
    return node.status === 'clean' ? total : total + 1
  }, 0)
}

/**
 * The tree as lines, each folder followed by its entries unless the folder is in `collapsed`.
 * `parent` is the path to `nodes`, ending in `/`; `indent` is the guide lines the folders above
 * pass down to them.
 */
export function flattenRows(
  nodes: readonly TreeNode[],
  collapsed: ReadonlySet<string> = new Set(),
  parent = '',
  depth = 0,
  indent = '',
): TreeRow[] {
  return nodes.flatMap((node, index): TreeRow[] => {
    const isLast = index === nodes.length - 1
    const guide = `${indent}${isLast ? GUIDE_LAST : GUIDE_MIDDLE}`
    if ('file' in node) {
      const path = `${parent}${node.file}`
      const landed = node.landed
      return [{ path, name: node.file, depth, guide, isDir: false, status: node.status, landed, changedBelow: 0 }]
    }
    const path = `${parent}${node.dir}`
    const changedBelow = countChanged(node.children)
    const folder: TreeRow = { path, name: node.dir, depth, guide, isDir: true, status: 'clean', landed: false, changedBelow }
    if (collapsed.has(path)) return [folder]
    const below = `${indent}${isLast ? GUIDE_BLANK : GUIDE_PASS}`
    return [folder, ...flattenRows(node.children, collapsed, `${path}/`, depth + 1, below)]
  })
}

/** `filePath` relative to the repo `root`; null for a path outside the repo. */
export function repoPath(root: string, filePath: string): string | null {
  const prefix = root.endsWith('/') ? root : `${root}/`
  return filePath.startsWith(prefix) ? filePath.slice(prefix.length) : null
}

/**
 * A folder shows whether it is open, and, closed, how many changed files it hides.
 *
 * A file Claude is editing right now, changed or not yet, takes Claude's colour in bold. A file
 * Claude changed in this session keeps a `✎` in Claude's colour. Otherwise a changed file takes
 * its status colour, a letter for the status, and a `·` for a change that is not Claude's; it is
 * bold until it is committed, then dim. Unchanged files are dim while something else changed,
 * and plain in a tree where nothing did.
 */
export function rowLook(row: TreeRow, marks: TreeMarks): RowLook {
  if (row.isDir) {
    const isClosed = marks.collapsed.has(row.path)
    const style = { bold: true, color: marks.palette.folder }
    const glyph = isClosed ? GLYPH_FOLDER_CLOSED : GLYPH_FOLDER_OPEN
    if (!isClosed || row.changedBelow === 0) return { glyph, style, badge: '' }
    return { glyph, style, badge: String(row.changedBelow), badgeColor: marks.palette.modified }
  }
  const isActive = marks.claudeActive.has(row.path)
  if (row.status === 'clean') {
    if (isActive) {
      return { glyph: GLYPH_CLAUDE, glyphColor: marks.palette.claude, style: { color: marks.palette.claude, bold: true }, badge: '' }
    }
    return { glyph: GLYPH_NONE, style: marks.hasChanges ? { dimColor: true } : {}, badge: '' }
  }
  const isClaude = isActive || marks.claudeTouched.has(row.path)
  const statusStyle = { color: marks.palette[row.status], bold: !row.landed, dimColor: row.landed }
  return {
    glyph: isClaude ? GLYPH_CLAUDE : GLYPH_USER,
    ...(isClaude ? { glyphColor: marks.palette.claude } : {}),
    style: isActive ? { color: marks.palette.claude, bold: true } : statusStyle,
    badge: STATUS_BADGES[row.status],
    badgeColor: marks.palette[row.status],
  }
}

/** The top bar's line about what Claude is editing; null when it is editing nothing. */
export function activeLine(paths: readonly string[]): string | null {
  const names = [...new Set(paths.map(path => path.split('/').pop() ?? path))]
  const [first, ...others] = names
  if (first === undefined) return null
  return others.length === 0 ? `Claude is editing ${first}` : `Claude is editing ${first} +${others.length}`
}

/** `paths` without its first `path`: two calls on one file end one at a time. */
export function withoutFirst(paths: readonly string[], path: string): string[] {
  const index = paths.indexOf(path)
  return index === -1 ? [...paths] : [...paths.slice(0, index), ...paths.slice(index + 1)]
}

// Columns between the owl and his words, and the fewest the words need beside the owl.
export const TOP_BAR_ART_GAP = 2
const TOP_BAR_MIN_TEXT_COLUMNS = 18
// The owl needs this many rows in the pane; a shorter pane draws his words alone.
const TOP_BAR_MIN_ROWS_FOR_ART = 14
// The rule under the top bar.
const TOP_BAR_RULE_ROWS = 1

/** What the top bar is made of, and the room the pane gives it. */
export type TopBarSize = {
  artRows: number
  artColumns: number
  /** Rows of terry's words: one line each, since they truncate. */
  wordRows: number
  bodyColumns: number
  bodyRows: number
}

/** Where the owl goes in the top bar, and how many rows the bar takes, its rule included. */
export type TopBar = { hasArt: boolean; isBeside: boolean; rows: number }

/**
 * The owl stands beside his words when the words keep enough columns, above them in a narrower
 * pane, and is left out of a pane too short for him. The tree scrolls under these rows.
 */
export function topBar(size: TopBarSize): TopBar {
  const hasArt = size.artColumns <= size.bodyColumns && size.bodyRows >= TOP_BAR_MIN_ROWS_FOR_ART
  const isBeside = hasArt && size.artColumns + TOP_BAR_ART_GAP + TOP_BAR_MIN_TEXT_COLUMNS <= size.bodyColumns
  const barRows = !hasArt ? size.wordRows : isBeside ? Math.max(size.artRows, size.wordRows) : size.artRows + size.wordRows
  return { hasArt, isBeside, rows: barRows + TOP_BAR_RULE_ROWS }
}
