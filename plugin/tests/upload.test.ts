import { expect, test } from 'claude-code/testing'

import { bannerRows, droppedPath, fitNote, noteRows, parseUpload, uploadMessage, uploadName } from '../hooks/upload'

const WINDOWS_DROP = '"D:\\Downloads\\Corporate Dashboard as a Museum Masterpiece.png"'

test('should take the pasted path when a Windows file is dropped', () => {
  expect(droppedPath({ inputText: WINDOWS_DROP })).toBe(WINDOWS_DROP)
})

test('should take the pasted path when a Unix file is dropped, quoted or with escaped spaces', () => {
  expect(droppedPath({ inputText: "'/home/me/a b.png'" })).toBe("'/home/me/a b.png'")
  expect(droppedPath({ inputText: '/home/me/a\\ b.png ' })).toBe('/home/me/a\\ b.png')
  expect(droppedPath({ inputText: 'file:///home/me/a%20b.png' })).toBe('file:///home/me/a%20b.png')
})

test('should take no path when the edit is a key press', () => {
  expect(droppedPath({ key: { key: '/' }, inputText: '/' })).toBeNull()
})

test('should take no path when the pasted text is not a path', () => {
  expect(droppedPath({ inputText: 'fix the build please' })).toBeNull()
  expect(droppedPath({ inputText: 'see /tmp/x for details' })).toBeNull()
})

test('should take no path when several files are dropped at once', () => {
  expect(droppedPath({ inputText: '"D:\\a.png" "D:\\b.png"' })).toBeNull()
})

test('should take no path when the paste has more than one line', () => {
  expect(droppedPath({ inputText: '/home/me/a.png\n/home/me/b.png' })).toBeNull()
})

test('should name the file after the last separator when the path is quoted and Windows', () => {
  expect(uploadName(WINDOWS_DROP)).toBe('Corporate Dashboard as a Museum Masterpiece.png')
  expect(uploadName('/home/me/a.png')).toBe('a.png')
})

test('should read the destination when terry upload says it worked', () => {
  expect(parseUpload('{"ok":true,"dest":"src/a.png"}\n', '')).toEqual({ ok: true, dest: 'src/a.png' })
})

test('should read the error when terry upload says it failed', () => {
  expect(parseUpload('{"ok":false,"error":"no such folder"}', '')).toEqual({ ok: false, error: 'no such folder' })
})

test('should use stderr as the error when the answer is not JSON', () => {
  expect(parseUpload('', 'terry: not found\n')).toEqual({ ok: false, error: 'terry: not found' })
  expect(parseUpload('', '')).toEqual({ ok: false, error: 'terry upload gave no answer' })
})

test('should word the answer by whether the upload worked', () => {
  expect(uploadMessage({ ok: true, dest: 'src/a.png' })).toBe('Uploaded src/a.png')
  expect(uploadMessage({ ok: false, error: 'no' })).toBe('Upload failed: no')
})

test('should count two rows for the prompt and its buttons, and the rows of the answer', () => {
  expect(bannerRows(false, 0)).toBe(0)
  expect(bannerRows(true, 0)).toBe(2)
  expect(bannerRows(true, 3)).toBe(5)
})

test('should count the rows an answer takes when it wraps, up to the most it may take', () => {
  expect(noteRows(null, 40)).toBe(0)
  expect(noteRows('x'.repeat(81), 40)).toBe(3)
  expect(noteRows('x'.repeat(5000), 40)).toBe(10)
})

test('should cut the answer with an ellipsis only when it is longer than the rows allow', () => {
  expect(fitNote('short', 40)).toBe('short')
  expect(fitNote('x'.repeat(5000), 40)).toHaveLength(400)
  expect(fitNote('x'.repeat(5000), 40).endsWith('…')).toBe(true)
})
