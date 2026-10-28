// Terry, a snowy owl on a branch, drawn in the band above the prompt.

/** While the gate is on and nothing waits: eyes closed, at rest. */
export const TERRY_CALM: readonly string[] = [
  '    ╭─────╮',
  '  ╭─╯◠ ▾ ◠╰─╮',
  '  ╲ ·  ·  · ╱',
  '   ╲ · · · ╱',
  '    ╲ · · ╱',
  '━━━━━╲┷┷┷╱━━━━━',
]

/** While terry holds a command for the user's review: eyes wide. */
export const TERRY_ALERT: readonly string[] = [
  '    ╭─────╮   !',
  '  ╭─╯◉ ▾ ◉╰─╮ !',
  '  ╲ ·  ·  · ╱',
  '   ╲ · · · ╱',
  '    ╲ · · ╱',
  '━━━━━╲┷┷┷╱━━━━━',
]

/** A smaller terry for the file tree pane, at rest: the ears are the ends of a solid line over the rounded head. */
export const TERRY_SMALL_CALM: readonly string[] = [
  '  ^───^',
  '╭─╯◠▾◠╰─╮',
  '╲ · · · ╱',
  ' ╲ · · ╱',
  '━━╲┷┷┷╱━━',
]

/** A smaller terry for the file tree pane, while he holds a command: eyes wide. */
export const TERRY_SMALL_ALERT: readonly string[] = [
  '  ^───^   !',
  '╭─╯◉▾◉╰─╮',
  '╲ · · · ╱',
  ' ╲ · · ╱',
  '━━╲┷┷┷╱━━',
]
