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
