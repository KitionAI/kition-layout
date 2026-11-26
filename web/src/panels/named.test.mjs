// Named things by hand (plan 2026-11-25, phase 11), in Node: Parent Page Options' and Delete
// Parent Spread's values, Object Style Options' `style.object.edit`, a gradient swatch's stops
// (added, moved, removed) and its `swatch.add` / `swatch.set`, the Pages panel's chosen pages and
// section marks, the new menu rows and strings in both languages, and (once `pnpm --dir web engine`
// has built the engine) each of them taken by the engine and read back from `inspect()`. Run with
// `pnpm --dir web test:unit` (node --test).
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'
import assert from 'node:assert/strict'
import { basedOnChoices, pagesUsing, parentDraft, parentEdit, parentReplacements, prefixProblem } from './parentRules.ts'
import { objectBasedOnChoices, objectStyleDraft, objectStyleEdit, objectStyleReplacements } from './objectStyleRules.ts'
import { addStop, colorAt, cssGradient, gradientDraft, gradientEdit, gradientParam, moveStop, newGradientDraft, removeStop } from './gradientRules.ts'
import { swatchReplacements } from './swatchRules.ts'
import { choosePages, pagesText } from './panelLogic.ts'
import { movedTo, movePlan, sectionStarts, sectionValues } from '../dialogs/pages.ts'
import { contextRows } from '../menus/contextRows.ts'
import { dialogFor } from '../dialogs/routes.ts'
import { CATALOGS, PHRASES, withEllipsis } from '../strings.ts'

const parents = [
  { id: 1, label: 'A-Parent', prefix: 'A', name: 'Parent', basedOn: null, pages: 2, items: 3 },
  { id: 2, label: 'B-Chapter', prefix: 'B', name: 'Chapter', basedOn: 'A-Parent', pages: 2, items: 1 },
  { id: 3, label: 'C-Back', prefix: 'C', name: 'Back', basedOn: 'B-Chapter', pages: 1, items: 0 },
]

test('Parent Page Options set only what changed; no parent is based on one based on it', () => {
  const from = parentDraft(parents[1])
  assert.deepEqual(from, { prefix: 'B', name: 'Chapter', basedOn: 'A-Parent', pages: 2 })
  assert.equal(parentEdit('B-Chapter', from, from, parents), null, 'nothing changed')
  assert.deepEqual(parentEdit('B-Chapter', from, { ...from, name: ' Body ', basedOn: '', pages: 1 }, parents), { parent: 'B-Chapter', name: 'Body', basedOn: null, pages: 1 })
  assert.equal(parentEdit('B-Chapter', from, { ...from, prefix: 'A' }, parents), null, 'a taken prefix')
  assert.equal(parentEdit('B-Chapter', from, { ...from, pages: 11 }, parents), null)
  assert.equal(prefixProblem('ABCDE', parents, 'B-Chapter'), 'long')
  assert.equal(prefixProblem('X-Y', parents, 'B-Chapter'), 'hyphen')
  assert.equal(prefixProblem('B', parents, 'B-Chapter'), '', 'its own prefix')
  // A is the base of B and (through B) of C: A may be based on neither, C on A or B.
  assert.deepEqual(basedOnChoices(parents, 'A-Parent'), [])
  assert.deepEqual(basedOnChoices(parents, 'C-Back'), ['A-Parent', 'B-Chapter'])
  assert.deepEqual(parentReplacements(parents, 'B-Chapter'), ['', 'A-Parent', 'C-Back'])
  assert.equal(pagesUsing([{ pages: [{ parent: 'A-Parent' }, { parent: 'B-Chapter' }] }, { pages: [{ parent: 'B-Chapter' }] }], 'B-Chapter'), 2)
})

const box = {
  name: 'Box',
  basedOn: null,
  own: { fill: 'Ink', fillTint: 1, stroke: null, strokeTint: null, strokeWeight: null, paragraphStyle: null, textFrame: null },
  resolved: { fill: 'Ink', fillTint: 1, stroke: null, strokeTint: null, strokeWeight: null, paragraphStyle: null, textFrame: null },
  uses: 2,
}

test('Object Style Options open with what the style includes and edit only what changed', () => {
  const from = objectStyleDraft(box, 'Box')
  assert.deepEqual([from.fill.on, from.fill.swatch, from.fill.tint, from.stroke.on, from.frame.on], [true, 'Ink', '100', false, false])
  assert.equal(objectStyleEdit('Box', from, from), null)
  const to = { ...from, fill: { ...from.fill, tint: '50' }, stroke: { ...from.stroke, on: true, swatch: '[Black]', weight: '2' } }
  assert.deepEqual(objectStyleEdit('Box', from, to), { name: 'Box', fillTint: 0.5, stroke: '[Black]', strokeWeight: 2, strokeTint: 1 })
  assert.deepEqual(objectStyleEdit('Box', from, { ...from, fill: { ...from.fill, on: false } }), { name: 'Box', fill: null })
  const frame = { ...from, frame: { on: true, columns: '2', gutter: '12', inset: ['6', '6', '6', '6'], vj: 'center' } }
  assert.deepEqual(objectStyleEdit('Box', from, frame).textFrame, { columns: 2, gutter: 12, inset: [6, 6, 6, 6], verticalJustification: 'center' })
  assert.equal(objectStyleEdit('Box', from, { ...frame, frame: { ...frame.frame, columns: '0' } }), null, 'no columns')
  // The rename waits for OK.
  assert.equal(objectStyleEdit('Box', from, { ...from, name: 'Card' }), null)
  assert.deepEqual(objectStyleEdit('Box', from, { ...from, name: 'Card' }, true), { name: 'Box', rename: 'Card' })
  const defs = [{ ...box, name: '[None]' }, box, { ...box, name: 'Kid', basedOn: 'Box' }]
  assert.deepEqual(objectBasedOnChoices(defs, 'Box'), [], 'Kid is based on Box')
  assert.deepEqual(objectBasedOnChoices(defs, 'Kid'), ['Box'])
  assert.deepEqual(objectStyleReplacements(defs, 'Box'), ['[None]', 'Kid'])
})

test('a gradient swatch: stops added where clicked, dragged along, dragged off; what changed', () => {
  const start = newGradientDraft('Sky')
  assert.equal(colorAt(start.stops, 50).model, 'rgb')
  assert.deepEqual(colorAt(start.stops, 50).rgb, [128, 128, 128], 'half way, half grey')
  const added = addStop(start.stops, 25)
  assert.deepEqual([added.index, added.stops.map((stop) => stop.location)], [1, [0, 25, 100]])
  assert.deepEqual(added.stops[1].color.rgb, [191, 191, 191])
  const moved = moveStop(added.stops, 1, 120)
  assert.deepEqual([moved.index, moved.stops.map((stop) => stop.location)], [1, [0, 100, 100]], 'clamped and kept in order')
  assert.equal(removeStop(added.stops, 1).length, 2)
  assert.equal(removeStop(start.stops, 0).length, 2, 'two stay')
  const param = gradientParam({ ...start, kind: 'radial', stops: [{ ...start.stops[0], midpoint: 5 }, start.stops[1]] })
  assert.deepEqual(param, { kind: 'radial', stops: [{ location: 0, color: '#ffffff', midpoint: 13 }, { location: 100, color: '#000000', midpoint: 50 }] })
  const entry = { kind: 'linear', stops: [{ hex: '#00a0e9', cmyk: [100, 0, 0, 0], location: 0, midpoint: 30 }, { hex: '#ff0000', location: 100, midpoint: 50 }] }
  const opened = gradientDraft('Sea', entry)
  assert.deepEqual(opened.stops[0].color, { model: 'cmyk', cmyk: [100, 0, 0, 0] })
  assert.equal(gradientEdit('Sea', opened, opened), null)
  assert.deepEqual(gradientEdit('Sea', opened, { ...opened, kind: 'radial' })?.gradient?.kind, 'radial')
  assert.deepEqual(gradientEdit('Sea', opened, { ...opened, name: 'Ocean' }, true), { name: 'Sea', rename: 'Ocean' })
  assert.equal(cssGradient(entry), 'linear-gradient(to right, #00ffff 0%, #ff0000 100%)')
  // The chip in the Swatches panel shows each stop as the page does (phase 12).
  assert.equal(cssGradient(entry, { asOnPage: true }), 'linear-gradient(to right, #00a0e9 0%, #ff0000 100%)')
  // A deleted gradient may take another gradient's place; a colour may not.
  const swatches = [{ name: '[Black]', kind: 'color' }, { name: 'Sea', kind: 'gradient' }, { name: 'Sky', kind: 'gradient' }]
  assert.deepEqual(swatchReplacements(swatches, 'Sea'), ['[Black]', 'Sky'])
  assert.deepEqual(swatchReplacements([...swatches, { name: 'Ink', kind: 'color' }], 'Ink'), ['[Black]'])
})

test('the Pages panel chooses pages together; the page rows act on all; sections are marked', () => {
  assert.deepEqual(choosePages([1], 1, 4, { range: true, toggle: false }), { pages: [1, 2, 3, 4], anchor: 1 })
  assert.deepEqual(choosePages([1, 2], 2, 5, { range: false, toggle: true }), { pages: [1, 2, 5], anchor: 5 })
  assert.deepEqual(choosePages([1, 2], 2, 2, { range: false, toggle: true }), { pages: [1], anchor: 2 })
  assert.deepEqual(choosePages([3], 3, 3, { range: false, toggle: true }), { pages: [3], anchor: 3 }, 'one page stays chosen')
  assert.equal(pagesText([4, 1, 2, 3, 7]), '2-5, 8')
  const plan = { pages: [2, 3], where: 'after', page: 5, count: 6 }
  assert.deepEqual(movedTo(plan), [3, 4])
  let order = [0, 1, 2, 3, 4, 5]
  for (const { from, to } of movePlan(plan)) order.splice(to, 0, order.splice(from, 1)[0])
  assert.deepEqual(order, [0, 3, 4, 1, 2, 5], 'pages 2 and 3 after page 5')
  const sections = [{ page: 1, style: 'arabic' }, { page: 3, startNumber: 1, style: 'lowerRoman', prefix: 'A', includePrefix: true, marker: 'Intro' }]
  assert.deepEqual(sectionValues(sections, 2), { page: 3, startNumber: 1, style: 'lowerRoman', prefix: 'A', includePrefix: true, marker: 'Intro' })
  assert.deepEqual(sectionValues(sections, 1), { page: 2 })
  assert.deepEqual([...sectionStarts(sections)], [[0, ''], [2, 'Intro']])
  const ids = (spot) => contextRows(spot).filter((row) => row !== '-').map((row) => row.id)
  assert.ok(ids({ kind: 'page', page: 0 }).includes('layout.section'))
  assert.deepEqual(ids({ kind: 'parent', parent: 'A-Parent' }).slice(0, 3), ['layout.parents.set', 'layout.parents.duplicate', 'layout.parents.delete'])
  assert.deepEqual(contextRows({ kind: 'parent', parent: 'A-Parent' })[0], { id: 'layout.parents.set', params: { parent: 'A-Parent' } })
  assert.ok(ids({ kind: 'page', page: 0, parent: 'A-Parent' }).includes('layout.parents.delete'), 'the panel menu has the chosen parent')
  assert.equal(dialogFor('layout.parents.set'), 'parentOptions')
  assert.equal(dialogFor('layout.parents.delete'), 'parentDelete')
  assert.equal(dialogFor('layout.parents.duplicate'), null, 'a copy needs no question')
})

test('every new string and menu title in both languages', () => {
  const own = (catalog) => Object.keys(catalog).filter((key) => /^(parent|objectStyle|gradient)\./.test(key)).sort()
  assert.ok(own(CATALOGS['en-US']).length > 40)
  assert.deepEqual(own(CATALOGS['zh-CN']), own(CATALOGS['en-US']))
  const phrase = withEllipsis((english) => PHRASES['zh-CN'][english] ?? english)
  for (const title of ['Parent Page Options…', 'Duplicate Parent Spread', 'Delete Parent Spread…', 'Object Styles', 'Numbering & Section Options…']) assert.notEqual(phrase(title), title, title)
})

const engineDir = new URL('../../public/engine/', import.meta.url)
const sdk = existsSync(new URL('kition_layout_kit_bg.wasm', engineDir))
  ? await (async () => {
      const module = await import(new URL('kition_layout_kit.js', engineDir).href)
      module.initSync({ module: readFileSync(new URL('kition_layout_kit_bg.wasm', engineDir)) })
      return module
    })()
  : null
const skip = sdk ? false : 'the web engine is not built'

test('the engine takes what the new dialogs send and reads it back', { skip }, () => {
  const doc = sdk.LayoutDocument.open('Sample.klayout', readFileSync(new URL('../../public/sample/Sample.klayout', import.meta.url)))
  const run = (command, params = {}) => JSON.parse(doc.execute(command, JSON.stringify(params)) || 'null')
  const inspected = () => JSON.parse(doc.inspect())
  // Parents: duplicate, options, delete.
  run('layout.parents.duplicate', { parent: 'A-Parent' })
  const b = () => inspected().parents.find((parent) => parent.prefix === 'B')
  const draft = parentDraft(b())
  run('layout.parents.set', parentEdit(b().label, draft, { ...draft, name: 'Chapter', basedOn: 'A-Parent', pages: 1 }, inspected().parents))
  assert.deepEqual([b().label, b().basedOn, b().pages], ['B-Chapter', 'A-Parent', 1])
  run('layout.pages.applyParent', { pages: [1], parent: 'B-Chapter' })
  run('layout.parents.delete', { parent: 'B-Chapter', replaceWith: 'A-Parent' })
  assert.equal(b(), undefined)
  assert.equal(inspected().spreads[0].pages[0].parent, 'A-Parent')
  // Sections: read back for the panel's mark and the dialog's values.
  run('layout.section', { page: 2, startNumber: 5, style: 'upperRoman', marker: 'Two' })
  assert.deepEqual(sectionValues(inspected().sections, 1), { page: 2, startNumber: 5, style: 'upperRoman', prefix: '', includePrefix: false, marker: 'Two' })
  // Object styles: an edit reaches the object; an override shows; Clear Overrides takes it away.
  run('swatch.add', { name: 'Ink', color: '#108080' })
  const frame = run('frame.add', { spread: 0, rect: [40, 40, 140, 140], content: 'graphic' })
  run('style.object.add', { name: 'Box', fromSelection: false, fill: 'Ink' })
  run('style.object.apply', { name: 'Box', ids: [frame.id] })
  const def = () => inspected().styleDefs.object.find((style) => style.name === 'Box')
  const item = () => inspected().spreads.flatMap((spread) => spread.items).find((one) => one.id === frame.id)
  const opened = objectStyleDraft(def(), 'Box')
  run('style.object.edit', objectStyleEdit('Box', opened, { ...opened, stroke: { ...opened.stroke, on: true, swatch: 'Ink', weight: '3' } }))
  assert.deepEqual([def().own.stroke, def().own.strokeWeight, def().uses], ['Ink', 3, 1])
  assert.deepEqual([item().objectStyle, item().styleOverride, item().stroke.swatch], ['Box', false, 'Ink'])
  run('object.fill', { ids: [frame.id], swatch: '[Paper]' })
  assert.equal(item().styleOverride, true, 'the "+"')
  run('style.object.apply', { name: 'Box', ids: [frame.id] })
  assert.equal(item().styleOverride, false, 'Clear Overrides')
  run('style.object.delete', { name: 'Box', replaceWith: '[None]' })
  assert.equal(item().objectStyle, '[None]')
  // A gradient swatch: made, read back, changed, on a stroke.
  const made = run('swatch.add', { name: 'Sky', gradient: gradientParam(newGradientDraft('Sky')) })
  assert.equal(made.name, 'Sky')
  const sky = () => inspected().colors.Sky.gradient
  assert.deepEqual(sky().stops.map((stop) => [stop.hex, stop.location]), [['#ffffff', 0], ['#000000', 100]])
  const stops = addStop(gradientDraft('Sky', sky()).stops, 40)
  run('swatch.set', gradientEdit('Sky', gradientDraft('Sky', sky()), { name: 'Sky', kind: 'radial', stops: stops.stops }))
  assert.deepEqual([sky().kind, sky().stops.length], ['radial', 3])
  run('object.stroke', { ids: [frame.id], swatch: 'Sky' })
  assert.equal(inspected().colors.Sky.uses, 1)
})
