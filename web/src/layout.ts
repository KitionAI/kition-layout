import type { KitEngineInfo, RasterRect } from '@kition/sdk/editor'

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
  /** A text frame's columns. */
  columns?: number
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
  /** Its ruler guides a person can take (shown, unlocked), as the engine reports them. */
  guides: unknown[]
}
export type Layer = { id: number; name: string; visible: boolean; locked: boolean; color: number[] }
/** A story; `words` and `oversetChars` (the characters that do not fit) for the status bar. */
export type Story = { id: number; frames: number[]; overset: boolean; preview: string; length: number; words?: number; oversetChars?: number }
/** A swatch's colour (`inspect().colors`, unnamed ones too): as the screen shows it, and its CMYK percentages when defined so. */
export type ColorValue = { hex?: string; cmyk?: number[] }
export type SwatchInfo = { name: string; kind: string; color: string | null; locked: boolean }
export type ParentInfo = { id: number; label: string | null; items: number }
/** The original layout (`name` null) and each alternate layout: a size of the document. */
export type LayoutInfo = { name: string | null; firstPage: number; pages: number; width: number; height: number }

/** What the editor reads from the engine's `inspect()`. */
export type DocumentInfo = {
  /** The canvas in document pixels: the spreads and the engine's margin of pasteboard around them. Scrolling stops at it. */
  canvas: RasterRect
  spreads: Spread[]
  pageCount: number
  selected: number
  selection: number[]
  editingText: boolean
  /** The story the text cursor or text selection is in (null when not editing text). */
  textStory: number | null
  layers: Layer[]
  activeLayer: number | null
  swatches: string[]
  swatchColors: SwatchInfo[]
  /** Every swatch's colour by name, the unnamed (hidden) ones too. */
  colors: Record<string, ColorValue>
  paragraphStyles: string[]
  characterStyles: string[]
  stories: Story[]
  parents: ParentInfo[]
  layouts: LayoutInfo[]
  settings: { pageWidth: number; pageHeight: number; facingPages: boolean; intent: string }
  tool: string
  /** The selected frame's placed graphic is selected, not the frame (plan stage 6). */
  selectionContent: boolean
  /** The key object Align aligns to (one of the selected objects), if any. */
  keyObject: number | null
  /** The graphics in the place cursor, the one a click places first. */
  placeQueue: PlaceQueued[]
}

/** A graphic waiting in the place cursor. */
export type PlaceQueued = { asset: number; name: string; width: number; height: number }

type Inspected = {
  pageCount?: number
  spreads?: Array<{ index?: number; pages?: Array<Partial<Page>>; items?: Item[] }>
  canvas?: { spreads?: Array<{ x: number; y: number; width: number; height: number; guides?: unknown[] }> }
  selection?: { items?: number[]; text?: { story?: number } | null }
  layers?: Layer[]
  activeLayer?: number
  swatches?: string[]
  swatchColors?: SwatchInfo[]
  colors?: Record<string, ColorValue>
  paragraphStyles?: string[]
  characterStyles?: string[]
  stories?: Story[]
  parents?: ParentInfo[]
  layouts?: LayoutInfo[]
  settings?: Partial<DocumentInfo['settings']>
  tool?: string
  selectionContent?: boolean
  keyObject?: number | null
  placeQueue?: PlaceQueued[]
}

const NO_BOUNDS: [number, number, number, number] = [0, 0, 0, 0]

export function describe(info: KitEngineInfo): DocumentInfo {
  const inspected = info.inspected as Inspected
  const placed = inspected.canvas?.spreads ?? []
  return {
    canvas: { x0: 0, y0: 0, x1: info.width, y1: info.height },
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
        guides: placed[index]?.guides ?? [],
      }
    }),
    pageCount: inspected.pageCount ?? 0,
    selected: inspected.selection?.items?.length ?? 0,
    selection: inspected.selection?.items ?? [],
    editingText: inspected.selection?.text != null,
    textStory: inspected.selection?.text?.story ?? null,
    layers: inspected.layers ?? [],
    activeLayer: inspected.activeLayer ?? null,
    swatches: inspected.swatches ?? [],
    swatchColors: inspected.swatchColors ?? (inspected.swatches ?? []).map((name) => ({ name, kind: 'color', color: null, locked: false })),
    colors: inspected.colors ?? {},
    paragraphStyles: inspected.paragraphStyles ?? [],
    characterStyles: inspected.characterStyles ?? [],
    stories: inspected.stories ?? [],
    parents: inspected.parents ?? [],
    layouts: inspected.layouts ?? [],
    settings: { pageWidth: 612, pageHeight: 792, facingPages: false, intent: 'print', ...inspected.settings },
    tool: inspected.tool ?? 'selection',
    selectionContent: inspected.selectionContent ?? false,
    keyObject: inspected.keyObject ?? null,
    placeQueue: inspected.placeQueue ?? [],
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

/**
 * Tools the rail leaves out: those that need panels this editor does not have yet, and the empty
 * ones (Free Transform, Measure and Vertical Type on a Path do nothing in the engine yet).
 */
export const HIDDEN_TOOLS = ['note', 'colorTheme', 'contentCollector', 'contentPlacer', 'freeTransform', 'measure', 'verticalTypeOnPath']
