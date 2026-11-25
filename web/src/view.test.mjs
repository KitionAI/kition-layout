// The view's commands and the right-click menus' rows (plan 2026-11-25, stage 2), without a
// browser: node --test src/view.test.mjs (Node strips the modules' types).
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { VIEW_ACTIONS, itemAt, pageArea, selectionArea, stepPage, viewMove, wheelZoomFactor, zoomAbout } from './view.ts'
import { contextRows } from './menus/contextRows.ts'
import { DRAWN_CURSORS, drawnCursor } from './cursors.ts'
import { VARIANTS, layoutMenus } from './menus/catalog.ts'
import { asks } from './dialogs/routes.ts'
import { PHRASES, withEllipsis } from './strings.ts'

/** Two facing spreads: page 1 alone on the right, then pages 2 and 3 (612 × 792 points each). */
const info = {
  canvas: { x0: 0, y0: 0, x1: 1400, y1: 1900 },
  spreads: [
    { index: 0, area: { x0: 36, y0: 36, x1: 648, y1: 828 }, left: 0, pages: [{ index: 0, name: '1', bounds: [0, 0, 612, 792] }], items: [{ id: 7, name: '', kind: '<rectangle>', layer: 1, bounds: [100, 100, 200, 150], hidden: false, locked: false }] },
    {
      index: 1,
      area: { x0: 36, y0: 900, x1: 1260, y1: 1692 },
      left: -612,
      pages: [
        { index: 1, name: '2', bounds: [-612, 0, 0, 792] },
        { index: 2, name: '3', bounds: [0, 0, 612, 792] },
      ],
      items: [
        { id: 8, name: '', kind: '<text frame>', layer: 1, bounds: [-500, 50, -300, 250], story: 3, hidden: false, locked: false },
        { id: 9, name: '', kind: '<rectangle>', layer: 1, bounds: [-450, 100, -350, 200], hidden: false, locked: true },
      ],
    },
  ],
  pageCount: 3,
  selection: [7, 8],
  layers: [{ id: 1, name: 'Layer 1', visible: true, locked: false, color: [0, 0, 255] }],
}

test('a page and the selection are measured in document pixels', () => {
  assert.deepEqual(pageArea(info, 0), { x0: 36, y0: 36, x1: 648, y1: 828 })
  // The left page of the second spread starts at the spread's left edge.
  assert.deepEqual(pageArea(info, 1), { x0: 36, y0: 900, x1: 648, y1: 1692 })
  assert.deepEqual(pageArea(info, 2), { x0: 648, y0: 900, x1: 1260, y1: 1692 })
  assert.equal(pageArea(info, 9), null)
  assert.deepEqual(selectionArea(info), { x0: 136, y0: 136, x1: 348, y1: 1150 })
  assert.equal(selectionArea({ ...info, selection: [] }), null)
})

test('the view actions fit the page, the spread, the selection, or set a zoom', () => {
  assert.deepEqual(viewMove('fitPage', info, 2), { area: pageArea(info, 2), page: 2 })
  assert.deepEqual(viewMove('fitSpread', info, 2), { area: info.spreads[1].area, page: 1 })
  assert.deepEqual(viewMove('actualSize', info, 0), { zoom: 1 })
  assert.deepEqual(viewMove('zoom200', info, 0), { zoom: 2 })
  assert.deepEqual(viewMove('fitSelection', info, 0), { area: selectionArea(info) })
  assert.deepEqual(viewMove('entirePasteboard', info, 0), { pasteboard: true })
})

test('pages and spreads step within the document', () => {
  assert.equal(stepPage(info, 0, -1), 0)
  assert.equal(stepPage(info, 2, 1), 2)
  assert.deepEqual(viewMove('nextPage', info, 0), { area: pageArea(info, 1), page: 1 })
  assert.deepEqual(viewMove('previousPage', info, 2), { area: pageArea(info, 1), page: 1 })
  assert.deepEqual(viewMove('firstPage', info, 2), { area: pageArea(info, 0), page: 0 })
  assert.deepEqual(viewMove('lastPage', info, 0), { area: pageArea(info, 2), page: 2 })
  assert.deepEqual(viewMove('nextSpread', info, 0), { area: info.spreads[1].area, page: 1 })
  assert.deepEqual(viewMove('nextSpread', info, 2), { area: info.spreads[1].area, page: 1 }, 'the last spread stays')
  assert.deepEqual(viewMove('previousSpread', info, 1), { area: info.spreads[0].area, page: 0 })
})

test('the view keys are InDesign\'s, Cmd+0 fitting the spread', () => {
  const keys = Object.fromEntries(VIEW_ACTIONS.map(([action, , shortcut]) => [action, shortcut]))
  assert.equal(keys.fitSpread, 'Cmd+0')
  assert.equal(keys.fitPage, 'Cmd+Alt+0')
  assert.equal(keys.actualSize, 'Cmd+1')
  assert.equal(keys.zoom200, 'Cmd+2')
  assert.equal(keys.nextPage, 'Shift+PageDown')
  assert.equal(keys.nextSpread, 'PageDown')
  assert.equal(new Set(Object.values(keys)).size, VIEW_ACTIONS.length, 'no key twice')
})

test('a wheel notch zooms by at most 1.2 times, about the pointer', () => {
  assert.ok(Math.abs(wheelZoomFactor(-100) - 1.2) < 1e-9)
  assert.ok(Math.abs(wheelZoomFactor(100) - 1 / 1.2) < 1e-9)
  assert.ok(Math.abs(wheelZoomFactor(-1000) - 1.2) < 1e-9, 'a fast wheel is still one step')
  assert.ok(Math.abs(wheelZoomFactor(-3, 1) - 1.2) < 1e-9, 'three lines are a notch')
  assert.ok(wheelZoomFactor(-5) > 1 && wheelZoomFactor(-5) < 1.02, 'a pinch zooms in proportion')
  const view = { zoom: 1, x: 100, y: 200 }
  const next = zoomAbout(view, 2, 50, 80)
  assert.equal(next.zoom, 2)
  // The document point under the pointer stays under it.
  assert.equal(view.x + 50 / view.zoom, next.x + 50 / next.zoom)
  assert.equal(view.y + 80 / view.zoom, next.y + 80 / next.zoom)
})

test('a right-click finds the topmost unlocked object under the pointer', () => {
  assert.equal(itemAt(info, info.spreads[0], 150, 120)?.id, 7)
  assert.equal(itemAt(info, info.spreads[0], 10, 10), null)
  // The locked rectangle is over the text frame: the click goes through it.
  assert.equal(itemAt(info, info.spreads[1], -400, 150)?.id, 8)
})

test('the right-click menus offer the rows of each place, run as the menu bar runs them', () => {
  const ids = (spot) => contextRows(spot).filter((row) => row !== '-').map((row) => row.id)
  const object = ids({ kind: 'object', textFrame: true, image: false, group: false, count: 1 })
  for (const id of ['edit.cut', 'edit.copy', 'edit.paste', 'edit.pasteInPlace', 'transform.rotate|cw90', 'arrange.bringToFront', 'object.textFrameOptions', 'object.lock']) assert.ok(object.includes(id), id)
  assert.ok(!object.includes('object.fit|fitProportionally'), 'no fitting without an image')
  assert.ok(ids({ kind: 'object', textFrame: false, image: true, group: false, count: 1 }).includes('object.fit|fitProportionally'))
  assert.ok(ids({ kind: 'object', textFrame: false, image: false, group: false, count: 2 }).includes('arrange.group'))
  assert.ok(ids({ kind: 'object', textFrame: false, image: false, group: true, count: 1 }).includes('arrange.ungroup'))
  const text = ids({ kind: 'text' })
  for (const id of ['edit.cut', 'edit.copy', 'edit.paste', 'edit.selectAll', 'text.insert|bullet', 'object.textFrameOptions']) assert.ok(text.includes(id), id)
  const canvas = ids({ kind: 'canvas' })
  for (const id of ['edit.paste', 'kit:view.fitPage', 'kit:view.actualSize', 'kit:view.rulers', 'guide.deleteAll']) assert.ok(canvas.includes(id), id)
  assert.deepEqual(contextRows({ kind: 'ruler', spread: 2 }).find((row) => row !== '-' && row.id === 'guide.deleteAll'), { id: 'guide.deleteAll', params: { spread: 2 } })
  assert.ok(ids({ kind: 'page', page: 0 }).includes('layout.pages.insert'))
  assert.deepEqual(contextRows({ kind: 'layer', layer: 4 }).find((row) => row !== '-' && row.id === 'layer.delete'), { id: 'layer.delete', params: { id: 4 } })
  // Every variant row is the menu bar's; separators never lead, trail or come twice.
  for (const spot of [{ kind: 'object', textFrame: true, image: true, group: true, count: 3 }, { kind: 'text' }, { kind: 'canvas' }, { kind: 'page', page: 1 }, { kind: 'layer', layer: 1 }, { kind: 'ruler', spread: 0 }]) {
    const rows = contextRows(spot)
    for (const row of rows) if (row !== '-' && row.id.includes('|')) assert.ok(VARIANTS.has(row.id), row.id)
    assert.notEqual(rows[0], '-')
    assert.notEqual(rows.at(-1), '-')
    rows.forEach((row, index) => assert.ok(row !== '-' || rows[index - 1] !== '-', `${spot.kind}: no doubled separator`))
  }
})

test('the View menu lists the view actions, each in Chinese', () => {
  const rows = layoutMenus([{ id: 'view.showTextThreads', title: 'Show Text Threads' }, { id: 'guide.deleteAll', title: 'Delete All Guides on Spread' }], asks).filter((row) => row.path[0] === 'View' && row.command !== '---')
  const phrase = withEllipsis((english) => PHRASES['zh-CN'][english] ?? english)
  for (const [action, label, shortcut] of VIEW_ACTIONS) {
    const row = rows.find((item) => item.command === `kit:view.${action}`)
    assert.ok(row, action)
    assert.equal(row.shortcut, shortcut)
    assert.notEqual(phrase(label), label, `${label} has a Chinese name`)
  }
  assert.ok(rows.some((row) => row.command === 'kit:view.rulers'))
})

test('the engine\'s shared cursors are drawn by the active tool', () => {
  assert.equal(drawnCursor('loadedText', 'copy'), DRAWN_CURSORS.loadedText)
  assert.equal(drawnCursor('placeGun', 'copy'), DRAWN_CURSORS.loadedGraphic)
  assert.equal(drawnCursor('eyedropper', 'copy'), DRAWN_CURSORS.eyedropper)
  assert.equal(drawnCursor('selection', 'grab'), DRAWN_CURSORS.rotate)
  assert.equal(drawnCursor('directSelection', 'default'), DRAWN_CURSORS.hollowArrow)
  assert.equal(drawnCursor('selection', 'default'), 'default')
  assert.equal(drawnCursor('hand', 'grab'), 'grab')
  assert.equal(drawnCursor('type', 'text'), 'text')
  for (const [name, css] of Object.entries(DRAWN_CURSORS)) {
    assert.match(css, /^url\("data:image\/svg\+xml,%3Csvg [^"<>#]*"\) \d+ \d+, [a-z-]+$/, name)
  }
})
