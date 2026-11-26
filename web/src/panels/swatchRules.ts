/**
 * Swatch Options and Delete Swatch (plan 2026-11-25, stage 7): the `swatch.set` a dialog's changes
 * make, and the swatches that may take a deleted one's place. Pure, so it is tested in Node.
 */
import { colorParam, type PickedColor } from './colors.ts'

/** Swatch Options' values: its name, process or spot, colour (none: Paper's only, or a tint), a tint swatch's tint in percent. */
export type SwatchDraft = { name: string; spot: boolean; color: PickedColor | null; tint: number | null }

/** The `swatch.set` parameters for what changed (null: nothing did, or the tint is not 0 to 100). */
export function swatchEdit(name: string, from: SwatchDraft, to: SwatchDraft): Record<string, unknown> | null {
  const params: Record<string, unknown> = { name }
  const rename = to.name.trim()
  if (rename && rename !== from.name) params.rename = rename
  if (to.spot !== from.spot) params.spot = to.spot
  if (to.color && (!from.color || JSON.stringify(colorParam(to.color)) !== JSON.stringify(colorParam(from.color)))) params.color = colorParam(to.color)
  if (to.tint !== from.tint) {
    if (to.tint == null || !Number.isFinite(to.tint) || to.tint < 0 || to.tint > 100) return null
    params.tint = to.tint / 100
  }
  return Object.keys(params).length > 1 ? params : null
}

/**
 * What may take a deleted swatch's place: the other swatches that are colours, [Black] first; for
 * a gradient swatch the other gradients too (phase 11).
 */
export function swatchReplacements(swatches: ReadonlyArray<{ name: string; kind: string }>, deleting: string): string[] {
  const gradient = swatches.some((swatch) => swatch.name === deleting && swatch.kind === 'gradient')
  const others = swatches.filter((swatch) => swatch.name !== deleting && swatch.kind !== 'registration' && (gradient || swatch.kind !== 'gradient')).map((swatch) => swatch.name)
  return others.includes('[Black]') ? ['[Black]', ...others.filter((name) => name !== '[Black]')] : others
}
