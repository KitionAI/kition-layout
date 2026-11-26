// Panels and styles by hand (plan 2026-11-25, stage 7), in Node: the style options' values and
// the edit their changes make, Swatch Options' `swatch.set`, the Effects dialog's commands,
// where a dragged page, layer or object goes, Layer Options' `layer.set`, the strings in both
// languages, and (once `pnpm --dir web engine` has built the engine) every edit taken by the
// engine and read back from `inspect()`. Run with `pnpm --dir web test:unit` (node --test).
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'
import assert from 'node:assert/strict'
import { STYLE_SECTIONS, copyName, duplicateParams, engineValue, replacementChoices, sectionsOf, shownValue, styleAttrs, styleDraft, styleEdit, styleGeneral } from './styleRules.ts'
import { swatchEdit, swatchReplacements } from './swatchRules.ts'
import { BLEND_MODES, EFFECT_PAGES, effectsDraft, effectsSteps } from './effects.ts'
import { LAYER_COLORS, dropSide, layerEdit, moveTarget, reorderIndex } from './reorder.ts'
import { contextRows, OWN_DIALOG_ROWS } from '../menus/contextRows.ts'
import { layoutMenus } from '../menus/catalog.ts'
import { asks } from '../dialogs/routes.ts'
import { pageHasObjects } from './panelLogic.ts'
import { CATALOGS, PHRASES, withEllipsis } from '../strings.ts'

const body = {
  name: 'Body',
  basedOn: null,
  nextStyle: null,
  para: { spaceAfter: 4 },
  chars: { size: 11 },
  resolvedPara: { align: 'left', leftIndent: 0, spaceAfter: 4, hyphenate: true, dropCapLines: 0 },
  resolvedChars: { fontFamily: 'Source Serif 4', size: 11, leading: { kind: 'auto' }, hScale: 1, underline: false, fill: '[Black]', fillTint: 1 },
  uses: 3,
}

test('the style options open with the style\'s own values and edit only what changed', () => {
  const draft = styleDraft('paragraph', body)
  assert.equal(draft.size, '11')
  assert.equal(draft.spaceAfter, '4')
  assert.equal(draft.leading, 'auto')
  assert.equal(draft.hScale, '100', 'a fraction shows as a percentage')
  assert.equal(draft.hyphenate, 'true')
  assert.equal(draft.align, 'left')
  const general = styleGeneral(body, 'Body')
  const opened = { general, draft }
  assert.equal(styleEdit('paragraph', 'Body', opened, opened), null, 'nothing changed')
  const edit = styleEdit('paragraph', 'Body', opened, { general: { ...general, basedOn: 'Head', name: 'Text' }, draft: { ...draft, size: '20', hScale: '90', leading: '14', align: 'center', hyphenate: 'false' } })
  assert.deepEqual(edit, { name: 'Body', para: { align: 'center', hyphenate: false }, chars: { size: 20, hScale: 0.9, leading: { kind: 'points', value: 14 } }, rename: 'Text', basedOn: 'Head' })
  assert.equal(styleEdit('paragraph', 'Body', opened, { general, draft: { ...draft, size: 'big' } }), null, 'not a number yet')
  assert.deepEqual(styleEdit('paragraph', 'Body', opened, { general, draft: { ...draft, leading: 'Auto' } }), null, 'auto was auto')
  // A character style leaves what it does not set to the text: '' unsets.
  const em = { name: 'Em', chars: { fontStyle: 'Italic' }, resolvedChars: { fontStyle: 'Italic', tracking: 20 } }
  const emDraft = styleDraft('character', em)
  assert.equal(emDraft.size, '')
  assert.equal(emDraft.tracking, '20')
  assert.deepEqual(styleEdit('character', 'Em', { general: styleGeneral(em, 'Em'), draft: emDraft }, { general: styleGeneral(em, 'Em'), draft: { ...emDraft, tracking: '', size: '9' } }), { name: 'Em', chars: { tracking: null, size: 9 } })
  // Character styles have no paragraph sections.
  assert.deepEqual(sectionsOf('character').map((section) => section.id), ['general', 'basicCharacter', 'advancedCharacter', 'characterColor'])
  assert.ok(sectionsOf('paragraph').some((section) => section.id === 'indentsSpacing'))
  assert.deepEqual(styleAttrs('character', { ...emDraft, size: '9' }).chars, { fontStyle: 'Italic', tracking: 20, size: 9 })
  assert.equal(shownValue({ key: 'fillTint', part: 'chars', kind: 'number', scale: 100 }, 0.5), '50')
  assert.deepEqual(engineValue({ key: 'fillTint', part: 'chars', kind: 'number', scale: 100 }, '25'), 0.25)
})

test('a duplicated style copies what it sets; a deleted one\'s text may take any other', () => {
  assert.equal(copyName('Body', ['Body', 'Body copy']), 'Body copy 2')
  assert.deepEqual(duplicateParams('paragraph', { ...body, basedOn: 'Base', nextStyle: 'Next' }, ['Body']), { name: 'Body copy', chars: { size: 11 }, basedOn: 'Base', para: { spaceAfter: 4 }, nextStyle: 'Next' })
  assert.deepEqual(replacementChoices('paragraph', ['[No Paragraph Style]', '[Basic Paragraph]', 'Body', 'Head'], 'Body'), ['[Basic Paragraph]', 'Head'])
  assert.deepEqual(replacementChoices('character', ['[None]', 'Em', 'Strong'], 'Em'), ['[None]', 'Strong'])
})

test('Swatch Options set only what changed; a deleted swatch\'s uses may take another colour', () => {
  const opened = { name: 'Ink', spot: false, color: { model: 'rgb', rgb: [0x33, 0x66, 0x99] }, tint: null }
  assert.equal(swatchEdit('Ink', opened, opened), null)
  assert.deepEqual(swatchEdit('Ink', opened, { ...opened, name: 'Ocean', spot: true }), { name: 'Ink', rename: 'Ocean', spot: true })
  assert.deepEqual(swatchEdit('Ink', opened, { ...opened, color: { model: 'cmyk', cmyk: [10, 20, 30, 40] } }), { name: 'Ink', color: { c: 10, m: 20, y: 30, k: 40 } })
  const tint = { name: 'Ink 50%', spot: false, color: null, tint: 50 }
  assert.deepEqual(swatchEdit('Ink 50%', tint, { ...tint, tint: 25 }), { name: 'Ink 50%', tint: 0.25 })
  assert.equal(swatchEdit('Ink 50%', tint, { ...tint, tint: 140 }), null)
  const swatches = [{ name: '[None]', kind: 'none' }, { name: '[Registration]', kind: 'registration' }, { name: '[Paper]', kind: 'paper' }, { name: '[Black]', kind: 'color' }, { name: 'Ink', kind: 'color' }, { name: 'Sky', kind: 'gradient' }]
  assert.deepEqual(swatchReplacements(swatches, 'Ink'), ['[Black]', '[None]', '[Paper]'])
})

test('the Effects dialog runs what changed, every effect with all its settings', () => {
  const item = { opacity: 0.5, blend: 'Multiply', effects: { dropShadow: { on: true, color: '[Black]', opacity: 0.75, angle: 135, distance: 7, size: 5, spread: 0 }, feather: 0, directionalFeather: { on: false, widths: [1, 2, 3, 4] } } }
  const opened = effectsDraft(item)
  assert.equal(opened.opacity, '50')
  assert.equal(opened.effects.dropShadow.opacity, '75')
  assert.equal(opened.effects.feather.on, 'false')
  assert.deepEqual(effectsSteps([7], opened, opened), [])
  const changed = structuredClone(opened)
  changed.opacity = '40'
  changed.effects.dropShadow.distance = '12'
  changed.effects.feather = { on: 'true', width: '6' }
  changed.effects.directionalFeather.on = 'true'
  assert.deepEqual(effectsSteps([7], opened, changed), [
    ['object.opacity', { ids: [7], opacity: 0.4, blend: 'Multiply' }],
    ['object.dropShadow', { ids: [7], color: '[Black]', opacity: 0.75, angle: 135, globalLight: false, distance: 12, size: 5, spread: 0, on: true }],
    ['object.feather', { ids: [7], width: 6 }],
    ['object.directionalFeather', { ids: [7], on: true, widths: [1, 2, 3, 4] }],
  ])
  changed.effects.dropShadow.distance = 'far'
  assert.equal(effectsSteps([7], opened, changed), null, 'not a number yet')
  // Turning Basic Feather off is a width of 0.
  const off = structuredClone(opened)
  off.effects.feather.width = '3'
  assert.deepEqual(effectsSteps([7], opened, off), [['object.feather', { ids: [7], width: 0 }]])
  assert.equal(EFFECT_PAGES.length, 9)
  assert.equal(new Set(BLEND_MODES).size, 16)
})

test('a dragged page, layer or object goes where it is dropped', () => {
  assert.equal(moveTarget(0, 3), 2, 'the first after the third of three')
  assert.equal(moveTarget(2, 0), 0)
  assert.equal(moveTarget(1, 1), null, 'before itself: it stays')
  assert.equal(moveTarget(1, 2), null, 'after itself: it stays')
  assert.equal(dropSide({ left: 0, top: 0, width: 100, height: 20 }, 10, 15, false), 'after')
  assert.equal(dropSide({ left: 0, top: 0, width: 100, height: 20 }, 10, 15, true), 'before')
  // A layer's objects front first: [11, 9, 7]; 17 dropped before 9 is one from the front.
  assert.equal(reorderIndex([17, 11, 9, 7], [17], 9), 1)
  assert.equal(reorderIndex([17, 11, 9, 7], [17], null), 0)
  assert.equal(reorderIndex([11, 9, 7], [11], 7), 1, 'the moved object is not counted')
  const layer = { name: 'Layer 1', color: [43, 155, 255], visible: true, locked: false, printable: true }
  assert.equal(layerEdit(1, layer, layer), null)
  assert.deepEqual(layerEdit(1, layer, { ...layer, name: 'Body', color: [255, 0, 0], printable: false }), { id: 1, name: 'Body', color: [255, 0, 0], printable: false })
  assert.equal(LAYER_COLORS.length, 16)
})

test('the right-click menus and the Object menu open the new dialogs, each in Chinese', () => {
  const ids = (spot) => contextRows(spot).filter((row) => row !== '-').map((row) => row.id)
  assert.ok(ids({ kind: 'object', textFrame: false, image: false, group: false, count: 1 }).includes('kit:object.effects'))
  assert.deepEqual(contextRows({ kind: 'layer', layer: 3 })[0], { id: 'kit:layer.options', params: { id: 3 } })
  const phrase = withEllipsis((english) => PHRASES['zh-CN'][english] ?? english)
  for (const { label } of Object.values(OWN_DIALOG_ROWS)) assert.notEqual(phrase(label), label, label)
  const effects = layoutMenus([{ id: 'object.dropShadow', title: 'Drop Shadow', menu: ['Object', 'Effects'] }], asks).find((row) => row.command === 'kit:object.effects')
  assert.deepEqual(effects?.path, ['Object', 'Effects'])
  // Every string of the stage in both languages.
  const own = (catalog) => Object.keys(catalog).filter((key) => /^(style|swatch|layer|layerColor|pages|effects|blend)\./.test(key)).sort()
  assert.deepEqual(own(CATALOGS['zh-CN']), own(CATALOGS['en-US']))
  for (const section of STYLE_SECTIONS) {
    assert.ok(CATALOGS['zh-CN'][`style.section.${section.id}`], section.id)
    for (const field of section.fields) assert.ok(CATALOGS['zh-CN'][`style.field.${field.key}`], field.key)
  }
  for (const page of EFFECT_PAGES) {
    assert.ok(CATALOGS['zh-CN'][`effects.${page.id}`], page.id)
    for (const field of page.fields) assert.ok(CATALOGS['zh-CN'][`effects.field.${field.key}`], field.key)
  }
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

test('the engine takes every edit the dialogs make and reads it back', { skip }, () => {
  const doc = sdk.LayoutDocument.open('Sample.klayout', readFileSync(new URL('../../public/sample/Sample.klayout', import.meta.url)))
  const run = (command, params = {}) => JSON.parse(doc.execute(command, JSON.stringify(params)) || 'null')
  const inspected = () => JSON.parse(doc.inspect())
  // Every style field the options set, read back as the dialog opens with it.
  run('style.paragraph.add', { name: 'Body' })
  const values = { fontStyle: 'Bold', size: '13', leading: '16', tracking: '25', capitalization: 'smallCaps', position: 'superscript', underline: 'true', strikethrough: 'true', hScale: '90', vScale: '110', baselineShift: '2', skew: '8', align: 'center', leftIndent: '12', rightIndent: '6', firstLineIndent: '18', lastLineIndent: '3', spaceBefore: '10', spaceAfter: '4', dropCapLines: '2', dropCapChars: '1', hyphenate: 'false', fill: '[Paper]', fillTint: '50', stroke: '[Black]', strokeWeight: '2' }
  const def = () => inspected().styleDefs.paragraph.find((style) => style.name === 'Body')
  const opened = { general: styleGeneral(def(), 'Body'), draft: styleDraft('paragraph', def()) }
  const edit = styleEdit('paragraph', 'Body', opened, { general: { ...opened.general, nextStyle: '[Basic Paragraph]' }, draft: { ...opened.draft, ...values } })
  run('style.paragraph.edit', edit)
  const back = styleDraft('paragraph', def())
  for (const [key, value] of Object.entries(values)) assert.equal(back[key], value, key)
  assert.equal(def().nextStyle, '[Basic Paragraph]')
  // Swatch Options: CMYK, spot, rename.
  run('swatch.add', { name: 'Ink', color: '#336699' })
  const color = { model: 'cmyk', cmyk: [10, 20, 30, 40] }
  run('swatch.set', swatchEdit('Ink', { name: 'Ink', spot: false, color: { model: 'rgb', rgb: [0x33, 0x66, 0x99] }, tint: null }, { name: 'Ocean', spot: true, color, tint: null }))
  assert.deepEqual(inspected().colors.Ocean.cmyk, [10, 20, 30, 40])
  assert.equal(inspected().colors.Ocean.spot, true)
  // Every effect the dialog turns on, read back on.
  const frame = run('frame.add', { spread: 0, rect: [40, 40, 240, 140], content: 'graphic' })
  const itemOf = () => inspected().spreads.flatMap((spread) => spread.items).find((item) => item.id === frame.id)
  const start = effectsDraft(itemOf())
  const all = structuredClone(start)
  all.opacity = '60'
  all.blend = 'Screen'
  for (const page of EFFECT_PAGES) all.effects[page.id].on = 'true'
  for (const [command, params] of effectsSteps([frame.id], start, all)) run(command, params)
  const read = effectsDraft(itemOf())
  assert.equal(read.opacity, '60')
  assert.equal(read.blend, 'Screen')
  for (const page of EFFECT_PAGES) assert.equal(read.effects[page.id].on, 'true', page.id)
  // Layer Options.
  const layer = inspected().layers[0]
  run('layer.set', layerEdit(layer.id, { ...layer }, { ...layer, name: 'Renamed', printable: false, color: [255, 0, 0] }))
  assert.deepEqual(inspected().layers[0], { ...inspected().layers[0], name: 'Renamed', printable: false, color: [255, 0, 0] })
})

test("the Pages panel's trash asks first only for a page with objects on it (phase 9)", () => {
  const spread = { pages: [{ index: 2, bounds: [0, 0, 600, 800] }, { index: 3, bounds: [600, 0, 1200, 800] }], items: [{ bounds: [100, 100, 200, 200] }] }
  assert.equal(pageHasObjects(spread, 2), true, 'an object on the left page')
  assert.equal(pageHasObjects(spread, 3), false, 'the right page is empty')
  assert.equal(pageHasObjects({ ...spread, items: [{ bounds: [550, 10, 650, 50] }] }, 3), true, 'an object across the spine is on both')
  assert.equal(pageHasObjects(spread, 9), false, 'a page not in the spread')
})
