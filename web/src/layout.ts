import type { KitEngineInfo, RasterRect } from '@kition/kit-editor'

/** A page in spread coordinates (points), as the engine reports it. */
export type Page = {
  index: number
  name: string
  bounds: [number, number, number, number]
  parent: string | null
  margins: { top: number; bottom: number; inside: number; outside: number } | null
  columns: number
  /** The swatch of the page's background (a locked rectangle on its parent), if any. */
  background: string | null
}
/** An object on a spread; groups hold their children. */
export type Item = {
  id: number
  name: string
  kind: string
  layer: number
  bounds: [number, number, number, number]
  fill: string
  stroke: { swatch: string; weight: number }
  locked: boolean
  hidden: boolean
  story?: number
  asset?: number
  children?: Item[]
}
export type Spread = {
  index: number
  /** Where the spread sits on the canvas, in document pixels. */
  area: RasterRect
  pages: Page[]
  items: Item[]
  /** Spread x of the area's left edge (its leftmost page). */
  left: number
}
export type Layer = { id: number; name: string; visible: boolean; locked: boolean; color: number[] }
export type Story = { id: number; frames: number[]; overset: boolean; preview: string; length: number }
export type SwatchInfo = { name: string; kind: string; color: string | null; locked: boolean }
export type ParentInfo = { id: number; label: string | null; items: number }
/** The original layout (`name` null) and each alternate layout: a size of the document. */
export type LayoutInfo = { name: string | null; firstPage: number; pages: number; width: number; height: number }

/** What the editor reads from the engine's `inspect()`. */
export type DocumentInfo = {
  spreads: Spread[]
  pageCount: number
  selected: number
  selection: number[]
  editingText: boolean
  layers: Layer[]
  activeLayer: number | null
  swatches: string[]
  swatchColors: SwatchInfo[]
  paragraphStyles: string[]
  characterStyles: string[]
  stories: Story[]
  parents: ParentInfo[]
  layouts: LayoutInfo[]
  settings: { pageWidth: number; pageHeight: number; facingPages: boolean; intent: string }
  tool: string
}

type Inspected = {
  pageCount?: number
  spreads?: Array<{ index?: number; pages?: Array<Partial<Page>>; items?: Item[] }>
  canvas?: { spreads?: Array<{ x: number; y: number; width: number; height: number }> }
  selection?: { items?: number[]; text?: unknown }
  layers?: Layer[]
  activeLayer?: number
  swatches?: string[]
  swatchColors?: SwatchInfo[]
  paragraphStyles?: string[]
  characterStyles?: string[]
  stories?: Story[]
  parents?: ParentInfo[]
  layouts?: LayoutInfo[]
  settings?: Partial<DocumentInfo['settings']>
  tool?: string
}

const NO_BOUNDS: [number, number, number, number] = [0, 0, 0, 0]

export function describe(info: KitEngineInfo): DocumentInfo {
  const inspected = info.inspected as Inspected
  const placed = inspected.canvas?.spreads ?? []
  return {
    spreads: (inspected.spreads ?? []).map((spread, index) => {
      const at = placed[index] ?? { x: 0, y: 0, width: info.width, height: info.height }
      const pages: Page[] = (spread.pages ?? []).map((page, pageIndex) => ({
        index: page.index ?? pageIndex,
        name: page.name ?? '',
        bounds: page.bounds ?? NO_BOUNDS,
        parent: page.parent ?? null,
        margins: page.margins ?? null,
        columns: page.columns ?? 1,
        background: page.background ?? null,
      }))
      return {
        index: spread.index ?? index,
        area: { x0: at.x, y0: at.y, x1: at.x + at.width, y1: at.y + at.height },
        pages,
        items: spread.items ?? [],
        left: pages.length ? Math.min(...pages.map((page) => page.bounds[0])) : 0,
      }
    }),
    pageCount: inspected.pageCount ?? 0,
    selected: inspected.selection?.items?.length ?? 0,
    selection: inspected.selection?.items ?? [],
    editingText: inspected.selection?.text != null,
    layers: inspected.layers ?? [],
    activeLayer: inspected.activeLayer ?? null,
    swatches: inspected.swatches ?? [],
    swatchColors: inspected.swatchColors ?? (inspected.swatches ?? []).map((name) => ({ name, kind: 'color', color: null, locked: false })),
    paragraphStyles: inspected.paragraphStyles ?? [],
    characterStyles: inspected.characterStyles ?? [],
    stories: inspected.stories ?? [],
    parents: inspected.parents ?? [],
    layouts: inspected.layouts ?? [],
    settings: { pageWidth: 612, pageHeight: 792, facingPages: false, intent: 'print', ...inspected.settings },
    tool: inspected.tool ?? 'selection',
  }
}

/** Spread point (points) to document pixels. */
export function toDocument(spread: Spread, x: number, y: number) {
  return { x: spread.area.x0 + x - spread.left, y: spread.area.y0 + y }
}

/** The spread whose canvas area holds document pixel (`x`, `y`), else the nearest one. */
export function spreadAt(info: DocumentInfo, x: number, y: number): Spread | null {
  let best: Spread | null = null
  let distance = Number.POSITIVE_INFINITY
  for (const spread of info.spreads) {
    const { x0, y0, x1, y1 } = spread.area
    const dx = x < x0 ? x0 - x : x > x1 ? x - x1 : 0
    const dy = y < y0 ? y0 - y : y > y1 ? y - y1 : 0
    const d = Math.hypot(dx, dy)
    if (d < distance) {
      distance = d
      best = spread
    }
  }
  return best
}

/** Every item of a spread, front first, with its depth in groups (for the Layers panel). */
export function flatten(items: readonly Item[], depth = 0, out: Array<{ item: Item; depth: number }> = []) {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i]
    out.push({ item, depth })
    if (item.children?.length) flatten(item.children, depth + 1, out)
  }
  return out
}

/** The item with `id` in the document, searching groups. */
export function findItem(info: DocumentInfo, id: number): { item: Item; spread: Spread } | null {
  const search = (items: readonly Item[]): Item | null => {
    for (const item of items) {
      if (item.id === id) return item
      const inner = item.children ? search(item.children) : null
      if (inner) return inner
    }
    return null
  }
  for (const spread of info.spreads) {
    const item = search(spread.items)
    if (item) return { item, spread }
  }
  return null
}

export const MENU_ORDER = ['File', 'Edit', 'Layout', 'Type', 'Object', 'Table', 'View', 'Window', 'Help']

/** The engine commands each menu tab shows in Single line's row (InDesign's most-used), by menu. */
export const LAYOUT_PRIMARY: Record<string, readonly string[]> = {
  File: ['file.package'],
  Edit: ['edit.undo', 'edit.redo', 'edit.cut', 'edit.copy', 'edit.paste', 'edit.duplicate', 'edit.stepAndRepeat', 'find.find'],
  Layout: ['layout.pages.insert', 'layout.parents.new', 'layout.marginsAndColumns', 'layout.createGuides', 'toc.generate', 'layout.section'],
  Type: ['footnote.insert', 'type.createOutlines', 'type.changeCase', 'hyperlink.create', 'list.define', 'type.fillWithPlaceholder'],
  Object: ['object.group', 'object.ungroup', 'object.bringForward', 'object.sendBackward', 'object.fit', 'object.textFrameOptions', 'object.dropShadow', 'object.lock'],
  Table: ['table.insertRowBelow', 'table.insertColumnRight', 'table.deleteRow', 'table.deleteColumn', 'table.merge', 'table.unmerge', 'table.convertFromText'],
  View: ['guide.deleteAll', 'view.showTextThreads'],
  Window: ['object.textWrap', 'preflight.run', 'article.new'],
}
/** Tools that need panels this editor does not have yet. */
export const HIDDEN_TOOLS = ['note', 'colorTheme', 'contentCollector', 'contentPlacer']
