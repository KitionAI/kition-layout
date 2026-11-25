/**
 * A command's parameters as the generic dialog's fields (plan 2026-11-25, stage 1): a number
 * field, a drop-down for a choice (the schema's `enum`, or one written into the description as
 * `a|b|c`), a check box, a colour, a text field, or a list typed as comma-separated values. The
 * keys every command shares (which objects, story, spread, file) and the ones the engine reads
 * beside its listed parameters are left out, so the engine's defaults apply to them. Pure, so it
 * is tested without a browser.
 */

type SchemaNode = {
  type?: string
  description?: string
  enum?: unknown[]
  default?: unknown
  minimum?: number
  maximum?: number
  oneOf?: SchemaNode[]
  items?: SchemaNode
}

/** A command as the engine lists it (`KitCommand`'s fields this module reads). */
export type ParamsCommand = { id: string; params?: string | null; schema?: unknown }

/** Keys every command (or every object command) shares: not the dialog's to ask. */
export const SHARED_KEYS: ReadonlySet<string> = new Set(['ids', 'id', 'story', 'frame', 'spread', 'toSpread', 'path', 'dir', 'base64', 'rect', 'attrs', 'entries', 'table'])

/** The description the engine gives keys it reads beside the listed ones: never asked. */
const BESIDE = /^read by the command beside/

export type ParamField =
  | { key: string; kind: 'number'; min: number; max: number; decimals: number; initial: number | null; unit: 'pt' | null }
  | { key: string; kind: 'choice'; options: readonly string[]; initial: string }
  | { key: string; kind: 'check'; initial: boolean }
  | { key: string; kind: 'color'; initial: string }
  | { key: string; kind: 'text'; initial: string }
  | { key: string; kind: 'list'; items: 'integer' | 'number' | 'string'; initial: string }

const NUMBER = String.raw`-?\d+(?:\.\d+)?`

/** The range a description gives ("0 to 1000", "1..1000", "0..1"), if any. */
export function rangeFromDescription(description: string | undefined): { min: number; max: number } | null {
  const match = description ? new RegExp(String.raw`^\s*(${NUMBER})\s*(?:to|\.\.|–)\s*(${NUMBER})`).exec(description) : null
  if (!match) return null
  const [min, max] = [Number(match[1]), Number(match[2])]
  return min < max ? { min, max } : null
}

/** A choice written into a description ("leftToRight|rightToLeft", "cmyk|rgb"), if any. */
export function choiceFromDescription(description: string | undefined): string[] | null {
  const match = description ? /^\s*([A-Za-z][\w-]*(?:\|[A-Za-z][\w-]*)+)\s*(?:$|[\s(,;—])/.exec(description) : null
  return match ? match[1].split('|') : null
}

/** A choice the compact `params` text gives a key (`type?: url|text|sms`), if any. */
export function choiceFromParams(params: string | null | undefined, key: string): string[] | null {
  const match = params ? new RegExp(String.raw`\b${key}\??\s*:\s*([A-Za-z][\w-]*(?:\|[A-Za-z][\w-]*)+)`).exec(params) : null
  return match ? match[1].split('|') : null
}

/**
 * Keys whose schema says another type than the command takes (Add to Dictionary's `word` is
 * listed as a boolean; the engine reads a word).
 */
const TYPE_FIXES: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  'spelling.addWord': { word: 'string' },
  'hyphenation.addException': { word: 'string' },
}

/** The default a compact `params` text gives a key (`count?: 1`, `rowGutter?: 12`), if it is a number. */
export function defaultFromParams(params: string | null | undefined, key: string): number | null {
  if (!params) return null
  const match = new RegExp(String.raw`\b${key}\?\s*:\s*(${NUMBER})\b(?!\s*(?:to|\.\.))`).exec(params)
  return match ? Number(match[1]) : null
}

/** The node a key's either-or schema offers a person: its plain number, text or choice, if any. */
function plainOf(node: SchemaNode): SchemaNode {
  if (!node.oneOf?.length) return node
  return node.oneOf.find((option) => option.type === 'number' || option.type === 'integer') ?? node.oneOf.find((option) => option.type === 'string') ?? node
}

/** Shared keys a command's dialog asks after all, because they are what it is about (Add Page to Spread's spread). */
export const ASKED: Readonly<Record<string, readonly string[]>> = { 'layout.pages.toSpread': ['spread'] }

/** One key's field, or null when the dialog does not ask for it. */
export function fieldFor(command: ParamsCommand, key: string, given: SchemaNode): ParamField | null {
  if ((SHARED_KEYS.has(key) && !ASKED[command.id]?.includes(key)) || BESIDE.test(given.description ?? '')) return null
  const fixed = TYPE_FIXES[command.id]?.[key]
  const node = fixed ? { type: fixed } : plainOf(given)
  const options = node.enum?.length && node.enum.every((value) => typeof value === 'string') ? (node.enum as string[]) : node.type === 'string' ? (choiceFromDescription(node.description ?? given.description) ?? choiceFromParams(command.params, key)) : null
  if (options) {
    const initial = typeof node.default === 'string' && options.includes(node.default) ? node.default : options[0]
    return { key, kind: 'choice', options, initial }
  }
  // An effect's `on` is what its dialog is for: checked.
  if (node.type === 'boolean') return { key, kind: 'check', initial: node.default === true || key === 'on' }
  if (node.type === 'number' || node.type === 'integer') {
    const integer = node.type === 'integer'
    const range = rangeFromDescription(node.description) ?? (node.minimum !== undefined && node.maximum !== undefined && node.minimum < node.maximum ? { min: node.minimum, max: node.maximum } : null)
    const fallback = typeof node.default === 'number' ? node.default : defaultFromParams(command.params, key)
    const points = /^points\b|\(pt\)|^pt\b/.test(node.description ?? given.description ?? '')
    return { key, kind: 'number', min: range?.min ?? node.minimum ?? -1_000_000, max: range?.max ?? node.maximum ?? 1_000_000, decimals: integer ? 0 : 2, initial: fallback, unit: points ? 'pt' : null }
  }
  if (node.type === 'string') {
    if (/colou?r$/i.test(key) && /#rrggbb/.test(node.description ?? '')) return { key, kind: 'color', initial: '#000000' }
    return { key, kind: 'text', initial: typeof node.default === 'string' ? node.default : '' }
  }
  // A list of pages or names, typed as "1, 3, 5"; a two-number index range is not a person's to type.
  if (node.type === 'array' && node.items && !/index range/.test(node.description ?? '')) {
    // A list the schema types loosely is of page numbers (`pages`) or of names (`styles`).
    const items = node.items.type === 'integer' || (node.items.type === 'json' && /^pages?$/.test(key)) ? 'integer' : node.items.type === 'number' ? 'number' : node.items.type === 'string' || node.items.type === 'json' ? 'string' : null
    if (items) return { key, kind: 'list', items, initial: '' }
  }
  // Objects, JSON and arrays of anything else keep the engine's defaults.
  return null
}

/**
 * Keys a command's dialog does not ask because a menu row of their own does it (Type on a Path ›
 * Delete Type from Path, Remove Section), or the row runs one way only (a static caption).
 */
export const OMITTED: Readonly<Record<string, readonly string[]>> = {
  'type.pathOptions': ['delete'],
  'layout.section': ['remove'],
  'object.caption': ['live'],
  // The gradient's ends are points on the page, which the Gradient Feather tool drags.
  'object.gradientFeather': ['from', 'to'],
}

/** Where `key` is first named in the compact `params` text (the command's own order), or past the end. */
function orderOf(params: string | null | undefined, key: string) {
  const at = params ? params.search(new RegExp(String.raw`\b${key}\b`)) : -1
  return at < 0 ? Number.MAX_SAFE_INTEGER : at
}

/** The fields the generic dialog shows for `command`, in the order its `params` text names them (rows before row gutter). */
export function paramFields(command: ParamsCommand): ParamField[] {
  const schema = (command.schema ?? {}) as Record<string, SchemaNode>
  const omitted = OMITTED[command.id] ?? []
  const keys = Object.keys(schema).sort((a, b) => orderOf(command.params, a) - orderOf(command.params, b))
  return keys.flatMap((key) => {
    const node = schema[key]
    if (omitted.includes(key)) return []
    const field = fieldFor(command, key, node)
    return field ? [field] : []
  })
}

export type ParamValues = Record<string, string | number | boolean | null>

/** Whether `value` is one `field` can show. */
export function fits(field: ParamField, value: unknown): boolean {
  switch (field.kind) {
    case 'number':
      return value === null || (typeof value === 'number' && Number.isFinite(value) && value >= field.min && value <= field.max)
    case 'choice':
      return typeof value === 'string' && field.options.includes(value)
    case 'check':
      return typeof value === 'boolean'
    case 'color':
      return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value)
    case 'text':
    case 'list':
      return typeof value === 'string'
  }
}

/** A list field's given value (an array, as the engine takes it) as the text the field shows. */
function shown(field: ParamField, value: unknown): unknown {
  return field.kind === 'list' && Array.isArray(value) ? value.join(', ') : value
}

/** The fields' starting values: the given or remembered ones where they fit, else each field's own. */
export function initialValues(fields: readonly ParamField[], remembered: Readonly<Record<string, unknown>> = {}): ParamValues {
  return Object.fromEntries(
    fields.map((field) => {
      const kept = shown(field, remembered[field.key])
      return [field.key, fits(field, kept) ? (kept as string | number | boolean | null) : field.initial]
    }),
  )
}

/** A list field's text as the engine's array; null when an item is not one the field takes. */
export function listOf(field: Extract<ParamField, { kind: 'list' }>, text: string): Array<string | number> | null {
  const items = text.split(/[,，]/).map((item) => item.trim()).filter(Boolean)
  if (field.items === 'string') return items
  const numbers = items.map(Number)
  return numbers.every((value) => (field.items === 'integer' ? Number.isInteger(value) : Number.isFinite(value))) ? numbers : null
}

/** The command's parameters from the fields' values: an empty text, list or number leaves its key out (the engine's default). */
export function paramsOf(fields: readonly ParamField[], values: ParamValues): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const field of fields) {
    const value = values[field.key]
    if (value === null || value === undefined) continue
    if ((field.kind === 'text' || field.kind === 'list') && value === '') continue
    if (field.kind === 'list') {
      const list = listOf(field, String(value))
      if (list?.length) out[field.key] = list
      continue
    }
    out[field.key] = value
  }
  return out
}

/** Keys a command needs that its compact `params` text does not name as optional; the dialog's OK waits for them. */
const REQUIRED_OVERRIDES: Readonly<Record<string, readonly string[]>> = {
  // A table of contents from `entries` or `styles`: the dialog asks for the styles.
  'toc.generate': ['styles'],
  // A QR code needs what it encodes (a URL by default).
  'object.qrCode': ['content'],
  // A cross-reference to the first paragraph holding this text (its other destinations are ids).
  'xref.insert': ['paragraph'],
  // An index entry needs its topic (the selected text is not read back into the dialog).
  'index.addReference': ['topic'],
}

/** The keys `command` cannot run without, from its `params` text (`{latex, size?: …}` needs `latex`). */
export function requiredKeys(command: ParamsCommand): string[] {
  const own = REQUIRED_OVERRIDES[command.id]
  if (own) return [...own]
  const text = (command.params ?? '').trim()
  if (!text.startsWith('{')) return []
  // The top-level names of the braces at the start: split at commas outside brackets, parentheses and braces.
  const parts: string[] = []
  let depth = 0
  let start = 1
  for (let index = 1; index < text.length; index += 1) {
    const char = text[index]
    if (char === '[' || char === '(' || char === '{') depth += 1
    else if ((char === ']' || char === ')' || char === '}') && depth > 0) depth -= 1
    else if (depth === 0 && (char === ',' || char === '}')) {
      parts.push(text.slice(start, index))
      start = index + 1
      if (char === '}') break
    }
  }
  const keys: string[] = []
  for (const part of parts) {
    // "url? | email? | page?" names alternatives: none of them is needed alone.
    if (part.includes('|') && /\?\s*\|/.test(part)) continue
    const match = /^\s*([A-Za-z]\w*)(\?)?/.exec(part)
    if (match && !match[2]) keys.push(match[1])
  }
  return keys
}

/** Whether the values can run: every list reads as the engine's array, and every needed field is filled. */
export function valuesValid(fields: readonly ParamField[], values: ParamValues, required: readonly string[] = []) {
  const lists = fields.every((field) => field.kind !== 'list' || values[field.key] === '' || listOf(field, String(values[field.key] ?? '')) !== null)
  const filled = fields.every((field) => !required.includes(field.key) || (values[field.key] !== null && values[field.key] !== ''))
  return lists && filled
}

/**
 * Words for a key or a choice in English Title Case ("columnGutter" → "Column Gutter",
 * "fitContentToFrame" → "Fit Content To Frame"), the form the phrase tables list them in.
 */
export function humanize(word: string) {
  const spaced = word
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[-_]+/g, ' ')
    .trim()
  return spaced.replace(/(^|\s)([a-z])/g, (_, space: string, letter: string) => `${space}${letter.toUpperCase()}`)
}
