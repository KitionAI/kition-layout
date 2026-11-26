/**
 * Object Style Options (plan 2026-11-25, phase 11): the dialog's values from what an object style
 * includes, and the `style.object.edit` their changes make (only what changed; a part switched off
 * is left out of the style, `null`). Pure, so it is tested in Node.
 */
import type { ObjectStyleDef, ObjectStyleParts } from './styleRules.ts'

/** The vertical justifications a text frame takes. */
export const VERTICAL_JUSTIFICATIONS = ['top', 'center', 'bottom', 'justify'] as const

/** The dialog's values: each part's switch ("include in the style") and its fields, as typed. */
export type ObjectStyleDraft = {
  name: string
  basedOn: string
  fill: { on: boolean; swatch: string; tint: string }
  stroke: { on: boolean; swatch: string; weight: string; tint: string }
  paragraph: { on: boolean; style: string }
  frame: { on: boolean; columns: string; gutter: string; inset: [string, string, string, string]; vj: string }
}

/** Built-in object styles: [None] cannot be changed; none of them renamed or deleted. */
export const BUILT_IN_OBJECT_STYLES: ReadonlySet<string> = new Set(['[None]', '[Basic Graphics Frame]', '[Basic Text Frame]', '[Basic Grid]'])

const text = (value: number | null | undefined, scale = 1) => (value == null ? '' : String(Math.round(value * scale * 1000) / 1000))

/** The values the dialog opens with: what the style includes itself (a part it leaves out shows its Based On value, off). */
export function objectStyleDraft(def: ObjectStyleDef | undefined, name: string): ObjectStyleDraft {
  const own: Partial<ObjectStyleParts> = def?.own ?? {}
  const shown: Partial<ObjectStyleParts> = def?.resolved ?? own
  const frame = own.textFrame ?? shown.textFrame
  const inset = frame?.inset ?? [0, 0, 0, 0]
  return {
    name,
    basedOn: def?.basedOn ?? '',
    fill: { on: own.fill != null, swatch: own.fill ?? shown.fill ?? '[None]', tint: text(own.fillTint ?? shown.fillTint ?? 1, 100) },
    stroke: { on: own.stroke != null, swatch: own.stroke ?? shown.stroke ?? '[Black]', weight: text(own.strokeWeight ?? shown.strokeWeight ?? 1), tint: text(own.strokeTint ?? shown.strokeTint ?? 1, 100) },
    paragraph: { on: own.paragraphStyle != null, style: own.paragraphStyle ?? shown.paragraphStyle ?? '[Basic Paragraph]' },
    frame: {
      on: own.textFrame != null,
      columns: text(frame?.columns ?? 1),
      gutter: text(frame?.gutter ?? 12),
      inset: [text(inset[0] ?? 0), text(inset[1] ?? 0), text(inset[2] ?? 0), text(inset[3] ?? 0)],
      vj: frame?.verticalJustification ?? 'top',
    },
  }
}

/** A typed number (null: not a number, or outside `min`..`max`). */
function number(value: string, min: number, max: number): number | null {
  const n = Number(value.trim())
  return value.trim() !== '' && Number.isFinite(n) && n >= min && n <= max ? n : null
}

/**
 * The `style.object.edit` parameters for what changed (null: nothing did, or a value is not
 * valid). A rename is left to OK (`withName`): the preview would rename the style it shows.
 */
export function objectStyleEdit(name: string, from: ObjectStyleDraft, to: ObjectStyleDraft, withName = false): Record<string, unknown> | null {
  const params: Record<string, unknown> = { name }
  if (withName && to.name.trim() && to.name.trim() !== from.name) params.rename = to.name.trim()
  if (to.basedOn !== from.basedOn) params.basedOn = to.basedOn || null
  // Fill.
  if (!to.fill.on && from.fill.on) params.fill = null
  if (to.fill.on) {
    const tint = number(to.fill.tint, 0, 100)
    if (tint == null) return null
    if (!from.fill.on || to.fill.swatch !== from.fill.swatch) params.fill = to.fill.swatch
    if (!from.fill.on || to.fill.tint !== from.fill.tint) params.fillTint = tint / 100
  }
  // Stroke.
  if (!to.stroke.on && from.stroke.on) params.stroke = null
  if (to.stroke.on) {
    const tint = number(to.stroke.tint, 0, 100)
    const weight = number(to.stroke.weight, 0, 1000)
    if (tint == null || weight == null) return null
    if (!from.stroke.on || to.stroke.swatch !== from.stroke.swatch) params.stroke = to.stroke.swatch
    if (!from.stroke.on || to.stroke.weight !== from.stroke.weight) params.strokeWeight = weight
    if (!from.stroke.on || to.stroke.tint !== from.stroke.tint) params.strokeTint = tint / 100
  }
  // Paragraph style.
  if (!to.paragraph.on && from.paragraph.on) params.paragraphStyle = null
  if (to.paragraph.on && (!from.paragraph.on || to.paragraph.style !== from.paragraph.style)) params.paragraphStyle = to.paragraph.style
  // Text frame options.
  if (!to.frame.on && from.frame.on) params.textFrame = null
  if (to.frame.on) {
    const columns = number(to.frame.columns, 1, 40)
    const gutter = number(to.frame.gutter, 0, 10000)
    const inset = to.frame.inset.map((side) => number(side, 0, 10000))
    if (columns == null || !Number.isInteger(columns) || gutter == null || inset.some((side) => side == null)) return null
    if (!from.frame.on || JSON.stringify(to.frame) !== JSON.stringify(from.frame)) params.textFrame = { columns, gutter, inset, verticalJustification: to.frame.vj }
  }
  return Object.keys(params).length > 1 ? params : null
}

/** The styles a style may be based on: every other one but [None] that is not based on it. */
export function objectBasedOnChoices(defs: readonly ObjectStyleDef[], name: string): string[] {
  const byName = new Map(defs.map((def) => [def.name, def]))
  const loops = (start: string) => {
    let at: string | null | undefined = start
    for (let steps = 0; at && steps <= defs.length; steps += 1) {
      if (at === name) return true
      at = byName.get(at)?.basedOn
    }
    return false
  }
  return defs.flatMap((def) => (def.name !== '[None]' && !loops(def.name) ? [def.name] : []))
}

/** What may take a deleted style's place: [None] first, then every other style. */
export function objectStyleReplacements(defs: readonly ObjectStyleDef[], deleting: string): string[] {
  const others = defs.map((def) => def.name).filter((name) => name !== deleting)
  return others.includes('[None]') ? ['[None]', ...others.filter((name) => name !== '[None]')] : others
}

/** A free name for a new style ("Object Style 1", "Object Style 2", …). */
export function freeObjectStyleName(defs: readonly ObjectStyleDef[], base: string): string {
  const names = new Set(defs.map((def) => def.name))
  for (let n = 1; ; n += 1) if (!names.has(`${base} ${n}`)) return `${base} ${n}`
}
