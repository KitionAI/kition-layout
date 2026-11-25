// The menus' and dialogs' pure logic (plan 2026-11-25, stage 1): which dialog a command opens,
// the generic dialog's fields, the catalog's rows and variants, the page dialogs' arithmetic,
// units, the greyed-out rows' reasons and the phrases. Run with `pnpm --dir web test:unit`.
import test from 'node:test'
import assert from 'node:assert/strict'
import { asks, dialogFor, GENERIC, PURPOSE_BUILT } from './routes.ts'
import { choiceFromDescription, defaultFromParams, fieldFor, humanize, initialValues, listOf, paramFields, paramsOf, rangeFromDescription, requiredKeys, valuesValid } from './params.ts'
import { insertPlan, movePlan, pagesFromText } from './pages.ts'
import { CATALOG_COMMANDS, VARIANT, VARIANTS, engineCommandOf, layoutMenus } from '../menus/catalog.ts'
import { HIDDEN, hiddenReason } from '../menus/hidden.ts'
import { contextReason, disabledReason, placeParams, placeValues, refusalReason } from '../menus/context.ts'
import { fromPoints, toPoints } from '../units.ts'
import { CATALOGS, PHRASES, withEllipsis } from '../strings.ts'
import { scaleLabel } from '../exporting.ts'

test('a command that asks opens its dialog; one that does not runs; a hidden one opens none', () => {
  assert.equal(dialogFor('object.textFrameOptions'), 'textFrameOptions')
  assert.equal(dialogFor('layout.pages.insert'), 'insertPages')
  assert.equal(dialogFor('table.insert'), 'insertTable')
  assert.equal(dialogFor('find.find'), 'findChange')
  assert.equal(dialogFor('object.cornerOptions'), 'params')
  assert.equal(dialogFor('object.group'), null)
  assert.equal(dialogFor('note.new'), null)
  assert.equal(asks('edit.stepAndRepeat'), true)
  assert.equal(asks('edit.undo'), false)
  // No command is both purpose-built and generic, and no generic one is hidden.
  for (const command of GENERIC) {
    assert.equal(PURPOSE_BUILT[command], undefined, command)
    assert.equal(hiddenReason(command), null, command)
  }
})

test('the generic dialog asks for a command\'s own values, not the keys every command shares', () => {
  const corner = { id: 'object.cornerOptions', params: '{shape: none|rounded, size, ids?}', schema: { ids: { type: 'array', items: { type: 'integer' } }, shape: { enum: ['none', 'rounded'], type: 'string' }, size: { type: 'number' } } }
  assert.deepEqual(paramFields(corner).map((field) => field.key), ['shape', 'size'])
  assert.deepEqual(requiredKeys(corner), ['shape', 'size'])
  // Keys the engine reads beside its listed ones are never asked.
  assert.equal(fieldFor({ id: 'x' }, 'edge', { type: 'string', description: 'read by the command beside its listed parameters' }), null)
  // A choice written into a description is a drop-down; a range in one bounds a number.
  assert.deepEqual(choiceFromDescription('leftToRight|rightToLeft'), ['leftToRight', 'rightToLeft'])
  assert.equal(choiceFromDescription('points'), null)
  assert.deepEqual(rangeFromDescription('0 to 1000'), { min: 0, max: 1000 })
  assert.deepEqual(rangeFromDescription('1..1000'), { min: 1, max: 1000 })
  assert.equal(defaultFromParams('{rows?: 0..1000, rowGutter?: 12}', 'rowGutter'), 12)
  assert.equal(defaultFromParams('{rows?: 0..1000}', 'rows'), null)
  // An effect's dialog is for turning it on.
  assert.equal(fieldFor({ id: 'object.dropShadow' }, 'on', { type: 'boolean' }).initial, true)
  // Points are labelled.
  assert.equal(fieldFor({ id: 'x' }, 'gutter', { type: 'number', description: 'points' }).unit, 'pt')
  // Type on a Path's options leave Delete to its own row.
  assert.deepEqual(paramFields({ id: 'type.pathOptions', schema: { delete: { type: 'boolean', default: true }, flip: { type: 'boolean' } } }).map((field) => field.key), ['flip'])
  // Add Page to Spread asks which spread, a key other commands share.
  assert.deepEqual(paramFields({ id: 'layout.pages.toSpread', schema: { page: { type: 'integer' }, spread: { oneOf: [{ type: 'integer' }, { type: 'object' }] } } }).map((field) => field.key), ['page', 'spread'])
})

test('lists are typed as comma-separated values and need what the command needs', () => {
  const pages = fieldFor({ id: 'layout.pages.applyParent' }, 'pages', { type: 'array', items: { type: 'integer' } })
  assert.equal(pages.kind, 'list')
  assert.deepEqual(listOf(pages, '1, 3，5'), [1, 3, 5])
  assert.equal(listOf(pages, '1, x'), null)
  const values = initialValues([pages], { pages: [2] })
  assert.deepEqual(values, { pages: '2' })
  assert.deepEqual(paramsOf([pages], values), { pages: [2] })
  assert.equal(valuesValid([pages], { pages: '1, x' }), false)
  assert.equal(valuesValid([pages], { pages: '' }, ['pages']), false)
  assert.deepEqual(requiredKeys({ id: 'x', params: '{find, grep?: bool, scope?: document|story}' }), ['find'])
  assert.deepEqual(requiredKeys({ id: 'x', params: '{url? | email? | page? (1-based), name?, ids?}' }), [])
  assert.deepEqual(requiredKeys({ id: 'x', params: '{pages?: [index] (default all), margins?: number|{top,bottom,inside,outside}, columns?}' }), [])
  assert.equal(humanize('columnGutter'), 'Column Gutter')
})

test('the catalog: every variant names its command, rows never repeat in a menu, and the reference order holds', () => {
  for (const [id, variant] of VARIANTS) {
    assert.ok(id.startsWith(`${variant.command}${VARIANT}`), id)
    assert.equal(engineCommandOf(id), variant.command)
  }
  assert.equal(engineCommandOf('kit:file.place'), null)
  const commands = [...CATALOG_COMMANDS].map((id) => ({ id, title: id }))
  const rows = layoutMenus(commands, asks)
  const seen = new Set()
  for (const row of rows) {
    if (row.command === '---') continue
    const key = `${row.path[0]} ${row.command}`
    assert.ok(!seen.has(key), `${key} twice`)
    seen.add(key)
  }
  assert.deepEqual([...new Set(rows.map((row) => row.path[0]))], ['File', 'Edit', 'Layout', 'Type', 'Object', 'Table', 'View', 'Help'])
  // A row that asks first ends in "…"; one that runs does not.
  assert.equal(rows.find((row) => row.command === 'object.textFrameOptions').label, 'object.textFrameOptions…')
  assert.equal(rows.find((row) => row.command === 'object.group').label, 'object.group')
  // Insert Table is the Table menu's first row; the Window rows moved to Object.
  assert.equal(rows.find((row) => row.path[0] === 'Table').command, 'table.insert')
  assert.ok(rows.some((row) => row.command === 'object.align|left' && row.path.join('/') === 'Object/Align'))
  assert.ok(rows.some((row) => row.command === 'object.textWrap|contour' && row.path.join('/') === 'Object/Text Wrap'))
  // No hidden command is in the catalog.
  for (const command of CATALOG_COMMANDS) assert.equal(HIDDEN[command], undefined, command)
})

test('Insert Pages and Move Pages put pages where the person asks', () => {
  assert.deepEqual(pagesFromText('3, 5-7', 8), [3, 5, 6, 7])
  assert.equal(pagesFromText('9', 8), null)
  assert.equal(pagesFromText('', 8), null)
  assert.deepEqual(insertPlan({ count: 2, where: 'after', page: 3, pages: 5, parent: 'A-Parent' }), [['layout.pages.insert', { count: 2, after: 2, parent: 'A-Parent' }]])
  assert.deepEqual(insertPlan({ count: 1, where: 'before', page: 3, pages: 5, parent: null }), [['layout.pages.insert', { count: 1, after: 1, parent: null }]])
  assert.deepEqual(insertPlan({ count: 1, where: 'end', page: 1, pages: 5, parent: null }), [['layout.pages.insert', { count: 1, after: 4, parent: null }]])
  assert.deepEqual(insertPlan({ count: 2, where: 'start', page: 1, pages: 5, parent: null }), [['layout.pages.insert', { count: 2, after: 0, parent: null }], ['layout.pages.move', { from: 0, to: 2 }]])
  // Moving pages 1 and 2 to the end of 4 pages: the order becomes 3, 4, 1, 2.
  const steps = movePlan({ pages: [1, 2], where: 'end', page: 1, count: 4 })
  const order = [0, 1, 2, 3]
  for (const { from, to } of steps) order.splice(to, 0, ...order.splice(from, 1))
  assert.deepEqual(order, [2, 3, 0, 1])
  assert.deepEqual(movePlan({ pages: [3], where: 'after', page: 2, count: 4 }), [])
  assert.equal(movePlan({ pages: [2], where: 'before', page: 2, count: 4 }), null)
})

test('lengths in millimetres, inches and points', () => {
  assert.equal(fromPoints(595.276, 'mm'), 210)
  assert.equal(fromPoints(72, 'in'), 1)
  assert.ok(Math.abs(toPoints(297, 'mm') - 841.89) < 0.01)
  assert.equal(scaleLabel(300), '4.17')
  assert.equal(scaleLabel(144), '2')
})

test('greyed-out rows say why, and the engine\'s refusals a menu meets are worded for the person', () => {
  const one = { items: [{ kind: '<rectangle>', image: false, story: false }], editingText: false }
  assert.equal(contextReason('object.group', one), 'needTwo')
  assert.equal(contextReason('object.distribute', { ...one, items: [...one.items, ...one.items] }), 'needThree')
  assert.equal(contextReason('object.clippingPath', one), 'needImage')
  assert.equal(contextReason('object.clippingPath', { items: [{ kind: '<rectangle>', image: true, story: false }], editingText: false }), null)
  assert.equal(contextReason('object.lock', one), null)
  assert.equal(disabledReason('table.merge'), 'needTable')
  assert.equal(disabledReason('table.insert'), 'needCaret')
  assert.equal(disabledReason('edit.undo'), 'nothingToUndo')
  assert.equal(refusalReason(new Error("invalid parameters for `transform.again`: no transform to repeat")), 'needTransform')
  assert.equal(refusalReason(new Error('something else')), null)
  assert.deepEqual(placeParams('guide.deleteAll', { page: 3, spread: 2 }), { spread: 2 })
  assert.deepEqual(placeParams('layout.removeOverrides', { page: 3, spread: 2 }), { page: 3 })
  assert.deepEqual(placeValues('layout.section', { page: 3, spread: 2 }), { page: 4 })
  // Every reason reads in both languages.
  for (const reason of ['needTwo', 'needThree', 'needTwoShapes', 'needImage', 'needTypeOnPath', 'needSelection', 'needCaret', 'needTextSelection', 'needTable', 'needClipboard', 'nothingToUndo', 'nothingToRedo', 'needToc', 'needIndex', 'needFootnote', 'needTransform', 'needTextFrame', 'needEmptyFrame', 'needStoryText', 'nothingToTrace', 'textFrameShape', 'noOverlap', 'notNow']) {
    assert.ok(CATALOGS['en-US'][`hint.${reason}`], reason)
    assert.ok(CATALOGS['zh-CN'][`hint.${reason}`], reason)
  }
})

test('a label ending in "…" reads as its phrase with the ellipsis', () => {
  const table = PHRASES['zh-CN']
  const phrase = withEllipsis((english) => table[english] ?? english)
  assert.equal(phrase('Text Frame Options…'), '文本框架选项…')
  assert.equal(phrase('Text Frame Options'), '文本框架选项')
  assert.equal(phrase('Nothing like it…'), 'Nothing like it…')
  // The editor's own strings exist in both languages.
  const own = (catalog) => Object.keys(catalog).filter((key) => /^(param|option|dialog|hint|find|side|page|where)\./.test(key)).sort()
  assert.deepEqual(own(CATALOGS['zh-CN']), own(CATALOGS['en-US']))
})
