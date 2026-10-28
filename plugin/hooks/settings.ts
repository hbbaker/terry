import type { DisplaySettings, Palette } from '../types'

/**
 * What the pane uses when `terry config` cannot be read (`terry` is not on the PATH, or it is an
 * old build). They mirror `Display::default` in src/display.rs: change both together.
 */
export const DEFAULT_DISPLAY: DisplaySettings = {
  name: 'Terry',
  paneColumns: 34,
  colors: {
    added: 'success',
    modified: 'warning',
    deleted: 'error',
    untracked: '#7FD6D6',
    claude: 'claude',
    owl: '#8FB8ED',
    alert: 'warning',
    folder: '#8FB8ED',
    branches: ['#8FB8ED', '#9BD39B', '#F2C879', '#E49AD0', '#7FD6D6', '#F29E8E'],
  },
  problems: [],
}

// `terry config` has already checked every colour against the theme's names; this only keeps a
// value of the wrong kind from reaching the engine, which would refuse to draw the pane.
const COLOR_PATTERN = /^(#[0-9a-fA-F]{6}|[A-Za-z]+)$/

function isColor(value: unknown): value is string {
  return typeof value === 'string' && COLOR_PATTERN.test(value)
}

function isColorList(value: unknown): value is string[] {
  return Array.isArray(value) && value.length > 0 && value.every(isColor)
}

function isPalette(value: unknown): value is Palette {
  if (typeof value !== 'object' || value === null) return false
  const colors = value as Record<string, unknown>
  const single = ['added', 'modified', 'deleted', 'untracked', 'claude', 'owl', 'alert', 'folder']
  return single.every(key => isColor(colors[key])) && isColorList(colors['branches'])
}

/** Reads the JSON `terry config` prints; null when it is not that shape. */
export function parseDisplay(text: string): DisplaySettings | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return null
  }
  if (typeof parsed !== 'object' || parsed === null) return null
  const { name, paneColumns, colors, problems } = parsed as Record<string, unknown>
  if (typeof name !== 'string' || name.trim() === '') return null
  if (typeof paneColumns !== 'number' || !Number.isInteger(paneColumns)) return null
  if (!isPalette(colors)) return null
  const notes = Array.isArray(problems) ? problems.filter((problem): problem is string => typeof problem === 'string') : []
  return { name, paneColumns, colors, problems: notes }
}

/** The colour of one layer of the branch chain, from the default branch down; the layers repeat. */
export function branchColor(colors: Palette, depth: number): string {
  return colors.branches[depth % colors.branches.length] ?? colors.owl
}
