// Lengths in the ruler unit (plan 2026-11-25, stage 3), without a browser:
// node --test src/units.test.mjs (Node strips the modules' types).
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ENGINE_UNITS, UNITS, UNIT_NAMES, unitDecimals, formatField, formatLength, parseLength, rulerLabel, rulerTicks } from './units.ts'

const near = (a, b) => assert.ok(a !== null && Math.abs(a - b) < 1e-6, `${a} vs ${b}`)

test('a typed length is read in the field unit or its own', () => {
  near(parseLength('10', 'mm'), 10 * 72 / 25.4)
  near(parseLength('10mm', 'pt'), 10 * 72 / 25.4)
  near(parseLength('10 mm', 'in'), 10 * 72 / 25.4)
  near(parseLength('0.5in', 'mm'), 36)
  near(parseLength('3p6', 'mm'), 42)
  near(parseLength('p6', 'pt'), 6)
  near(parseLength('3p', 'pt'), 36)
  near(parseLength('12pt', 'mm'), 12)
  near(parseLength('2cm', 'pt'), 2 * 72 / 2.54)
  near(parseLength('10px', 'mm'), 10)
  near(parseLength('1,5', 'in'), 108)
  near(parseLength('1in+6pt', 'mm'), 78)
  near(parseLength('1in - 6pt', 'mm'), 66)
  near(parseLength('-5', 'pt'), -5)
  near(parseLength('3', 'pc'), 36)
  near(parseLength('210毫米', 'pt'), 210 * 72 / 25.4)
  for (const wrong of ['', 'abc', '10 parsecs', '1in++2', '5 6', 'mm']) assert.equal(parseLength(wrong, 'pt'), null, wrong)
})

test('a length reads back as it was typed', () => {
  assert.equal(formatLength(72, 'in'), '1 in')
  assert.equal(formatLength(595.276, 'mm'), '210 mm')
  assert.equal(formatLength(42, 'pc'), '3p6')
  assert.equal(formatLength(36, 'pc'), '3p0')
  assert.equal(formatField(841.89, 'mm'), '297')
  assert.equal(formatField(12, 'pt'), '12')
  assert.equal(formatField(42, 'pc'), '3p6')
  // What a field shows reads back within its last decimal.
  for (const unit of UNIT_NAMES) {
    for (const points of [0, 1, 12.5, 595.276, 1000]) {
      const back = parseLength(formatField(points, unit), unit)
      assert.ok(back !== null && Math.abs(back - points) <= 0.5 * 10 ** -unitDecimals(unit) * UNITS[unit] + 0.006, `${unit}: ${points} → ${back}`)
    }
    assert.ok(ENGINE_UNITS[unit], unit)
  }
})

test('the rulers tick on round numbers of their unit, never closer than 4 screen pixels', () => {
  for (const unit of UNIT_NAMES) {
    for (const zoom of [0.1, 0.38, 1, 2.5, 8, 30]) {
      const { major, parts } = rulerTicks(unit, zoom)
      assert.ok(major * zoom >= 56 || major >= 1000, `${unit} at ${zoom}: ${major}`)
      assert.ok((major / parts) * zoom >= 4 || parts === 1, `${unit} at ${zoom}: ${parts} parts of ${major}`)
      // A major tick is a whole or simple number of the unit.
      const inUnit = Number(rulerLabel(major, unit))
      assert.ok(Number.isFinite(inUnit) && Math.abs(inUnit * 8 - Math.round(inUnit * 8)) < 1e-6, `${unit}: ${inUnit}`)
    }
  }
  // Millimetres at 100%: 20 mm apart, labelled 20, 40…
  const mm = rulerTicks('mm', 1)
  assert.equal(rulerLabel(mm.major * 2, 'mm'), '40')
})
