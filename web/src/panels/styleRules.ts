/**
 * Paragraph and Character Style Options (plan 2026-11-25, stage 7): the dialog's sections and
 * fields, the values it opens with (the style's own, from `inspect().styleDefs`), and the one
 * `style.paragraph.edit` / `style.character.edit` its changes make. Pure, so it is tested in Node.
 */
import { PARAGRAPH_ALIGNS } from './panelLogic.ts'

export type StyleKind = 'paragraph' | 'character'

/** A style as `inspect().styleDefs` reads it back. */
export type StyleDef = {
  name: string
  basedOn?: string | null
  nextStyle?: string | null
  /** What the style sets itself (sparse). */
  para?: Record<string, unknown>
  chars?: Record<string, unknown>
  /** A paragraph style's values with its Based On chain (complete). */
  resolvedPara?: Record<string, unknown>
  /** A paragraph style's character values (complete); a character style's chain (sparse). */
  resolvedChars?: Record<string, unknown>
  /** Paragraphs (or runs of characters) that use it. */
  uses?: number
}

export type StyleDefs = { paragraph: StyleDef[]; character: StyleDef[]; object?: ObjectStyleDef[] }

/** What an object style includes (each part null when the style leaves it out), as `inspect()` reads it (phase 11). */
export type ObjectStyleParts = {
  fill: string | null
  fillTint: number | null
  stroke: string | null
  strokeTint: number | null
  strokeWeight: number | null
  paragraphStyle: string | null
  textFrame: { columns: number; gutter: number; inset: number[]; verticalJustification: string } | null
}

/** An object style: its own parts, those with its Based On chain, and how many objects use it. */
export type ObjectStyleDef = { name: string; basedOn?: string | null; own: ObjectStyleParts; resolved: ObjectStyleParts; uses?: number }

/** A field of the dialog: an attribute of the style's `para` or `chars`. */
export type StyleField = {
  key: string
  part: 'para' | 'chars'
  kind: 'number' | 'text' | 'choice' | 'check' | 'font' | 'swatch'
  /** Shown multiplied by it (a fraction as a percentage). */
  scale?: number
  unit?: string
  options?: readonly string[]
}

export type StyleSection = { id: string; kinds: readonly StyleKind[]; fields: readonly StyleField[] }

const both = ['paragraph', 'character'] as const
const num = (key: string, part: 'para' | 'chars', unit?: string, scale?: number): StyleField => ({ key, part, kind: 'number', unit, scale })

/** InDesign's sections, those the engine's attributes fill (Tabs, Rules, Keeps, GREP: the menu's own dialogs). */
export const STYLE_SECTIONS: readonly StyleSection[] = [
  { id: 'general', kinds: both, fields: [] },
  {
    id: 'basicCharacter',
    kinds: both,
    fields: [
      { key: 'fontFamily', part: 'chars', kind: 'font' },
      { key: 'fontStyle', part: 'chars', kind: 'text' },
      num('size', 'chars', 'pt'),
      num('leading', 'chars', 'pt'),
      num('tracking', 'chars'),
      { key: 'capitalization', part: 'chars', kind: 'choice', options: ['normal', 'allCaps', 'smallCaps'] },
      { key: 'position', part: 'chars', kind: 'choice', options: ['normal', 'superscript', 'subscript'] },
      { key: 'underline', part: 'chars', kind: 'check' },
      { key: 'strikethrough', part: 'chars', kind: 'check' },
    ],
  },
  {
    id: 'advancedCharacter',
    kinds: both,
    fields: [num('hScale', 'chars', '%', 100), num('vScale', 'chars', '%', 100), num('baselineShift', 'chars', 'pt'), num('skew', 'chars', '°')],
  },
  {
    id: 'indentsSpacing',
    kinds: ['paragraph'],
    fields: [
      { key: 'align', part: 'para', kind: 'choice', options: PARAGRAPH_ALIGNS },
      num('leftIndent', 'para', 'pt'),
      num('rightIndent', 'para', 'pt'),
      num('firstLineIndent', 'para', 'pt'),
      num('lastLineIndent', 'para', 'pt'),
      num('spaceBefore', 'para', 'pt'),
      num('spaceAfter', 'para', 'pt'),
    ],
  },
  { id: 'dropCaps', kinds: ['paragraph'], fields: [num('dropCapLines', 'para'), num('dropCapChars', 'para')] },
  { id: 'hyphenation', kinds: ['paragraph'], fields: [{ key: 'hyphenate', part: 'para', kind: 'check' }] },
  {
    id: 'characterColor',
    kinds: both,
    fields: [{ key: 'fill', part: 'chars', kind: 'swatch' }, num('fillTint', 'chars', '%', 100), { key: 'stroke', part: 'chars', kind: 'swatch' }, num('strokeWeight', 'chars', 'pt')],
  },
]

/** The sections a style of `kind` shows. */
export function sectionsOf(kind: StyleKind) {
  return STYLE_SECTIONS.filter((section) => section.kinds.includes(kind))
}

/** The dialog's values: field key → text (numbers, names), `true`/`false` (checks); '' is not set (a character style leaves it to the text). */
export type StyleDraft = Record<string, string>

/** The general settings: name, Based On, Next Style (paragraph styles). */
export type StyleGeneral = { name: string; basedOn: string; nextStyle: string }

const round = (value: number) => Math.round(value * 1000) / 1000

/** A field's value as the dialog shows it. */
export function shownValue(field: StyleField, value: unknown): string {
  if (value == null) return ''
  if (field.key === 'leading') {
    const leading = value as { kind?: string; value?: number }
    return leading.kind === 'points' && typeof leading.value === 'number' ? String(round(leading.value)) : 'auto'
  }
  if (typeof value === 'number') return String(round(value * (field.scale ?? 1)))
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  return typeof value === 'string' ? value : ''
}

/** A field's value for the engine, from the dialog's text; undefined when it is not one (a bad number). */
export function engineValue(field: StyleField, text: string): unknown {
  if (text === '') return null
  if (field.key === 'leading') {
    if (text.trim().toLowerCase() === 'auto') return { kind: 'auto' }
    const points = Number(text)
    return Number.isFinite(points) && points > 0 ? { kind: 'points', value: points } : undefined
  }
  if (field.kind === 'number') {
    const value = Number(text)
    return Number.isFinite(value) ? value / (field.scale ?? 1) : undefined
  }
  if (field.kind === 'check') return text === 'true'
  return text
}

/**
 * The values a style opens with: a paragraph style's resolved values (what its text looks like),
 * a character style's own and Based On ones ('' where it leaves the text as it is).
 */
export function styleDraft(kind: StyleKind, def: StyleDef | undefined): StyleDraft {
  const draft: StyleDraft = {}
  for (const section of sectionsOf(kind)) {
    for (const field of section.fields) {
      const source = kind === 'paragraph' ? (field.part === 'para' ? def?.resolvedPara : def?.resolvedChars) : def?.resolvedChars
      draft[field.key] = shownValue(field, source?.[field.key])
    }
  }
  return draft
}

/** The general settings a style opens with. */
export function styleGeneral(def: StyleDef | undefined, name: string): StyleGeneral {
  return { name: def?.name ?? name, basedOn: def?.basedOn ?? '', nextStyle: def?.nextStyle ?? '' }
}

/**
 * The `style.<kind>.edit` parameters for what changed between `from` and `to` (null: nothing did,
 * or a value is not valid yet). A paragraph style sets a changed attribute; a character style
 * also unsets one cleared to ''.
 */
export function styleEdit(kind: StyleKind, name: string, from: { general: StyleGeneral; draft: StyleDraft }, to: { general: StyleGeneral; draft: StyleDraft }): Record<string, unknown> | null {
  const params: Record<string, unknown> = { name }
  const para: Record<string, unknown> = {}
  const chars: Record<string, unknown> = {}
  for (const section of sectionsOf(kind)) {
    for (const field of section.fields) {
      const before = from.draft[field.key] ?? ''
      const after = to.draft[field.key] ?? ''
      if (before === after) continue
      if (after === '' && kind === 'paragraph') continue
      const value = engineValue(field, after)
      if (value === undefined) return null
      // The same value written another way ('Auto', '11.0').
      if (JSON.stringify(value) === JSON.stringify(engineValue(field, before))) continue
      ;(field.part === 'para' ? para : chars)[field.key] = value
    }
  }
  if (Object.keys(para).length) params.para = para
  if (Object.keys(chars).length) params.chars = chars
  const rename = to.general.name.trim()
  if (rename && rename !== from.general.name) params.rename = rename
  if (to.general.basedOn !== from.general.basedOn) params.basedOn = to.general.basedOn || null
  if (kind === 'paragraph' && to.general.nextStyle !== from.general.nextStyle) params.nextStyle = to.general.nextStyle || null
  return Object.keys(params).length > 1 ? params : null
}

/** The attributes a new style is made with: every field that has a value. */
export function styleAttrs(kind: StyleKind, draft: StyleDraft): { para: Record<string, unknown>; chars: Record<string, unknown> } {
  const para: Record<string, unknown> = {}
  const chars: Record<string, unknown> = {}
  for (const section of sectionsOf(kind)) {
    for (const field of section.fields) {
      const value = draft[field.key] ? engineValue(field, draft[field.key]) : undefined
      if (value !== undefined && value !== null) (field.part === 'para' ? para : chars)[field.key] = value
    }
  }
  return { para, chars }
}

/** A free name for a copy of `name` ("Body copy", "Body copy 2", …). */
export function copyName(name: string, taken: readonly string[]) {
  const base = `${name} copy`
  let next = base
  for (let n = 2; taken.includes(next); n += 1) next = `${base} ${n}`
  return next
}

/** The `style.<kind>.add` parameters of a copy of `def` (what it sets, its Based On and Next Style). */
export function duplicateParams(kind: StyleKind, def: StyleDef, taken: readonly string[]): Record<string, unknown> {
  const params: Record<string, unknown> = { name: copyName(def.name, taken), chars: def.chars ?? {} }
  if (def.basedOn) params.basedOn = def.basedOn
  if (kind === 'paragraph') {
    params.para = def.para ?? {}
    if (def.nextStyle) params.nextStyle = def.nextStyle
  }
  return params
}

/** Styles the engine makes and keeps: they can be applied and edited but not deleted. */
export const BUILT_IN_STYLES: ReadonlySet<string> = new Set(['[No Paragraph Style]', '[Basic Paragraph]', '[None]'])

/** What a deleted style's text may take instead: the other styles (built-in first). */
export function replacementChoices(kind: StyleKind, names: readonly string[], deleting: string) {
  const fallback = kind === 'paragraph' ? '[Basic Paragraph]' : '[None]'
  const others = names.filter((name) => name !== deleting && name !== '[No Paragraph Style]')
  return [fallback, ...others.filter((name) => name !== fallback)]
}
