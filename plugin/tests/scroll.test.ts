import { expect, test } from 'claude-code/testing'

import { lastOffset, nextColumn, nextOffset } from '../hooks/scroll'

const TREE = { totalRows: 300, windowRows: 37 }
const WHEEL = { contentRows: 41, isWheel: true }
const KEY = { contentRows: 41, isWheel: false }

test('should move the set number of rows when one wheel tick scrolls down', () => {
  expect(nextOffset(10, { ...WHEEL, by: 1 }, 5, TREE.totalRows, TREE.windowRows)).toBe(15)
})

test('should move the set number of rows when one wheel tick scrolls up', () => {
  expect(nextOffset(10, { ...WHEEL, by: -1 }, 5, TREE.totalRows, TREE.windowRows)).toBe(5)
})

test('should stop at the top when the move would pass it', () => {
  expect(nextOffset(2, { ...WHEEL, by: -1 }, 5, TREE.totalRows, TREE.windowRows)).toBe(0)
})

test('should stop at the last full window when the move would pass the end', () => {
  expect(nextOffset(260, { ...WHEEL, by: 1 }, 5, TREE.totalRows, TREE.windowRows)).toBe(263)
})

test('should move one row per arrow key', () => {
  expect(nextOffset(10, { ...KEY, by: 1 }, 5, TREE.totalRows, TREE.windowRows)).toBe(11)
})

test('should move a page when a page key is pressed', () => {
  expect(nextOffset(10, { ...KEY, by: 40 }, 5, TREE.totalRows, TREE.windowRows)).toBe(50)
})

test('should jump to the start and the end when Home and End are pressed', () => {
  expect(nextOffset(100, { ...KEY, by: -41 }, 5, TREE.totalRows, TREE.windowRows)).toBe(0)
  expect(nextOffset(100, { ...KEY, by: 41 }, 5, TREE.totalRows, TREE.windowRows)).toBe(263)
})

test('should stay at the top when the whole review fits the window', () => {
  expect(lastOffset(10, 37)).toBe(0)
  expect(nextOffset(0, { ...WHEEL, by: 1 }, 5, 10, 37)).toBe(0)
})

test('should move the diff sideways by one step when the right button is pressed', () => {
  expect(nextColumn(0, 1, 20, 100)).toBe(20)
  expect(nextColumn(20, -1, 20, 100)).toBe(0)
})

test('should stop at the left edge and at the furthest column', () => {
  expect(nextColumn(5, -1, 20, 100)).toBe(0)
  expect(nextColumn(90, 1, 20, 100)).toBe(100)
})
