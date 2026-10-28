import { expect, test } from 'claude-code/testing'

import { DEFAULT_DISPLAY, branchColor, parseDisplay } from '../hooks/settings'

function printed(change: Record<string, unknown> = {}): string {
  return JSON.stringify({ ...DEFAULT_DISPLAY, ...change })
}

test('should read what terry config prints when it is complete', () => {
  const settings = parseDisplay(printed({ name: 'Hedwig', paneColumns: 40, problems: ['display.name: too long'] }))

  expect(settings?.name).toBe('Hedwig')
  expect(settings?.paneColumns).toBe(40)
  expect(settings?.colors.claude).toBe('claude')
  expect(settings?.problems).toEqual(['display.name: too long'])
})

test('should give back the defaults when terry config prints them', () => {
  expect(parseDisplay(printed())).toEqual(DEFAULT_DISPLAY)
})

test('should return null when the text is not JSON', () => {
  expect(parseDisplay('usage: terry ...')).toBeNull()
})

test('should return null when the name is missing or empty', () => {
  expect(parseDisplay(printed({ name: '   ' }))).toBeNull()
  expect(parseDisplay(printed({ name: undefined }))).toBeNull()
})

test('should return null when the width is not a whole number', () => {
  expect(parseDisplay(printed({ paneColumns: 34.5 }))).toBeNull()
  expect(parseDisplay(printed({ paneColumns: '34' }))).toBeNull()
})

test('should return null when a colour is not text of the kind the engine knows', () => {
  expect(parseDisplay(printed({ colors: { ...DEFAULT_DISPLAY.colors, added: 5 } }))).toBeNull()
  expect(parseDisplay(printed({ colors: { ...DEFAULT_DISPLAY.colors, added: 'not a colour' } }))).toBeNull()
  expect(parseDisplay(printed({ colors: { ...DEFAULT_DISPLAY.colors, added: '#12345' } }))).toBeNull()
})

test('should return null when the branch colours are empty or hold a bad entry', () => {
  expect(parseDisplay(printed({ colors: { ...DEFAULT_DISPLAY.colors, branches: [] } }))).toBeNull()
  expect(parseDisplay(printed({ colors: { ...DEFAULT_DISPLAY.colors, branches: ['#8FB8ED', 3] } }))).toBeNull()
})

test('should accept a hex colour in either case and a theme colour name', () => {
  const colors = { ...DEFAULT_DISPLAY.colors, added: '#aabbcc', modified: '#AABBCC', deleted: 'diffRemoved' }

  expect(parseDisplay(printed({ colors }))?.colors.added).toBe('#aabbcc')
  expect(parseDisplay(printed({ colors }))?.colors.deleted).toBe('diffRemoved')
})

test('should keep no problems when the field is missing', () => {
  expect(parseDisplay(printed({ problems: undefined }))?.problems).toEqual([])
})

test('should repeat the branch colours when the chain is deeper than the list', () => {
  const colors = { ...DEFAULT_DISPLAY.colors, branches: ['#111111', '#222222'] }

  expect([0, 1, 2, 3].map(depth => branchColor(colors, depth))).toEqual(['#111111', '#222222', '#111111', '#222222'])
})
