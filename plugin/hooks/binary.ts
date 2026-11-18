/**
 * Which terry binary the pane runs: the one the gate's hooks run. `terry install` writes its own
 * path into `~/.claude/settings.json` as `<path> gate bash` (`hook_command` in src/install.rs), so
 * the pane reads it from there and does not depend on `PATH`. Change both together.
 */

/** What the pane runs when no hook names a binary: `terry`, looked up on `PATH`. */
export const FALLBACK_BINARY = 'terry'

// The words `hook_command` puts after the path for the Bash gate.
const BASH_GATE_SUFFIX = ' gate bash'
// `shell_quote` in src/text.rs writes a `'` inside quotes as `'\''`.
const QUOTED_QUOTE = "'\\''"

/**
 * Undoes `shell_quote` in src/text.rs: a word with only safe characters as it is, anything else in
 * single quotes. Null for text that `shell_quote` could not have written.
 */
export function unquote(text: string): string | null {
  if (text.length >= 2 && text.startsWith("'") && text.endsWith("'")) {
    const inner = text.slice(1, -1)
    const parts = inner.split(QUOTED_QUOTE)
    return parts.some(part => part.includes("'")) ? null : parts.join("'")
  }
  return text === '' || /['"\\\s]/.test(text) ? null : text
}

function hookCommands(settings: unknown): string[] {
  if (typeof settings !== 'object' || settings === null) return []
  const hooks = (settings as Record<string, unknown>)['hooks']
  if (typeof hooks !== 'object' || hooks === null) return []
  const entries = (hooks as Record<string, unknown>)['PreToolUse']
  if (!Array.isArray(entries)) return []
  return entries.flatMap(entry => {
    const inner = typeof entry === 'object' && entry !== null ? (entry as Record<string, unknown>)['hooks'] : null
    if (!Array.isArray(inner)) return []
    return inner.flatMap(hook => {
      const command = typeof hook === 'object' && hook !== null ? (hook as Record<string, unknown>)['command'] : null
      return typeof command === 'string' ? [command] : []
    })
  })
}

/**
 * The absolute path of the binary terry's Bash gate hook runs, from the text of Claude Code's
 * settings.json. Null when the text is not JSON, or no hook is terry's Bash gate.
 */
export function binaryFromSettings(text: string): string | null {
  let settings: unknown
  try {
    settings = JSON.parse(text)
  } catch {
    return null
  }
  for (const command of hookCommands(settings)) {
    if (!command.endsWith(BASH_GATE_SUFFIX)) continue
    const path = unquote(command.slice(0, -BASH_GATE_SUFFIX.length))
    if (path !== null && path.startsWith('/') && /(^|\/)terry$/.test(path)) return path
  }
  return null
}
