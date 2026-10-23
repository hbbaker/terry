/** A wheel move on the pane, in the rows `ui.scroll` reports. */
export type WheelMove = { offset: number; by: number; bodyRows: number; contentRows: number }

/**
 * Where the pane's window lands when each wheel row counts `rowsPerTick` rows. `offset`
 * already holds one row per tick (the engine's own move), so the rest is added on top,
 * then kept inside the tree.
 */
export function wheelOffset(move: WheelMove, rowsPerTick: number): number {
  const lastOffset = Math.max(0, move.contentRows - move.bodyRows)
  return Math.min(lastOffset, Math.max(0, move.offset + move.by * (rowsPerTick - 1)))
}
