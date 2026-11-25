// The view options and ruler guides (plan 2026-11-25, stage 3), without a browser:
// node --test src/viewOptions.test.mjs (Node strips the modules' types).
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SCREEN_MODES, VIEW_DEFAULTS, VIEW_STORAGE_KEY, VIEW_TOGGLES, engineView, loadView, saveView, viewChange, viewChecked, viewRowLabel } from './viewOptions.ts'
import { guideAt, listedGuides, snapGuide, snapTargets } from './guides.ts'
import { contextRows } from './menus/contextRows.ts'
import { layoutMenus } from './menus/catalog.ts'
import { asks } from './dialogs/routes.ts'
import { PHRASES, CATALOGS, withEllipsis } from './strings.ts'

const memory = () => {
  const kept = new Map()
  return { getItem: (key) => kept.get(key) ?? null, setItem: (key, value) => kept.set(key, value) }
}

test('the view rows toggle their option, and the screen modes and units choose one', () => {
  let view = { ...VIEW_DEFAULTS }
  for (const [key] of VIEW_TOGGLES) {
    const next = viewChange(`kit:view.${key}`, view)
    assert.equal(next[key], !view[key], key)
    assert.equal(viewChecked(`kit:view.${key}`, next), next[key])
  }
  view = viewChange('kit:view.screenMode.preview', view)
  assert.equal(view.screenMode, 'preview')
  assert.equal(viewChecked('kit:view.screenMode.preview', view), true)
  assert.equal(viewChange('kit:view.togglePreview', view).screenMode, 'normal')
  assert.equal(viewChange('kit:view.unit.mm', view).unit, 'mm')
  assert.equal(viewChange('kit:view.unit.furlongs', view), null)
  assert.equal(viewChange('kit:view.fitPage', view), null, 'a fit is no view option')
  assert.equal(viewChecked('kit:view.fitPage', view), undefined)
  // The engine is told everything it draws and snaps by, in its own unit names.
  const told = engineView({ ...VIEW_DEFAULTS, unit: 'mm', guides: false }, { selected: { spread: 0, page: 0, index: 2 } })
  assert.deepEqual([told.unit, told.guides, told.selectedGuide, told.movingGuide], ['millimeters', false, { spread: 0, page: 0, index: 2 }, null])
  assert.ok(!('rulers' in told) && !('lockGuides' in told), 'the editor keeps its own')
})

test('the options last for the person, each bad one at its default, the screen mode always Normal', () => {
  const storage = memory()
  saveView(storage, { ...VIEW_DEFAULTS, guides: false, unit: 'mm', screenMode: 'preview' })
  const back = loadView(storage)
  assert.deepEqual([back.guides, back.unit, back.screenMode], [false, 'mm', 'normal'])
  storage.setItem(VIEW_STORAGE_KEY, JSON.stringify({ guides: 'yes', unit: 'furlong' }))
  assert.deepEqual(loadView(storage), VIEW_DEFAULTS)
  storage.setItem(VIEW_STORAGE_KEY, '{nope')
  assert.deepEqual(loadView(storage), VIEW_DEFAULTS)
  assert.deepEqual(loadView(null), VIEW_DEFAULTS)
})

test('the View, Type and Edit menus and the right-click menus offer the options, each in Chinese', () => {
  const rows = layoutMenus([{ id: 'view.showTextThreads', title: 'Show Text Threads' }, { id: 'guide.deleteAll', title: 'Delete All Guides on Spread' }], asks)
  const commands = rows.map((row) => row.command)
  for (const [key] of VIEW_TOGGLES) assert.ok(commands.includes(`kit:view.${key}`), key)
  for (const [mode] of SCREEN_MODES) assert.ok(commands.includes(`kit:view.screenMode.${mode}`), mode)
  assert.ok(commands.includes('kit:edit.preferences'))
  const phrase = withEllipsis((english) => PHRASES['zh-CN'][english] ?? english)
  for (const row of rows.filter((item) => item.command.startsWith('kit:'))) assert.notEqual(phrase(row.label), row.label, row.label)
  const canvas = contextRows({ kind: 'canvas' }).filter((row) => row !== '-').map((row) => row.id)
  for (const id of ['kit:view.togglePreview', 'kit:view.guides', 'kit:view.snapToGuides', 'kit:view.smartGuides']) {
    assert.ok(canvas.includes(id), id)
    assert.notEqual(phrase(viewRowLabel(id)), viewRowLabel(id), id)
  }
  const ruler = contextRows({ kind: 'ruler', spread: 0 }).filter((row) => row !== '-').map((row) => row.id)
  for (const unit of ['pt', 'pc', 'in', 'mm', 'cm', 'px']) {
    assert.ok(ruler.includes(`kit:view.unit.${unit}`), unit)
    assert.ok(CATALOGS['zh-CN'][`unit.${unit}`], unit)
  }
})

/** One spread, a Letter page at document pixels (72, 72), with two guides. */
const info = {
  canvas: { x0: 0, y0: 0, x1: 756, y1: 936 },
  spreads: [{ index: 0, area: { x0: 72, y0: 72, x1: 684, y1: 864 }, left: 0, items: [{ id: 5, bounds: [100, 200, 300, 260], hidden: false }], pages: [{ index: 0, bounds: [0, 0, 612, 792], margins: { top: 36, bottom: 36, inside: 36, outside: 36 }, columns: 2 }] }],
}
const guides = listedGuides(0, [
  { page: 0, index: 0, orientation: 'horizontal', position: 300, spread: false },
  { page: 0, index: 1, orientation: 'vertical', position: 400, spread: false },
  { nonsense: true },
])

test('a press finds the guide within 3 screen pixels, nearest first', () => {
  assert.equal(guides.length, 2)
  assert.equal(guideAt(info, guides, 200, 72 + 301, 1)?.index, 0)
  assert.equal(guideAt(info, guides, 72 + 402, 500, 1)?.index, 1)
  assert.equal(guideAt(info, guides, 200, 72 + 305, 1), null, 'too far at 100%')
  assert.equal(guideAt(info, guides, 200, 72 + 301, 4), null, 'a point is four pixels at 400%')
  assert.equal(guideAt(info, guides, 20, 72 + 300, 1), null, 'a page guide stops at its page')
})

test('a dragged guide snaps to the page, margins, columns, guides and objects, Shift to the ticks', () => {
  const spread = info.spreads[0]
  const across = snapTargets(spread, guides, 'horizontal', null)
  for (const y of [0, 396, 792, 36, 756, 300, 200, 230, 260]) assert.ok(across.includes(y), `y ${y}`)
  const down = snapTargets(spread, guides, 'vertical', guides[1])
  for (const x of [0, 306, 612, 36, 576, 306, 100, 200, 300]) assert.ok(down.includes(x), `x ${x}`)
  assert.ok(!down.includes(400), 'not to itself')
  assert.equal(snapGuide(298, across, 1), 300)
  assert.equal(snapGuide(298, across, 0.25), 300, 'farther at a smaller zoom')
  assert.equal(snapGuide(310, across, 1), 310, 'nothing near')
  assert.equal(snapGuide(43, [], 1, { shift: true, unit: 'pt' }), 40, 'the ticks of points at 100%')
  assert.ok(Math.abs(snapGuide(30, [], 1, { shift: true, unit: 'mm' }) - 10 * 72 / 25.4) < 1e-6, 'the ticks of millimetres')
})
