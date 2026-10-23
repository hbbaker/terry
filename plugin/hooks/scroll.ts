/** A move of the pane's window, as `ui.scroll` reports it. */
export type PaneMove = { by: number; contentRows: number; isWheel: boolean }

/** Rows past the visible window; the pane keeps its own place, so this is the most it moves. */
export function lastOffset(totalRows: number, windowRows: number): number {
  return Math.max(0, totalRows - windowRows)
}

/**
 * Where the first shown row lands after `move`. A wheel tick counts `rowsPerTick` rows. The
 * pane draws one row more than it shows, so a move of exactly `contentRows` is Home or End
 * (a page key moves one row less); that jumps to the start or the end.
 */
export function nextOffset(
  current: number,
  move: PaneMove,
  rowsPerTick: number,
  totalRows: number,
  windowRows: number,
): number {
  const last = lastOffset(totalRows, windowRows)
  if (!move.isWheel && Math.abs(move.by) === move.contentRows) return move.by < 0 ? 0 : last
  const rows = move.isWheel ? move.by * rowsPerTick : move.by
  return Math.min(last, Math.max(0, current + rows))
}

/** Where the diff's left edge lands after a sideways step of `step` columns, `-1` or `1` times. */
export function nextColumn(current: number, direction: -1 | 1, step: number, last: number): number {
  return Math.min(last, Math.max(0, current + direction * step))
}
