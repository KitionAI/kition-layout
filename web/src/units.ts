/**
 * Lengths in the units a layout is measured in (plan 2026-11-25, stages 1 and 3): the ruler unit
 * the person chooses (points, picas, inches, millimetres, centimetres, pixels) for the rulers,
 * the dialogs' and panels' fields and the tools' readouts; the engine always takes points. A
 * pixel is a point, as in a web or mobile document. Fields take a length with its unit (`10mm`,
 * `3p6`, `0.5in`, `12pt`) or a bare number in the field's unit, and sums (`1in+6pt`). Pure, so it
 * is tested in Node.
 */

/** Points per unit. A pica is 12 points; `3p6` is 3 picas and 6 points. */
export const UNITS = { mm: 72 / 25.4, cm: 72 / 2.54, in: 72, pt: 1, pc: 12, px: 1 } as const

export type Unit = keyof typeof UNITS

export const UNIT_NAMES = Object.keys(UNITS) as Unit[]

/** The engine's name of each unit (its tools' readouts, `setView`). */
export const ENGINE_UNITS: Readonly<Record<Unit, string>> = { mm: 'millimeters', cm: 'centimeters', in: 'inches', pt: 'points', pc: 'picas', px: 'pixels' }

/** Whether `value` names a unit. */
export function isUnit(value: unknown): value is Unit {
  return typeof value === 'string' && Object.prototype.hasOwnProperty.call(UNITS, value)
}

/** Decimals a length keeps in `unit`. */
export function unitDecimals(unit: Unit) {
  return unit === 'in' || unit === 'cm' ? 3 : unit === 'pc' ? 3 : 2
}

/** `points` in `unit`, rounded to the unit's decimals. */
export function fromPoints(points: number, unit: Unit) {
  const scale = 10 ** unitDecimals(unit)
  return Math.round((points / UNITS[unit]) * scale) / scale
}

/** A length in `unit` as points. */
export function toPoints(value: number, unit: Unit) {
  return value * UNITS[unit]
}

/** The suffix a length shows in `unit` (`mm`, `pt`); picas show as `3p6` instead. */
export function unitSuffix(unit: Unit) {
  return unit === 'pc' ? 'p' : unit
}

const trim = (value: number, decimals: number) => {
  const text = value.toFixed(decimals)
  const short = text.includes('.') ? text.replace(/0+$/, '').replace(/\.$/, '') : text
  return short === '-0' ? '0' : short
}

/** `points` as the person reads it in `unit`: `25.4 mm`, `72 pt`, `3p6`. */
export function formatLength(points: number, unit: Unit, decimals = unitDecimals(unit)) {
  if (!Number.isFinite(points)) return ''
  if (unit === 'pc') {
    const sign = points < 0 ? '-' : ''
    const whole = Math.floor(Math.abs(points) / 12 + 1e-9)
    const rest = Math.abs(points) - whole * 12
    return `${sign}${whole}p${trim(rest, 2)}`
  }
  return `${trim(points / UNITS[unit], decimals)} ${unit}`
}

/** `points` as a field shows it in `unit`, without the unit when it is the field's own. */
export function formatField(points: number, unit: Unit) {
  if (unit === 'pc') return formatLength(points, unit)
  return trim(points / UNITS[unit], unitDecimals(unit))
}

/** Unit names a person types, longest first so `mm` is not read as `m…`. */
const TYPED: ReadonlyArray<readonly [string, Unit]> = [
  ['millimeters', 'mm'],
  ['millimetres', 'mm'],
  ['centimeters', 'cm'],
  ['centimetres', 'cm'],
  ['inches', 'in'],
  ['points', 'pt'],
  ['pixels', 'px'],
  ['picas', 'pc'],
  ['inch', 'in'],
  ['mm', 'mm'],
  ['cm', 'cm'],
  ['in', 'in'],
  ['pt', 'pt'],
  ['px', 'px'],
  ['pc', 'pc'],
  ['"', 'in'],
  ['毫米', 'mm'],
  ['厘米', 'cm'],
  ['英寸', 'in'],
  ['点', 'pt'],
  ['像素', 'px'],
  ['派卡', 'pc'],
]

/** One term of a typed length (no sign) in points, or null. */
function term(text: string, unit: Unit): number | null {
  const t = text.trim().toLowerCase().replace(',', '.')
  if (!t) return null
  // Picas and points: `3p6`, `3p`, `p6`.
  const picas = /^(\d*\.?\d*)p(\d*\.?\d*)$/.exec(t)
  if (picas && (picas[1] || picas[2])) {
    const whole = picas[1] ? Number(picas[1]) : 0
    const points = picas[2] ? Number(picas[2]) : 0
    return Number.isFinite(whole) && Number.isFinite(points) ? whole * 12 + points : null
  }
  const number = /^(\d*\.?\d+|\d+\.)\s*(.*)$/.exec(t)
  if (!number) return null
  const value = Number(number[1])
  const name = number[2].trim()
  if (!Number.isFinite(value)) return null
  if (!name) return value * UNITS[unit]
  const found = TYPED.find(([typed]) => typed === name)
  return found ? value * UNITS[found[1]] : null
}

/**
 * A typed length in points: a number in `unit`, a number with its own unit (`10mm`, `0.5 in`,
 * `3p6`, `12pt`), or a sum or difference of them (`1in+6pt`). Null when it is not one.
 */
export function parseLength(text: string, unit: Unit): number | null {
  const source = text.trim()
  if (!source) return null
  const pattern = /\s*([+-]?)\s*([^+-]+)/y
  let total = 0
  let at = 0
  while (at < source.length) {
    pattern.lastIndex = at
    const found = pattern.exec(source)
    // Every term after the first has its sign.
    if (!found || (at > 0 && !found[1])) return null
    const value = term(found[2], unit)
    if (value === null) return null
    total += found[1] === '-' ? -value : value
    at = pattern.lastIndex
  }
  return Number.isFinite(total) ? total : null
}

/**
 * The rulers' ticks in `unit` at `zoom` screen pixels per point: the major step (in points, at
 * least `minGap` screen pixels apart, on a round number of the unit) and how many parts it is cut
 * into.
 */
export function rulerTicks(unit: Unit, zoom: number, minGap = 56): { major: number; parts: number } {
  const per = UNITS[unit]
  const steps: ReadonlyArray<readonly [number, number]> =
    unit === 'in'
      ? [[0.125, 1], [0.25, 2], [0.5, 2], [1, 8], [2, 2], [5, 5], [10, 10], [20, 2], [50, 5], [100, 10]]
      : unit === 'pc'
        ? [[1, 6], [2, 2], [3, 3], [6, 6], [12, 12], [24, 2], [60, 5], [120, 10], [240, 2], [600, 5]]
        : [[1, 1], [2, 2], [5, 5], [10, 10], [20, 2], [25, 5], [50, 5], [100, 10], [200, 2], [250, 5], [500, 5], [1000, 10], [2000, 2], [5000, 5], [10000, 10]]
  const found = steps.find(([step]) => step * per * zoom >= minGap) ?? steps[steps.length - 1]
  let parts = found[1]
  // Never closer than 4 screen pixels between ticks.
  while (parts > 1 && (found[0] * per * zoom) / parts < 4) parts = parts % 2 === 0 ? parts / 2 : 1
  return { major: found[0] * per, parts }
}

/** A ruler label for `points` in `unit`: whole units (picas as whole picas). */
export function rulerLabel(points: number, unit: Unit) {
  const value = points / UNITS[unit]
  const rounded = Math.round(value * 1000) / 1000
  return String(Object.is(rounded, -0) ? 0 : rounded)
}
