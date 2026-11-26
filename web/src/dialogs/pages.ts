/**
 * The page dialogs' arithmetic (plan 2026-11-25, stage 1): page numbers typed as "3, 5-7", and
 * the engine commands Insert Pages and Move Pages run for a place (after or before a page, at the
 * start or the end). Page numbers are counted from 1, the engine's indexes from 0. Pure, so it is
 * tested in Node.
 */

export type Where = 'after' | 'before' | 'start' | 'end'

/** The pages `text` names ("3", "3, 5-7"), sorted, each once; null when one is not a page of `count`. */
export function pagesFromText(text: string, count: number): number[] | null {
  const parts = text.split(/[,，\s]+/).filter(Boolean)
  if (!parts.length) return null
  const out = new Set<number>()
  for (const part of parts) {
    const range = /^(\d+)\s*[-–]\s*(\d+)$/.exec(part)
    const [from, to] = range ? [Number(range[1]), Number(range[2])] : [Number(part), Number(part)]
    if (!Number.isInteger(from) || !Number.isInteger(to) || from < 1 || to > count || from > to) return null
    for (let page = from; page <= to; page += 1) out.add(page)
  }
  return [...out].sort((a, b) => a - b)
}

/** The commands that insert `count` pages at a place, with `parent` (null: none). */
export function insertPlan({ count, where, page, pages, parent }: { count: number; where: Where; page: number; pages: number; parent: string | null }): Array<[string, Record<string, unknown>]> {
  const insert = (after: number) => ['layout.pages.insert', { count, after, parent }] as [string, Record<string, unknown>]
  if (where === 'end') return [insert(pages - 1)]
  if (where === 'after') return [insert(page - 1)]
  if (where === 'before' && page > 1) return [insert(page - 2)]
  // At the start: after the first page, then the first page moves behind the new ones.
  return [insert(0), ['layout.pages.move', { from: 0, to: count }]]
}

/**
 * The `layout.pages.move` steps (`to` is the page's index once moved) that put `pages` (numbers
 * from 1) together at a place among the others, in their order; null when the place is one of the
 * pages moved. An order already in place needs no step.
 */
export function movePlan({ pages, where, page, count }: { pages: readonly number[]; where: Where; page: number; count: number }): Array<{ from: number; to: number }> | null {
  const moving = pages.map((number) => number - 1)
  if ((where === 'after' || where === 'before') && (page < 1 || page > count || moving.includes(page - 1))) return null
  const rest = Array.from({ length: count }, (_, index) => index).filter((index) => !moving.includes(index))
  const at = where === 'start' ? 0 : where === 'end' ? rest.length : rest.indexOf(page - 1) + (where === 'after' ? 1 : 0)
  const target = [...rest.slice(0, at), ...moving, ...rest.slice(at)]
  const order = Array.from({ length: count }, (_, index) => index)
  const steps: Array<{ from: number; to: number }> = []
  target.forEach((index, to) => {
    const from = order.indexOf(index)
    if (from === to) return
    order.splice(from, 1)
    order.splice(to, 0, index)
    steps.push({ from, to })
  })
  return steps
}

/** A page as `inspect()` reports it: its box on its spread, its side and margins. */
export type InspectedPage = { bounds: number[]; side?: string; margins?: { top?: number; bottom?: number; inside?: number; outside?: number } }

/** The margin box of `page` on its spread: the inside margin is on the left of a right-hand page and of a single one. */
export function marginBox(page: InspectedPage): [number, number, number, number] {
  const [x0, y0, x1, y1] = page.bounds
  const m = page.margins ?? {}
  const left = page.side === 'left' ? (m.outside ?? 0) : (m.inside ?? 0)
  const right = page.side === 'left' ? (m.inside ?? 0) : (m.outside ?? 0)
  return [x0 + left, y0 + (m.top ?? 0), x1 - right, y1 - (m.bottom ?? 0)]
}

/**
 * New Document's Primary Text Frame (plan stage 8): a text frame filling each page's margins, with
 * the document's columns, threaded page to page into one story that is the primary story (Smart
 * Text Reflow adds pages for it), as the engine's own new document makes them, with its commands.
 */
export async function addPrimaryTextFrames(execute: (command: string, params: Record<string, unknown>) => Promise<string>, columns: number, gutter: number) {
  const info = JSON.parse(await execute('file.inspect', {})) as { spreads?: Array<{ index?: number; pages?: InspectedPage[] }> }
  let previous: number | null = null
  let first: number | null = null
  for (const [index, spread] of (info.spreads ?? []).entries()) {
    for (const page of spread.pages ?? []) {
      const frame = JSON.parse(await execute('frame.add', { spread: spread.index ?? index, rect: marginBox(page), content: 'text', caret: false })) as { id: number }
      await execute('object.textFrameOptions', { ids: [frame.id], columns, gutter })
      if (previous != null) await execute('frame.thread', { from: previous, to: frame.id })
      else first = frame.id
      previous = frame.id
    }
  }
  if (first == null) return
  await execute('select.set', { ids: [first] })
  await execute('object.primaryTextFrame', { ids: [first], on: true })
  await execute('select.set', { ids: [] })
}

/** A numbering section as `inspect()` reads it (phase 11): its first page counted from 1. */
export type SectionInfo = { page: number; startNumber?: number | null; style?: string; prefix?: string; includePrefix?: boolean; marker?: string }

/**
 * The values Numbering & Section Options opens with for page `page` (from 0, phase 11): the
 * section that starts there as the document has it (a page that starts none: the engine's
 * defaults, with the page only).
 */
export function sectionValues(sections: readonly SectionInfo[], page: number): Record<string, unknown> {
  const section = sections.find((one) => one.page === page + 1)
  if (!section) return { page: page + 1 }
  const values: Record<string, unknown> = { page: page + 1, style: section.style ?? 'arabic', prefix: section.prefix ?? '', includePrefix: Boolean(section.includePrefix), marker: section.marker ?? '' }
  if (section.startNumber != null) values.startNumber = section.startNumber
  return values
}

/** The page (from 0) each section starts on, with what the Pages panel's mark tells: its marker or prefix. */
export function sectionStarts(sections: readonly SectionInfo[]): Map<number, string> {
  return new Map(sections.map((section) => [section.page - 1, section.marker || section.prefix || '']))
}

/** Where `pages` (numbers from 1) end up (indexes from 0) once `movePlan` with the same values has moved them. */
export function movedTo({ pages, where, page, count }: { pages: readonly number[]; where: Where; page: number; count: number }): number[] {
  const moving = pages.map((number) => number - 1)
  const rest = Array.from({ length: count }, (_, index) => index).filter((index) => !moving.includes(index))
  const at = where === 'start' ? 0 : where === 'end' ? rest.length : rest.indexOf(page - 1) + (where === 'after' ? 1 : 0)
  return moving.map((_, i) => at + i)
}
