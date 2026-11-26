/**
 * The Effects dialog (plan 2026-11-25, stage 7): Transparency (opacity, blend mode) and each
 * effect the engine draws, with its settings as `inspect()` reads them back (`effects`, `blend`,
 * `opacity`); the commands its changes make, run as one undo step (`runSteps` / `previewSteps`).
 * Each effect command takes the names its read-back has, so a page's fields are the effect's own.
 * Pure, so it is tested in Node.
 */

/** InDesign's blend modes, as the engine names them. */
export const BLEND_MODES = ['Normal', 'Multiply', 'Screen', 'Overlay', 'SoftLight', 'HardLight', 'ColorDodge', 'ColorBurn', 'Darken', 'Lighten', 'Difference', 'Exclusion', 'Hue', 'Saturation', 'Color', 'Luminosity'] as const

export type EffectField = {
  key: string
  kind: 'number' | 'check' | 'swatch'
  unit?: string
  /** Shown multiplied by it (an opacity from 0 to 1 as a percentage). */
  scale?: number
  min?: number
  max?: number
}

export type EffectPage = {
  /** Its key in `inspect().effects` (and the dialog's page). */
  id: string
  command: string
  fields: readonly EffectField[]
}

const opacity: EffectField = { key: 'opacity', kind: 'number', unit: '%', scale: 100, min: 0, max: 100 }
const color: EffectField = { key: 'color', kind: 'swatch' }
const points = (key: string, max = 1000): EffectField => ({ key, kind: 'number', unit: 'pt', min: key === 'distance' ? -1000 : 0, max })
const percent = (key: string): EffectField => ({ key, kind: 'number', unit: '%', min: 0, max: 100 })
const angle: EffectField = { key: 'angle', kind: 'number', unit: '°', min: -360, max: 360 }
const check = (key: string): EffectField => ({ key, kind: 'check' })

/** The effects, in InDesign's order. */
export const EFFECT_PAGES: readonly EffectPage[] = [
  { id: 'dropShadow', command: 'object.dropShadow', fields: [color, opacity, angle, check('globalLight'), points('distance'), points('size'), percent('spread')] },
  { id: 'innerShadow', command: 'object.innerShadow', fields: [color, opacity, angle, check('globalLight'), points('distance'), points('size'), percent('choke')] },
  { id: 'outerGlow', command: 'object.outerGlow', fields: [color, opacity, points('size'), percent('spread')] },
  { id: 'innerGlow', command: 'object.innerGlow', fields: [color, opacity, points('size'), percent('choke'), check('center')] },
  {
    id: 'bevel',
    command: 'object.bevel',
    fields: [points('size'), percent('depth'), angle, check('globalLight'), { key: 'highlight', kind: 'swatch' }, { ...opacity, key: 'highlightOpacity' }, { key: 'shadow', kind: 'swatch' }, { ...opacity, key: 'shadowOpacity' }],
  },
  { id: 'satin', command: 'object.satin', fields: [color, opacity, angle, points('distance'), points('size'), check('invert')] },
  { id: 'feather', command: 'object.feather', fields: [points('width')] },
  { id: 'directionalFeather', command: 'object.directionalFeather', fields: [points('top'), points('left'), points('bottom'), points('right')] },
  { id: 'gradientFeather', command: 'object.gradientFeather', fields: [check('radial'), angle, percent('start'), percent('end')] },
]

/** The dialog's values: `on` and each field as text (checks `true`/`false`) per effect, and Transparency. */
export type EffectsDraft = { opacity: string; blend: string; effects: Record<string, Record<string, string>> }

const round = (value: number) => Math.round(value * 1000) / 1000
const shown = (field: EffectField, value: unknown) =>
  typeof value === 'number' ? String(round(value * (field.scale ?? 1))) : typeof value === 'boolean' ? String(value) : typeof value === 'string' ? value : ''

/** What an object reads back for the Effects dialog. */
export type EffectsItem = { opacity?: number; blend?: string; effects?: Record<string, unknown> }

/** The values the dialog opens with, from the first selected object. */
export function effectsDraft(item: EffectsItem | undefined): EffectsDraft {
  const read = item?.effects ?? {}
  const effects: Record<string, Record<string, string>> = {}
  for (const page of EFFECT_PAGES) {
    const values: Record<string, string> = {}
    if (page.id === 'feather') {
      const width = typeof read.feather === 'number' ? read.feather : 0
      values.on = String(width > 0)
      values.width = String(round(width > 0 ? width : 9))
    } else if (page.id === 'directionalFeather') {
      const effect = (read.directionalFeather ?? {}) as { on?: boolean; widths?: number[] }
      values.on = String(Boolean(effect.on))
      ;['top', 'left', 'bottom', 'right'].forEach((side, index) => (values[side] = String(round(effect.widths?.[index] ?? 0))))
    } else {
      const effect = (read[page.id] ?? {}) as Record<string, unknown>
      values.on = String(Boolean(effect.on))
      for (const field of page.fields) values[field.key] = shown(field, effect[field.key])
    }
    effects[page.id] = values
  }
  return { opacity: String(round((item?.opacity ?? 1) * 100)), blend: item?.blend ?? 'Normal', effects }
}

const value = (field: EffectField, text: string): unknown => {
  if (field.kind === 'check') return text === 'true'
  if (field.kind === 'swatch') return text
  const number = Number(text)
  if (text.trim() === '' || !Number.isFinite(number)) return undefined
  return number / (field.scale ?? 1)
}

/**
 * The commands for what changed between `from` and `to` on the objects `ids`: `object.opacity`
 * for Transparency, and each changed effect's command with all its settings (and `on`). Null
 * when a value is not a number yet; empty when nothing changed.
 */
export function effectsSteps(ids: readonly number[], from: EffectsDraft, to: EffectsDraft): Array<[string, Record<string, unknown>]> | null {
  const steps: Array<[string, Record<string, unknown>]> = []
  if (to.opacity !== from.opacity || to.blend !== from.blend) {
    const amount = Number(to.opacity)
    if (to.opacity.trim() === '' || !Number.isFinite(amount) || amount < 0 || amount > 100) return null
    steps.push(['object.opacity', { ids: [...ids], opacity: amount / 100, blend: to.blend }])
  }
  for (const page of EFFECT_PAGES) {
    const before = from.effects[page.id] ?? {}
    const after = to.effects[page.id] ?? {}
    if (JSON.stringify(before) === JSON.stringify(after)) continue
    const on = after.on === 'true'
    const params: Record<string, unknown> = { ids: [...ids] }
    const values: Record<string, unknown> = {}
    for (const field of page.fields) {
      const next = value(field, after[field.key] ?? '')
      if (next === undefined || (field.kind === 'swatch' && next === '')) {
        if (field.kind === 'swatch') continue
        return null
      }
      values[field.key] = next
    }
    if (page.id === 'feather') params.width = on ? values.width : 0
    else if (page.id === 'directionalFeather') {
      params.on = on
      params.widths = ['top', 'left', 'bottom', 'right'].map((side) => values[side])
    } else Object.assign(params, values, { on })
    steps.push([page.command, params])
  }
  return steps
}

/** Whether an effect is on in the dialog's values (the page list marks it). */
export function effectOn(draft: EffectsDraft, id: string) {
  return draft.effects[id]?.on === 'true'
}
