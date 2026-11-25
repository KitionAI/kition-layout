/**
 * Lengths in the units a layout is measured in (plan 2026-11-25, stage 1: the New Document,
 * Document Setup and Margins and Columns dialogs). The engine takes points; a pixel is a point, as
 * in a web or mobile document. Pure, so it is tested in Node.
 */

/** Points per unit. */
export const UNITS = { mm: 72 / 25.4, cm: 72 / 2.54, in: 72, pt: 1, px: 1 } as const

export type Unit = keyof typeof UNITS

export const UNIT_NAMES = Object.keys(UNITS) as Unit[]

/** Decimals a length keeps in `unit`. */
export function unitDecimals(unit: Unit) {
  return unit === 'in' || unit === 'cm' ? 3 : unit === 'mm' ? 2 : 2
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
