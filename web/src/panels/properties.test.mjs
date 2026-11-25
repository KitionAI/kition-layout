// The Properties panel's rules (plan 2026-11-25, stage 4), in Node: colours in RGB and CMYK, the
// reference point, the fixed-value buttons (each a menu row), X and D, which layer new objects go
// on, a font file's family names, the strings in both languages, and (once `pnpm --dir web engine`
// has built the engine) every value the panel sends taken by the engine and read back from
// `inspect()`. Run with `pnpm --dir web test:unit` (node --test).
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'
import assert from 'node:assert/strict'
import { cmykToRgb, colorParam, hexToRgb, rgbToCmyk, rgbToHex, sameColor, swatchColor } from './colors.ts'
import { ARROWHEADS, CORNER_SHAPES, LANGUAGES, PANEL_ROWS, PARAGRAPH_ALIGNS, STROKE_TYPES, defaultFillStroke, keptProportion, layerToActivate, refOffset, shownPosition, strokeType, swapFillStroke } from './panelLogic.ts'
import { VARIANTS, VARIANT_LABELS } from '../menus/catalog.ts'
import { fontFamilies } from '../fonts.ts'
import { CATALOGS, PHRASES } from '../strings.ts'

test('colours go between hex, RGB and CMYK as the picker shows them', () => {
  assert.deepEqual(hexToRgb('#5645d4'), [0x56, 0x45, 0xd4])
  assert.deepEqual(hexToRgb('fff'), [255, 255, 255])
  assert.equal(hexToRgb('blue'), null)
  assert.equal(rgbToHex([0x56, 0x45, 0xd4]), '#5645d4')
  assert.deepEqual(cmykToRgb([0, 0, 0, 0]), [255, 255, 255])
  assert.deepEqual(cmykToRgb([0, 100, 100, 0]), [255, 0, 0])
  assert.deepEqual(rgbToCmyk([255, 0, 0]), [0, 100, 100, 0])
  assert.deepEqual(rgbToCmyk([0, 0, 0]), [0, 0, 0, 100])
  // A colour chosen in CMYK reaches the engine as CMYK percentages, kept as CMYK.
  assert.deepEqual(colorParam({ model: 'cmyk', cmyk: [10, 20, 30, 40] }), { c: 10, m: 20, y: 30, k: 40 })
  assert.equal(colorParam({ model: 'rgb', rgb: [255, 0, 0] }), '#ff0000')
  assert.deepEqual(swatchColor({ hex: '#ff0000', cmyk: [0, 100, 100, 0] }), { model: 'cmyk', cmyk: [0, 100, 100, 0] })
  assert.deepEqual(swatchColor({ hex: '#00ff00' }), { model: 'rgb', rgb: [0, 255, 0] })
  assert.equal(swatchColor(undefined), null)
  assert.ok(sameColor({ model: 'rgb', rgb: [1, 2, 3] }, { model: 'rgb', rgb: [1, 2, 3] }))
})

test('X and Y read at the reference point; linked sizes keep the proportions', () => {
  assert.deepEqual(refOffset(0), [0, 0])
  assert.deepEqual(refOffset(4), [0.5, 0.5])
  assert.deepEqual(refOffset(8), [1, 1])
  assert.deepEqual(shownPosition({ x: 10, y: 20, width: 100, height: 50 }, 4), { x: 60, y: 45 })
  assert.deepEqual(shownPosition({ x: 10, y: 20, width: 100, height: 50 }, 2), { x: 110, y: 20 })
  assert.equal(keptProportion({ width: 100, height: 50 }, 'width', 200), 100)
  assert.equal(keptProportion({ width: 100, height: 50 }, 'height', 100), 200)
  assert.equal(keptProportion({ width: 0, height: 50 }, 'height', 100), null)
})

test('every fixed-value button of the panel is a menu row, with its label', () => {
  for (const row of PANEL_ROWS) {
    assert.ok(VARIANTS.has(row), row)
    assert.ok(VARIANT_LABELS.get(row), row)
    assert.ok(PHRASES['zh-CN'][VARIANT_LABELS.get(row)], `zh-CN: ${VARIANT_LABELS.get(row)}`)
  }
  assert.deepEqual(strokeType('dashed', 2), { kind: 'dashed', pattern: [6, 4] })
  assert.deepEqual(strokeType('dotted', 2), { kind: 'dotted' })
})

test('X swaps fill and stroke, D gives the default ones', () => {
  const items = [
    { id: 1, fill: 'Red', fillTint: 0.5, stroke: { swatch: '[Black]', weight: 1 }, strokeDetail: { tint: 1 } },
    { id: 2, fill: 'Red', fillTint: 0.5, stroke: { swatch: '[Black]', weight: 3 }, strokeDetail: { tint: 1 } },
    { id: 3, fill: '[None]', stroke: { swatch: 'Blue', weight: 1 } },
  ]
  assert.deepEqual(swapFillStroke(items), [
    ['object.fill', { swatch: '[Black]', tint: 1, ids: [1, 2] }],
    ['object.stroke', { swatch: 'Red', tint: 0.5, ids: [1, 2] }],
    ['object.fill', { swatch: 'Blue', tint: 1, ids: [3] }],
    ['object.stroke', { swatch: '[None]', tint: 1, ids: [3] }],
  ])
  assert.deepEqual(defaultFillStroke([4]), [
    ['object.fill', { swatch: '[None]', ids: [4] }],
    ['object.stroke', { swatch: '[Black]', tint: 1, weight: 1, ids: [4] }],
  ])
})

test('new objects go on the selection\'s layer, not on an empty layer a document opened on', () => {
  const layers = [
    { id: 16, visible: true, locked: false },
    { id: 1, visible: true, locked: false },
    { id: 5, visible: true, locked: true },
  ]
  const counts = new Map([[1, 3], [5, 2]])
  // Opened on the empty "Notes" layer: the content layer instead.
  assert.equal(layerToActivate(layers, counts, 16, []), 1)
  // A layer that has objects stays.
  assert.equal(layerToActivate(layers, counts, 1, []), null)
  // Selected objects on one layer make it active; on several, or on a locked one, nothing changes.
  assert.equal(layerToActivate(layers, counts, 1, [16]), 16)
  assert.equal(layerToActivate(layers, counts, 16, [1, 16]), null)
  assert.equal(layerToActivate(layers, counts, 1, [5]), null)
  // Every layer empty: nothing to move to.
  assert.equal(layerToActivate(layers, new Map(), 16, []), null)
})

test('a font file names its family', () => {
  const bytes = readFileSync(new URL('../../../assets/fonts/SourceSerif4-Regular.ttf', import.meta.url))
  assert.deepEqual(fontFamilies(new Uint8Array(bytes)), ['Source Serif 4'])
  assert.deepEqual(fontFamilies(new Uint8Array([1, 2, 3])), [])
})

test('the panel\'s words are in English and Chinese', () => {
  const keys = [
    ...STROKE_TYPES.map((kind) => `stroke.${kind}`),
    ...ARROWHEADS.map((kind) => `arrow.${kind}`),
    ...CORNER_SHAPES.map((kind) => `corner.${kind}`),
    ...PARAGRAPH_ALIGNS.map((kind) => `align.${kind}`),
    ...LANGUAGES.map((name) => `lang.${name}`),
    ...[0, 1, 2, 3, 4, 5, 6, 7, 8].map((point) => `props.ref.${point}`),
  ]
  for (const key of keys) {
    assert.ok(CATALOGS['en-US'][key], `en-US ${key}`)
    assert.ok(CATALOGS['zh-CN'][key], `zh-CN ${key}`)
  }
  const own = (catalog) => Object.keys(catalog).filter((key) => /^(props|font|status|stroke|arrow|corner|lang|char|kerning|cap|join|vj|alignTo|gridAlign|strokeAlign)\./.test(key)).sort()
  assert.deepEqual(own(CATALOGS['zh-CN']), own(CATALOGS['en-US']))
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

test('the engine takes every value the panel sends and reads it back', { skip }, () => {
  const doc = sdk.LayoutDocument.open('Sample.klayout', readFileSync(new URL('../../public/sample/Sample.klayout', import.meta.url)))
  const run = (command, params = {}) => JSON.parse(doc.execute(command, JSON.stringify(params)) || 'null')
  const inspected = () => JSON.parse(doc.inspect())
  const itemOf = (id) => inspected().spreads.flatMap((spread) => spread.items).find((item) => item.id === id)
  const frame = run('frame.add', { spread: 0, rect: [40, 40, 240, 140], content: 'text', text: 'Kition aurora words here', caret: false })
  for (const kind of STROKE_TYPES) {
    run('object.stroke', { ids: [frame.id], type: strokeType(kind, 2) })
    assert.equal(itemOf(frame.id).strokeDetail.type.kind, kind)
  }
  for (const end of ARROWHEADS) {
    run('object.stroke', { ids: [frame.id], start: end, end })
    assert.equal(itemOf(frame.id).strokeDetail.end, end)
  }
  for (const shape of CORNER_SHAPES) {
    run('object.cornerOptions', { ids: [frame.id], shape, size: 6 })
    assert.equal(itemOf(frame.id).corner.shape, shape)
  }
  run('object.textFrameOptions', { ids: [frame.id], gutter: 9, inset: [null, 4, null, null], verticalJustification: 'bottom', columns: 2 })
  assert.deepEqual(itemOf(frame.id).textFrame.inset, [0, 4, 0, 0])
  assert.equal(itemOf(frame.id).textFrame.verticalJustification, 'bottom')
  run('object.textWrap', { ids: [frame.id], mode: 'boundingBox', offset: [1, 2, 3, 4] })
  assert.deepEqual(itemOf(frame.id).wrap.offsets, [1, 2, 3, 4])
  run('object.color', { ids: [frame.id], color: colorParam({ model: 'cmyk', cmyk: [0, 50, 100, 0] }), target: 'fill' })
  assert.deepEqual(swatchColor(inspected().colors[itemOf(frame.id).fill]), { model: 'cmyk', cmyk: [0, 50, 100, 0] })
  // Character and paragraph values, set on the frame's story and read back at the selection.
  run('select.set', { ids: [frame.id] })
  const chars = { underline: true, strikethrough: true, capitalization: 'smallCaps', position: 'superscript', kerning: { kind: 'optical' }, tracking: 50, hScale: 0.9, vScale: 1.1, baselineShift: 2, skew: 8, language: 'German: 2006 Reform' }
  run('text.char', { attrs: chars })
  const back = run('text.selectionAttrs').chars
  for (const [key, value] of Object.entries(chars)) assert.deepEqual(back[key], value, key)
  for (const align of PARAGRAPH_ALIGNS) {
    run('text.para', { attrs: { align } })
    assert.equal(run('text.selectionAttrs').para.align, align)
  }
  const para = { leftIndent: 12, rightIndent: 6, firstLineIndent: 18, lastLineIndent: 3, spaceBefore: 10, spaceAfter: 4, dropCapLines: 2, dropCapChars: 1, hyphenate: false, listType: 'bullets', gridAlign: 'allLines' }
  run('text.para', { attrs: para })
  const read = run('text.selectionAttrs').para
  for (const [key, value] of Object.entries(para)) assert.deepEqual(read[key], value, key)
  // The story's words for the status bar.
  assert.equal(inspected().stories.find((story) => story.frames.includes(frame.id)).words, 4)
  // The active layer the editor sets is the one new objects go on.
  const layers = inspected().layers.map((layer) => layer.id)
  for (const id of layers) {
    run('layer.activate', { id })
    assert.equal(inspected().activeLayer, id)
    assert.equal(run('frame.add', { spread: 0, rect: [300, 300, 340, 340], content: 'graphic' }) && itemOf(Math.max(...inspected().spreads[0].items.map((item) => item.id))).layer, id)
  }
})
