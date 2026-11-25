/**
 * Ruler guides by hand (plan 2026-11-25, stage 3): where each guide lies on the canvas, which one
 * a press is on, and where a dragged guide lands: snapped to the page's edges, centre, margins
 * and columns, the other guides and the objects' edges and centres (within 4 screen pixels), or
 * with Shift to the ruler's ticks. Positions are in spread points as the engine keeps them
 * (`guide.list`, `guide.add`, `guide.move`). Pure, so it is tested in Node.
 */
import { toDocument, type DocumentInfo, type Item, type Spread } from './layout.ts'
import { rulerTicks, type Unit } from './units.ts'

/** A guide as `guide.list` reports it, with the spread it is on. */
export type Guide = { spread: number; page: number; index: number; orientation: 'horizontal' | 'vertical'; position: number; spreadGuide: boolean }

/** How near (screen pixels) a press must be to take a guide, and a drag to snap. */
export const GUIDE_HIT_PX = 3
export const GUIDE_SNAP_PX = 4

/** `guide.list`'s answer for spread `spread` as guides. */
export function listedGuides(spread: number, listed: unknown): Guide[] {
  if (!Array.isArray(listed)) return []
  return listed.flatMap((entry) => {
    const guide = entry as { page?: unknown; index?: unknown; orientation?: unknown; position?: unknown; spread?: unknown }
    if (typeof guide.page !== 'number' || typeof guide.index !== 'number' || typeof guide.position !== 'number') return []
    if (guide.orientation !== 'horizontal' && guide.orientation !== 'vertical') return []
    return [{ spread, page: guide.page, index: guide.index, orientation: guide.orientation, position: guide.position, spreadGuide: guide.spread === true }]
  })
}

/** The span a guide is drawn across, in document pixels: its page, or the canvas for a spread guide. */
export function guideSpan(info: DocumentInfo, guide: Guide): { x0: number; y0: number; x1: number; y1: number } | null {
  const spread = info.spreads.find((item) => item.index === guide.spread)
  if (!spread) return null
  if (guide.spreadGuide) return { x0: info.canvas.x0, y0: spread.area.y0 - 36, x1: info.canvas.x1, y1: spread.area.y1 + 36 }
  const page = spread.pages[guide.page]
  if (!page) return null
  const a = toDocument(spread, page.bounds[0], page.bounds[1])
  const b = toDocument(spread, page.bounds[2], page.bounds[3])
  return { x0: a.x, y0: a.y, x1: b.x, y1: b.y }
}

/** A guide's position in document pixels (x for a vertical guide, y for a horizontal one). */
export function guideCanvasPosition(info: DocumentInfo, guide: Guide): number | null {
  const spread = info.spreads.find((item) => item.index === guide.spread)
  if (!spread) return null
  return guide.orientation === 'vertical' ? toDocument(spread, guide.position, 0).x : toDocument(spread, 0, guide.position).y
}

/** The guide within `GUIDE_HIT_PX` screen pixels of document pixel (`x`, `y`) at `zoom`, nearest first. */
export function guideAt(info: DocumentInfo, guides: readonly Guide[], x: number, y: number, zoom: number): Guide | null {
  const tolerance = GUIDE_HIT_PX / Math.max(zoom, 1e-6)
  let best: Guide | null = null
  let distance = Number.POSITIVE_INFINITY
  for (const guide of guides) {
    const at = guideCanvasPosition(info, guide)
    const span = guideSpan(info, guide)
    if (at === null || !span) continue
    const along = guide.orientation === 'vertical' ? y >= span.y0 - tolerance && y <= span.y1 + tolerance : x >= span.x0 - tolerance && x <= span.x1 + tolerance
    const off = Math.abs((guide.orientation === 'vertical' ? x : y) - at)
    if (along && off <= tolerance && off < distance) {
      best = guide
      distance = off
    }
  }
  return best
}

/** The spread point (`x`, `y` in spread points) of a document pixel on `spread`. */
export function toSpread(spread: Spread, x: number, y: number) {
  return { x: x - spread.area.x0 + spread.left, y: y - spread.area.y0 }
}

/** Every line on `spread` a dragged guide of `orientation` snaps to (spread points along its axis). */
export function snapTargets(spread: Spread, guides: readonly Guide[], orientation: Guide['orientation'], except: Guide | null): number[] {
  const vertical = orientation === 'vertical'
  const out: number[] = []
  for (const page of spread.pages) {
    const [x0, y0, x1, y1] = page.bounds
    out.push(...(vertical ? [x0, (x0 + x1) / 2, x1] : [y0, (y0 + y1) / 2, y1]))
    const margins = page.margins
    if (margins) {
      // Inside is at the spine: the left margin of a right-hand page (one with a page to its left).
      const right = spread.pages.length > 1 && page !== spread.pages[0]
      const left = right ? margins.inside : spread.pages.length > 1 ? margins.outside : margins.inside
      const rightMargin = right ? margins.outside : spread.pages.length > 1 ? margins.inside : margins.outside
      if (vertical) {
        const a = x0 + left
        const b = x1 - rightMargin
        out.push(a, b)
        const columns = Math.max(1, page.columns)
        if (columns > 1) for (let at = 1; at < columns; at += 1) out.push(a + ((b - a) * at) / columns)
      } else out.push(y0 + margins.top, y1 - margins.bottom)
    }
  }
  for (const guide of guides) {
    if (guide.spread !== spread.index || guide.orientation !== orientation) continue
    if (except && guide.page === except.page && guide.index === except.index && guide.spread === except.spread) continue
    out.push(guide.position)
  }
  const items = (list: readonly Item[]) => {
    for (const item of list) {
      if (item.hidden) continue
      const [x0, y0, x1, y1] = item.bounds
      out.push(...(vertical ? [x0, (x0 + x1) / 2, x1] : [y0, (y0 + y1) / 2, y1]))
    }
  }
  items(spread.items)
  return out
}

/**
 * Where a dragged guide lands (spread points along its axis): with Shift on the nearest tick of
 * the ruler in `unit` (its origin the page's top-left, as the rulers count), else on the nearest
 * target within `GUIDE_SNAP_PX` screen pixels, else where the pointer is.
 */
export function snapGuide(position: number, targets: readonly number[], zoom: number, options: { shift?: boolean; unit?: Unit; origin?: number } = {}): number {
  if (options.shift) {
    const { major, parts } = rulerTicks(options.unit ?? 'pt', zoom)
    const step = major / Math.max(1, parts)
    const origin = options.origin ?? 0
    return step > 0 ? origin + Math.round((position - origin) / step) * step : position
  }
  const tolerance = GUIDE_SNAP_PX / Math.max(zoom, 1e-6)
  let best = position
  let distance = tolerance
  for (const target of targets) {
    const off = Math.abs(target - position)
    if (off <= distance) {
      best = target
      distance = off
    }
  }
  return best
}

/** The page of `spread` under spread x `x` (its index in the spread), or null over the pasteboard. */
export function pageUnder(spread: Spread, x: number, y: number): number | null {
  const at = spread.pages.findIndex((page) => x >= page.bounds[0] && x <= page.bounds[2] && y >= page.bounds[1] && y <= page.bounds[3])
  return at < 0 ? null : at
}
