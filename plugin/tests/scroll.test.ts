import { expect, test } from 'claude-code/testing'

import { wheelOffset } from '../hooks/scroll'

const TREE = { bodyRows: 40, contentRows: 300 }

test('should move the set number of rows when one wheel tick scrolls down', () => {
  expect(wheelOffset({ ...TREE, offset: 11, by: 1 }, 5)).toBe(15)
})

test('should move the set number of rows when one wheel tick scrolls up', () => {
  expect(wheelOffset({ ...TREE, offset: 9, by: -1 }, 5)).toBe(5)
})

test('should stop at the top when the move would pass it', () => {
  expect(wheelOffset({ ...TREE, offset: 1, by: -1 }, 5)).toBe(0)
})

test('should stop at the last full window when the move would pass the end', () => {
  expect(wheelOffset({ ...TREE, offset: 259, by: 1 }, 5)).toBe(260)
})

test('should keep the engine move when the speed is one row per tick', () => {
  expect(wheelOffset({ ...TREE, offset: 12, by: 2 }, 1)).toBe(12)
})
