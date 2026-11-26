/**
 * Preflight and spelling as the person reads them (plan 2026-11-25, stage 8): each preflight issue
 * worded from its kind and the values the engine gives with it (`details`), never the engine's
 * English message; the order a spelling check walks its words; and the steps Change All runs.
 * Pure, so it is tested in Node.
 */

/** A preflight issue as `preflight.run` answers it (`page` counts from 0). */
export type Issue = {
  severity: 'error' | 'warning'
  kind: string
  message: string
  item?: number
  page?: number
  details?: Record<string, unknown> | null
}

/** `preflight.run`'s answer. */
export type Preflight = { errors: number; warnings: number; issues: Issue[] }

/** The preflight kinds the editor words; any other is worded by its severity alone. */
export const PREFLIGHT_KINDS = ['overset', 'missingFont', 'missingGlyphs', 'missingLink', 'lowResolution', 'emptyText', 'rgbColor', 'unsupportedTypography'] as const

type Words = (key: string, values?: Record<string, string | number>) => string

/** An issue in the person's language: `preflight.<kind>` with the values its details give. */
export function issueText(issue: Issue, t: Words): string {
  const d = issue.details ?? {}
  const text = (key: string) => (typeof d[key] === 'string' ? (d[key] as string) : '')
  const number = (key: string) => (typeof d[key] === 'number' ? (d[key] as number) : 0)
  switch (issue.kind) {
    case 'overset':
      return t('preflight.overset', { chars: number('chars') })
    case 'missingFont':
      return t('preflight.missingFont', { font: text('font') })
    case 'missingGlyphs': {
      const missing = Array.isArray(d.missing) ? (d.missing as unknown[]).filter((c): c is string => typeof c === 'string').join(' ') : ''
      const more = number('more')
      return t(more ? 'preflight.missingGlyphsMore' : 'preflight.missingGlyphs', { font: text('font'), chars: missing, more })
    }
    case 'missingLink':
      return text('name') ? t('preflight.missingLinkNamed', { name: text('name') }) : t('preflight.missingLink')
    case 'lowResolution':
      return t('preflight.lowResolution', { name: text('name'), ppi: number('ppi'), min: number('minPpi') })
    case 'emptyText':
      return t('preflight.emptyText')
    case 'rgbColor':
      return t('preflight.rgbColor', { swatch: text('swatch') })
    case 'unsupportedTypography':
      return t('preflight.unsupportedTypography')
    default:
      return t(issue.severity === 'error' ? 'preflight.otherError' : 'preflight.otherWarning')
  }
}

/** A word `spelling.check` found: its story, byte range, the word, suggestions and its frame. */
export type Misspelling = { story: number; start: number; end: number; word: string; suggestions: string[]; frame: number | null }

/** The words a check still shows: those not ignored this session (Ignore All), by spelling. */
export function shownWords(words: readonly Misspelling[], ignored: ReadonlySet<string>): Misspelling[] {
  return words.filter((word) => !ignored.has(word.word.toLowerCase()))
}

/**
 * Where the check goes on after every `word` before position `index` went away (Ignore All, Add to
 * Dictionary, Change All): the words before it that stay, so the next one is the one after it.
 */
export function indexAfterRemoving(words: readonly Misspelling[], index: number, word: string): number {
  const gone = word.toLowerCase()
  return words.slice(0, Math.max(0, index)).filter((item) => item.word.toLowerCase() !== gone).length
}

/**
 * Change All: every occurrence of `word` (the same spelling) changed to `to`, as `spelling.change`
 * steps, each story's from its end, so the offsets still to come stay right.
 */
export function changeAllSteps(words: readonly Misspelling[], word: string, to: string): Array<[string, Record<string, unknown>]> {
  return words
    .filter((item) => item.word === word)
    .sort((a, b) => a.story - b.story || b.start - a.start)
    .map((item) => ['spelling.change', { story: item.story, start: item.start, end: item.end, to }])
}
