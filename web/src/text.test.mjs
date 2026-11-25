// Text by hand (plan 2026-11-25, stage 5), without a browser: node --test src/text.test.mjs
// Presses in a row, and where the input method's candidate window goes.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CLICK_INTERVAL_MS, nextPress } from './clicks.ts'
import { caretOnScreen } from './ime.ts'
import { LIST_ROWS, nextListType } from './lists.ts'
import { resizeTableSteps } from './panels/tableLogic.ts'

test('presses close in time and place count up, a pause or a move starts again', () => {
  let press = nextPress(null, 1000, 10, 10)
  assert.equal(press.count, 1)
  for (const want of [2, 3, 4, 5, 6]) {
    press = nextPress(press, press.time + 120, 11, 9)
    assert.equal(press.count, want)
  }
  assert.equal(nextPress(press, press.time + CLICK_INTERVAL_MS + 1, 11, 9).count, 1, 'a pause')
  assert.equal(nextPress(press, press.time + 100, 40, 9).count, 1, 'moved away')
  assert.equal(nextPress(press, press.time - 5, 11, 9).count, 1, 'a clock going back')
  let many = nextPress(null, 0, 0, 0)
  for (let i = 1; i < 300; i += 1) many = nextPress(many, i, 0, 0)
  assert.equal(many.count, 255, 'the count stops at what the engine takes')
})

test('the candidate window goes to the caret on the screen', () => {
  const spot = caretOnScreen([110, 200, 110, 214], { x: 100, y: 150, zoom: 2 }, { left: 40, top: 60 })
  assert.deepEqual(spot, { left: 60, top: 160, height: 28 })
  assert.equal(caretOnScreen([Number.NaN, 0, 0, 1], { x: 0, y: 0, zoom: 1 }, { left: 0, top: 0 }), null)
  assert.equal(caretOnScreen([0, 0, 0, 1], { x: 0, y: 0, zoom: 0 }, { left: 0, top: 0 }), null)
  assert.equal(caretOnScreen([5, 9, 5, 9], { x: 0, y: 0, zoom: 1 }, { left: 0, top: 0 }).height, 1, 'never a zero-high field')
})

test('bullets and numbering switch on, over each other, and off', () => {
  assert.equal(nextListType('none', 'bullets'), 'bullets')
  assert.equal(nextListType(undefined, 'numbers'), 'numbers')
  assert.equal(nextListType('bullets', 'numbers'), 'numbers')
  assert.equal(nextListType('numbers', 'numbers'), 'none')
  assert.deepEqual(LIST_ROWS.map(([kind]) => kind), ['bullets', 'numbers'])
})

test('the Table panel adds rows and columns after the last and deletes the last ones', () => {
  const at = { story: 4, table: 9 }
  assert.deepEqual(resizeTableSteps(at, 'rows', 3, 5), [['table.insertRow', { story: 4, table: 9, rows: [2, 2], cols: [0, 0], where: 'below', count: 2 }]])
  assert.deepEqual(resizeTableSteps(at, 'columns', 3, 4), [['table.insertColumn', { story: 4, table: 9, rows: [0, 0], cols: [2, 2], where: 'right', count: 1 }]])
  assert.deepEqual(resizeTableSteps(at, 'rows', 4, 2), [['table.deleteRow', { story: 4, table: 9, rows: [2, 3], cols: [0, 0] }]])
  assert.deepEqual(resizeTableSteps(at, 'columns', 3, 0), [['table.deleteColumn', { story: 4, table: 9, rows: [0, 0], cols: [1, 2] }]], 'a table keeps one column')
  assert.deepEqual(resizeTableSteps(at, 'rows', 3, 3), [])
  assert.deepEqual(resizeTableSteps(at, 'rows', 3, Number.NaN), [])
})
