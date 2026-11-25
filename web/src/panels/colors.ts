/**
 * Colours as the Properties panel's colour picker reads and writes them (plan 2026-11-25, stage 4):
 * RGB bytes for the picker, CMYK percentages as a printer reads them, and the colour value the
 * engine's commands take (`#rrggbb`, or `{c, m, y, k}` in percent, kept as CMYK). The screen shows
 * CMYK by the simple conversion (no colour profile), as the picker's preview. Pure, so it is
 * tested in Node.
 */

export type Rgb = readonly [number, number, number]
export type Cmyk = readonly [number, number, number, number]
/** A colour the person chose, in the model they chose it in. */
export type PickedColor = { model: 'rgb'; rgb: Rgb } | { model: 'cmyk'; cmyk: Cmyk }

const byte = (value: number) => Math.max(0, Math.min(255, Math.round(value)))
const percent = (value: number) => Math.max(0, Math.min(100, Math.round(value * 10) / 10))

/** `#rrggbb` (or `rrggbb`, `#rgb`) as RGB bytes, or null. */
export function hexToRgb(text: string): Rgb | null {
  const hex = text.trim().replace(/^#/, '')
  const full = /^[0-9a-f]{3}$/i.test(hex) ? hex.replace(/./g, (digit) => digit + digit) : hex
  if (!/^[0-9a-f]{6}$/i.test(full)) return null
  return [Number.parseInt(full.slice(0, 2), 16), Number.parseInt(full.slice(2, 4), 16), Number.parseInt(full.slice(4, 6), 16)]
}

export function rgbToHex([r, g, b]: Rgb) {
  return `#${[r, g, b].map((value) => byte(value).toString(16).padStart(2, '0')).join('')}`
}

/** CMYK percentages on screen (no profile). */
export function cmykToRgb([c, m, y, k]: Cmyk): Rgb {
  const ink = (part: number) => 255 * (1 - Math.max(0, Math.min(100, part)) / 100) * (1 - Math.max(0, Math.min(100, k)) / 100)
  return [byte(ink(c)), byte(ink(m)), byte(ink(y))]
}

/** RGB bytes as CMYK percentages (no profile; black from the darkest channel). */
export function rgbToCmyk([r, g, b]: Rgb): Cmyk {
  const [rr, gg, bb] = [r / 255, g / 255, b / 255]
  const k = 1 - Math.max(rr, gg, bb)
  if (k >= 1) return [0, 0, 0, 100]
  const part = (channel: number) => percent(((1 - channel - k) / (1 - k)) * 100)
  return [part(rr), part(gg), part(bb), percent(k * 100)]
}

/** The colour as RGB bytes, for the picker and the swatch chip. */
export function pickedRgb(color: PickedColor): Rgb {
  return color.model === 'rgb' ? color.rgb : cmykToRgb(color.cmyk)
}

/** The colour as the engine's commands take it (`object.color`, `swatch.add`). */
export function colorParam(color: PickedColor): string | { c: number; m: number; y: number; k: number } {
  if (color.model === 'rgb') return rgbToHex(color.rgb)
  const [c, m, y, k] = color.cmyk.map(percent)
  return { c, m, y, k }
}

/** A swatch's colour as the engine reports it (`inspect().colors`): CMYK when defined so, else RGB. */
export function swatchColor(entry: { hex?: string; cmyk?: readonly number[] } | undefined): PickedColor | null {
  if (!entry) return null
  if (entry.cmyk && entry.cmyk.length === 4) return { model: 'cmyk', cmyk: [entry.cmyk[0], entry.cmyk[1], entry.cmyk[2], entry.cmyk[3]] }
  const rgb = entry.hex ? hexToRgb(entry.hex) : null
  return rgb ? { model: 'rgb', rgb } : null
}

/** Whether two colours are the same to the engine (what it would store). */
export function sameColor(a: PickedColor, b: PickedColor) {
  return JSON.stringify(colorParam(a)) === JSON.stringify(colorParam(b))
}
