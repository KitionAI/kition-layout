/**
 * Gradient swatches (plan 2026-11-25, phase 11): a gradient swatch as the dialog edits it (kind,
 * stops with a colour, a location and a midpoint to the next stop, in percent), the `swatch.add` /
 * `swatch.set` parameters it makes, and the stop bar's moves (add a stop where the bar is clicked,
 * with the colour the gradient has there; drag one along; drag one off to remove it). Pure, so it
 * is tested in Node.
 */
import { cmykToRgb, colorParam, hexToRgb, pickedRgb, rgbToHex, type PickedColor } from './colors.ts'

export type GradientKind = 'linear' | 'radial'
export type GradientStop = { location: number; color: PickedColor; midpoint: number }
export type GradientDraft = { name: string; kind: GradientKind; stops: GradientStop[] }
/** A gradient swatch as `inspect().colors` reads it. */
export type GradientEntry = { kind?: string; stops?: Array<{ hex?: string; cmyk?: number[]; location?: number; midpoint?: number }> }

/** The colour a stop reads back as: CMYK when defined so, else RGB. */
function stopColor(stop: { hex?: string; cmyk?: number[] }): PickedColor {
  if (stop.cmyk && stop.cmyk.length === 4) return { model: 'cmyk', cmyk: [stop.cmyk[0], stop.cmyk[1], stop.cmyk[2], stop.cmyk[3]] }
  return { model: 'rgb', rgb: hexToRgb(stop.hex ?? '') ?? [0, 0, 0] }
}

/** A new gradient swatch: white to black, linear (InDesign's). */
export function newGradientDraft(name = ''): GradientDraft {
  return {
    name,
    kind: 'linear',
    stops: [
      { location: 0, color: { model: 'rgb', rgb: [255, 255, 255] }, midpoint: 50 },
      { location: 100, color: { model: 'rgb', rgb: [0, 0, 0] }, midpoint: 50 },
    ],
  }
}

/** The values the dialog opens with for an existing gradient swatch. */
export function gradientDraft(name: string, entry: GradientEntry | undefined): GradientDraft {
  const stops = (entry?.stops ?? []).map((stop) => ({ location: stop.location ?? 0, color: stopColor(stop), midpoint: stop.midpoint ?? 50 }))
  return { name, kind: entry?.kind === 'radial' ? 'radial' : 'linear', stops: stops.length >= 2 ? stops : newGradientDraft(name).stops }
}

const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value))
const round = (value: number) => Math.round(value * 10) / 10

/** The `gradient` parameter of `swatch.add` / `swatch.set`. */
export function gradientParam(draft: GradientDraft) {
  return {
    kind: draft.kind,
    stops: [...draft.stops].sort((a, b) => a.location - b.location).map((stop) => ({ location: round(clamp(stop.location, 0, 100)), color: colorParam(stop.color), midpoint: round(clamp(stop.midpoint, 13, 87)) })),
  }
}

/** Swatch Options for a gradient swatch: what changed (null: nothing). A rename is left to OK (`withName`). */
export function gradientEdit(name: string, from: GradientDraft, to: GradientDraft, withName = false): Record<string, unknown> | null {
  const params: Record<string, unknown> = { name }
  if (withName && to.name.trim() && to.name.trim() !== from.name) params.rename = to.name.trim()
  if (JSON.stringify(gradientParam(to)) !== JSON.stringify(gradientParam(from))) params.gradient = gradientParam(to)
  return Object.keys(params).length > 1 ? params : null
}

/** The gradient's colour at `location` (percent), between the stops around it (midpoints as the engine draws them). */
export function colorAt(stops: readonly GradientStop[], location: number): PickedColor {
  const sorted = [...stops].sort((a, b) => a.location - b.location)
  const first = sorted[0]
  const last = sorted[sorted.length - 1]
  if (!first || !last) return { model: 'rgb', rgb: [0, 0, 0] }
  if (location <= first.location) return first.color
  if (location >= last.location) return last.color
  const at = sorted.findIndex((stop) => stop.location >= location)
  const a = sorted[at - 1] ?? first
  const b = sorted[at] ?? last
  const span = Math.max(1e-6, b.location - a.location)
  let t = (location - a.location) / span
  // The midpoint is where the two colours mix half and half.
  const mid = clamp(a.midpoint / 100, 0.13, 0.87)
  t = t <= mid ? (t / mid) * 0.5 : 0.5 + ((t - mid) / (1 - mid)) * 0.5
  if (a.color.model === 'cmyk' && b.color.model === 'cmyk') {
    const [p, q] = [a.color.cmyk, b.color.cmyk]
    return { model: 'cmyk', cmyk: [0, 1, 2, 3].map((i) => round(p[i] + (q[i] - p[i]) * t)) as unknown as [number, number, number, number] }
  }
  const [p, q] = [pickedRgb(a.color), pickedRgb(b.color)]
  return { model: 'rgb', rgb: [0, 1, 2].map((i) => Math.round(p[i] + (q[i] - p[i]) * t)) as unknown as [number, number, number] }
}

/** A stop added where the bar was clicked, with the gradient's colour there: the stops and the new one's index. */
export function addStop(stops: readonly GradientStop[], location: number): { stops: GradientStop[]; index: number } {
  const at = round(clamp(location, 0, 100))
  const next = [...stops, { location: at, color: colorAt(stops, at), midpoint: 50 }].sort((a, b) => a.location - b.location)
  return { stops: next, index: next.findIndex((stop) => stop.location === at) }
}

/** Stop `index` dragged to `location`: the stops in order and where that stop now is. */
export function moveStop(stops: readonly GradientStop[], index: number, location: number): { stops: GradientStop[]; index: number } {
  const moving = stops[index]
  if (!moving) return { stops: [...stops], index }
  const moved = { ...moving, location: round(clamp(location, 0, 100)) }
  const next = stops.map((stop, i) => (i === index ? moved : stop)).sort((a, b) => a.location - b.location)
  return { stops: next, index: next.indexOf(moved) }
}

/** Stop `index` dragged off the bar: gone, as long as two stay. */
export function removeStop(stops: readonly GradientStop[], index: number): GradientStop[] {
  return stops.length > 2 ? stops.filter((_, i) => i !== index) : [...stops]
}

/** The swatch chip's CSS background for a gradient. */
export function cssGradient(entry: GradientEntry | undefined, { asOnPage = false }: { asOnPage?: boolean } = {}): string | undefined {
  const stops = entry?.stops ?? []
  if (stops.length < 2) return undefined
  // `asOnPage`: each stop as the engine draws it on the page (its `hex`, the document's colour
  // conversion), so a CMYK cyan chip matches the page (phase 12: the chip showed #00ffff).
  const color = (stop: NonNullable<GradientEntry['stops']>[number]) =>
    asOnPage && stop.hex ? stop.hex : stop.cmyk && stop.cmyk.length === 4 ? rgbToHex(cmykToRgb([stop.cmyk[0], stop.cmyk[1], stop.cmyk[2], stop.cmyk[3]])) : (stop.hex ?? '#000000')
  const parts = stops.map((stop) => `${color(stop)} ${stop.location ?? 0}%`)
  return entry?.kind === 'radial' ? `radial-gradient(circle, ${parts.join(', ')})` : `linear-gradient(to right, ${parts.join(', ')})`
}

/** The draft's own CSS (the stop bar). */
export function draftCss(draft: GradientDraft): string {
  const parts = [...draft.stops].sort((a, b) => a.location - b.location).map((stop) => `${rgbToHex(pickedRgb(stop.color))} ${stop.location}%`)
  return `linear-gradient(to right, ${parts.join(', ')})`
}
