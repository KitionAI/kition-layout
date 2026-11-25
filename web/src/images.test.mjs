// Images and objects by hand (plan 2026-11-25, stage 6), without a browser:
// node --test src/images.test.mjs (Node strips the modules' types).
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { escapeAction, loadRequests, placeBadge, unembedPath, unembeddable } from './images.ts'
import { VARIANT_KEYS, layoutMenus } from './menus/catalog.ts'
import { contextRows } from './menus/contextRows.ts'
import { DRAWN_CURSORS, drawnCursor } from './cursors.ts'
import { CATALOGS, PHRASES } from './strings.ts'
import { nextPress, takeCount } from './clicks.ts'

const info = (over = {}) => ({ selection: [7], selectionContent: false, placeQueue: [], ...over })

test('the fitting rows have their keys: Fill Cmd+Alt+Shift+C, Fit Cmd+Alt+Shift+E, Frame to Content Cmd+Alt+C, Content to Frame Cmd+Alt+E, Center Cmd+Shift+E', () => {
  // The walkthrough's "the fitting keys leave the scale at 25.2 %": the rows had no keys at all.
  const keys = new Map(VARIANT_KEYS)
  assert.equal(keys.get('Cmd+Alt+Shift+C'), 'object.fit|fillProportionally')
  assert.equal(keys.get('Cmd+Alt+Shift+E'), 'object.fit|fitProportionally')
  assert.equal(keys.get('Cmd+Alt+C'), 'object.fit|fitFrameToContent')
  assert.equal(keys.get('Cmd+Alt+E'), 'object.fit|fitContentToFrame')
  assert.equal(keys.get('Cmd+Shift+E'), 'object.fit|centerContent')
  // The menu shows them, and no two rows share a key.
  const rows = layoutMenus([{ id: 'object.fit', title: 'Fitting' }], () => false)
  assert.equal(rows.find((row) => row.command === 'object.fit|fitFrameToContent')?.shortcut, 'Cmd+Alt+C')
  assert.equal(new Set(VARIANT_KEYS.map(([key]) => key)).size, VARIANT_KEYS.length)
})

test('Escape takes a graphic back to its frame and drops the place cursor\'s front graphic', () => {
  assert.equal(escapeAction(info({ selectionContent: true }), 'selection'), 'frame')
  assert.equal(escapeAction(info(), 'selection'), null, 'a frame: the editor deselects as before')
  assert.equal(escapeAction(info(), 'placeGun'), 'placeSkip')
  assert.equal(escapeAction(null, 'selection'), null)
})

test('several graphics load the place cursor, the first replacing its load unless adding', () => {
  const files = [{ base64: 'AA', name: 'a.png' }, { base64: 'BB', name: 'b.jpg' }]
  assert.deepEqual(loadRequests(files, false), [{ base64: 'AA', name: 'a.png' }, { base64: 'BB', name: 'b.jpg', queue: true }])
  assert.deepEqual(loadRequests(files, true).map((request) => request.queue), [true, true])
  assert.equal(placeBadge([]), null)
  const queue = [{ asset: 3, name: 'a.png', width: 10, height: 10 }, { asset: 4, name: 'b.png', width: 10, height: 10 }]
  assert.deepEqual(placeBadge(queue), { count: 2, next: queue[0] })
})

test('an unembedded graphic goes beside the document under its own name, PNG and JPEG only', () => {
  assert.equal(unembedPath('work/Flyer.klayout', 'photo.png'), 'work/photo.png')
  assert.equal(unembedPath('Flyer.klayout', 'dir/photo.jpg'), 'photo.jpg')
  assert.equal(unembedPath('Flyer.klayout', ''), 'image.png')
  assert.ok(unembeddable('a.JPG') && unembeddable('b.png') && !unembeddable('c.tif'))
})

test('the Selection tool draws a hand over a graphic; the layer menu duplicates a layer; the new words have Chinese', () => {
  assert.equal(drawnCursor('selection', 'pointer'), DRAWN_CURSORS.contentHand)
  assert.equal(drawnCursor('hand', 'pointer'), 'pointer')
  assert.ok(contextRows({ kind: 'layer', layer: 2 }).some((row) => row !== '-' && row.id === 'layer.duplicate' && row.params?.id === 2))
  for (const key of ['placeLoaded', 'props.content', 'unembed', 'unembedded', 'unembedFailed']) {
    assert.ok(CATALOGS['en-US'][key] && CATALOGS['zh-CN'][key], key)
  }
  for (const phrase of ['Swap Fill and Stroke', 'Default Fill and Stroke', 'Duplicate Layer', 'Unembed Link']) assert.ok(PHRASES['zh-CN'][phrase], phrase)
})

test('each press keeps its own count when the editor sends it after the next press was counted', () => {
  // A fast double click: both presses are counted before the editor sends the first on. The
  // first is one press, the second two (it took the latest count for both, and toggled twice).
  const waiting = []
  let press = null
  for (const time of [1000, 1080]) {
    press = nextPress(press, time, 10, 10)
    waiting.push({ count: press.count, at: time })
  }
  assert.equal(takeCount(waiting, 1100), 1)
  assert.equal(takeCount(waiting, 1100), 2)
  assert.equal(takeCount(waiting, 1100), 1, 'none waiting: one')
  // A press nobody sent on (a view tool took it) is dropped once it waited too long.
  waiting.push({ count: 2, at: 0 }, { count: 1, at: 5000 })
  assert.equal(takeCount(waiting, 5100), 1)
})
