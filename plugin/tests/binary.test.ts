import { expect, test } from 'claude-code/testing'

import { binaryFromSettings, unquote } from '../hooks/binary'

/** The settings `terry install` writes: `configure` in src/install.rs, with `bash` and `edit` hooks. */
function settingsWith(binary: string): string {
  return JSON.stringify({
    hooks: {
      PreToolUse: [
        { matcher: 'Read', hooks: [{ type: 'command', command: '/usr/local/bin/other-hook' }] },
        { matcher: 'Bash', hooks: [{ type: 'command', command: `${binary} gate bash`, timeout: 90 }] },
        { matcher: 'Edit|Write', hooks: [{ type: 'command', command: `${binary} gate edit`, timeout: 30 }] },
      ],
    },
  })
}

test('should read the binary of the Bash gate hook', () => {
  expect(binaryFromSettings(settingsWith('/Users/a/.cargo/bin/terry'))).toBe('/Users/a/.cargo/bin/terry')
})

test('should undo the quotes shell_quote puts around a path with a space or a quote', () => {
  expect(binaryFromSettings(settingsWith("'/Users/a b/.cargo/bin/terry'"))).toBe('/Users/a b/.cargo/bin/terry')
  expect(binaryFromSettings(settingsWith("'/Users/o'\\''neil/.cargo/bin/terry'"))).toBe("/Users/o'neil/.cargo/bin/terry")
})

test('should return null when no hook is the Bash gate', () => {
  expect(binaryFromSettings(JSON.stringify({ hooks: { PreToolUse: [] } }))).toBeNull()
  expect(binaryFromSettings(JSON.stringify({ permissions: { ask: [] } }))).toBeNull()
  expect(binaryFromSettings(JSON.stringify({ hooks: { PreToolUse: [{ hooks: [{ command: 'ls gate bash' }] }] } }))).toBeNull()
})

test('should return null when the settings are not JSON or not the expected shape', () => {
  expect(binaryFromSettings('{ not json')).toBeNull()
  expect(binaryFromSettings('[]')).toBeNull()
  expect(binaryFromSettings(JSON.stringify({ hooks: { PreToolUse: 'Bash' } }))).toBeNull()
  expect(binaryFromSettings(JSON.stringify({ hooks: { PreToolUse: [{ hooks: [{ command: 5 }] }] } }))).toBeNull()
})

test('should take only an absolute path', () => {
  expect(binaryFromSettings(settingsWith('terry'))).toBeNull()
  expect(binaryFromSettings(settingsWith('target/release/terry'))).toBeNull()
})

test('should refuse text that shell_quote could not have written', () => {
  expect(unquote('')).toBeNull()
  expect(unquote('/a b/terry')).toBeNull()
  expect(unquote("'/a'b/terry'")).toBeNull()
  expect(unquote('/a/terry')).toBe('/a/terry')
})
