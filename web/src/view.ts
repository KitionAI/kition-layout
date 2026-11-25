/**
 * The view's commands (plan 2026-11-25, stage 2): fit the page, the spread, the selection or the
 * whole pasteboard, 100% and 200%, and go to the first, previous, next or last page or spread.
 * Pure geometry on the engine's report, so it is tested in Node; `App` moves the canvas.
 */
import type { RasterRect } from '@kition/sdk/editor'
import { findItem, toDocument, type DocumentInfo, type Spread } from './layout.ts'

/** A view action, as a `kit:` row runs it (`kit:view.fitPage`). */
export type ViewAction =
  | 'fitPage'
  | 'fitSpread'
  | 'actualSize'
  | 'zoom200'
  | 'fitSelection'
  | 'entirePasteboard'
  | 'firstPage'
  | 'previousPage'
  | 'nextPage'
  | 'lastPage'
  | 'previousSpread'
  | 'nextSpread'

/** The view actions with their keys (InDesign's, Cmd+0 fitting the spread as the reference application does). */
export const VIEW_ACTIONS: ReadonlyArray<readonly [ViewAction, string, string]> = [
  ['fitPage', 'Fit Page in Window', 'Cmd+Alt+0'],
  ['fitSpread', 'Fit Spread in Window', 'Cmd+0'],
  ['actualSize', 'Actual Size', 'Cmd+1'],
  ['zoom200', 'Zoom to 200%', 'Cmd+2'],
  ['fitSelection', 'Fit Selection in Window', 'Cmd+Alt+='],
  ['entirePasteboard', 'Entire Pasteboard', 'Cmd+Alt+Shift+0'],
  ['firstPage', 'First Page', 'Cmd+Shift+PageUp'],
  ['previousPage', 'Previous Page', 'Shift+PageUp'],
  ['nextPage', 'Next Page', 'Shift+PageDown'],
  ['lastPage', 'Last Page', 'Cmd+Shift+PageDown'],
  ['previousSpread', 'Previous Spread', 'PageUp'],
  ['nextSpread', 'Next Spread', 'PageDown'],
]

/** Keys that also run a view action (Alt+Page Down: the next spread, as InDesign's). */
export const VIEW_KEY_ALIASES: ReadonlyArray<readonly [string, ViewAction]> = [
  ['Alt+PageUp', 'previousSpread'],
  ['Alt+PageDown', 'nextSpread'],
]

/** The spread a page (0-based) is on. */
export function spreadOfPage(info: DocumentInfo, page: number): Spread | null {
  return info.spreads.find((spread) => spread.pages.some((item) => item.index === page)) ?? null
}

/** A page's area in document pixels. */
export function pageArea(info: DocumentInfo, page: number): RasterRect | null {
  const spread = spreadOfPage(info, page)
  const bounds = spread?.pages.find((item) => item.index === page)?.bounds
  if (!spread || !bounds) return null
  const a = toDocument(spread, bounds[0], bounds[1])
  const b = toDocument(spread, bounds[2], bounds[3])
  return { x0: a.x, y0: a.y, x1: b.x, y1: b.y }
}

/** The selected objects' bounds together, in document pixels; null with nothing selected. */
export function selectionArea(info: DocumentInfo): RasterRect | null {
  let area: RasterRect | null = null
  for (const id of info.selection) {
    const found = findItem(info, id)
    if (!found) continue
    const [x0, y0, x1, y1] = found.item.bounds
    const a = toDocument(found.spread, x0, y0)
    const b = toDocument(found.spread, x1, y1)
    const next = { x0: Math.min(a.x, b.x), y0: Math.min(a.y, b.y), x1: Math.max(a.x, b.x), y1: Math.max(a.y, b.y) }
    area = area ? { x0: Math.min(area.x0, next.x0), y0: Math.min(area.y0, next.y0), x1: Math.max(area.x1, next.x1), y1: Math.max(area.y1, next.y1) } : next
  }
  return area
}

/** The page `delta` pages from `page`, kept within the document. */
export function stepPage(info: DocumentInfo, page: number, delta: number) {
  return Math.max(0, Math.min(info.pageCount - 1, page + delta))
}

/** The spread `delta` spreads from the one with `page`, kept within the document. */
export function stepSpread(info: DocumentInfo, page: number, delta: number): Spread | null {
  const at = info.spreads.findIndex((spread) => spread.pages.some((item) => item.index === page))
  if (at < 0) return info.spreads[0] ?? null
  return info.spreads[Math.max(0, Math.min(info.spreads.length - 1, at + delta))] ?? null
}

/** What a view action does: fit an area, set a zoom, or fit the whole pasteboard. */
export type ViewMove = { area: RasterRect; page?: number } | { zoom: number } | { pasteboard: true } | null

/** What `action` does with the document `info`, the page in view `page` (0-based). */
export function viewMove(action: ViewAction, info: DocumentInfo, page: number): ViewMove {
  const spreadArea = (spread: Spread | null): ViewMove => (spread ? { area: spread.area, page: spread.pages[0]?.index ?? page } : null)
  const toPage = (target: number): ViewMove => {
    const area = pageArea(info, target)
    return area ? { area, page: target } : null
  }
  switch (action) {
    case 'fitPage':
      return toPage(page)
    case 'fitSpread':
      return spreadArea(spreadOfPage(info, page))
    case 'actualSize':
      return { zoom: 1 }
    case 'zoom200':
      return { zoom: 2 }
    case 'fitSelection': {
      const area = selectionArea(info)
      return area ? { area } : toPage(page)
    }
    case 'entirePasteboard':
      return { pasteboard: true }
    case 'firstPage':
      return toPage(0)
    case 'previousPage':
      return toPage(stepPage(info, page, -1))
    case 'nextPage':
      return toPage(stepPage(info, page, 1))
    case 'lastPage':
      return toPage(info.pageCount - 1)
    case 'previousSpread':
      return spreadArea(stepSpread(info, page, -1))
    case 'nextSpread':
      return spreadArea(stepSpread(info, page, 1))
  }
}

/** One mouse-wheel notch (or its pixel equivalent) zooms by this much at most. */
export const WHEEL_STEP = 1.2

/**
 * The zoom factor of one wheel event: a notch (100 pixels, or 3 lines) by at most 1.2 times, a
 * trackpad's pinch (small deltas) in proportion.
 */
export function wheelZoomFactor(deltaY: number, deltaMode = 0) {
  const pixels = deltaMode === 1 ? (deltaY * 100) / 3 : deltaMode === 2 ? deltaY * 100 : deltaY
  const notches = Math.max(-1, Math.min(1, -pixels / 100))
  return WHEEL_STEP ** notches
}

/** The view after zooming by `factor` about client point (`px`, `py`) of a canvas at (`left`, `top`): the point stays put. */
export function zoomAbout(view: { zoom: number; x: number; y: number }, factor: number, px: number, py: number, min = 0.05, max = 40) {
  const zoom = Math.max(min, Math.min(max, view.zoom * factor))
  const docX = view.x + px / view.zoom
  const docY = view.y + py / view.zoom
  return { zoom, x: docX - px / zoom, y: docY - py / zoom }
}

/** The topmost object at spread point (`x`, `y`) of `spread` (a group as a whole), skipping hidden and locked ones. */
export function itemAt(info: DocumentInfo, spread: Spread, x: number, y: number) {
  const layers = new Map(info.layers.map((layer) => [layer.id, layer]))
  for (let i = spread.items.length - 1; i >= 0; i -= 1) {
    const item = spread.items[i]
    const layer = layers.get(item.layer)
    if (item.hidden || item.locked || layer?.visible === false || layer?.locked) continue
    const [x0, y0, x1, y1] = item.bounds
    if (x >= Math.min(x0, x1) && x <= Math.max(x0, x1) && y >= Math.min(y0, y1) && y <= Math.max(y0, y1)) return item
  }
  return null
}
