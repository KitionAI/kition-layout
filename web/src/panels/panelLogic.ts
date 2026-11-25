/**
 * The Properties panel's rules that need no browser (plan 2026-11-25, stage 4), tested in Node:
 * the reference point, the fixed-value buttons (the menus' variant rows), the stroke types and
 * arrowheads the engine draws, the languages text can be set in, and which layer new objects go on.
 */

/** The reference point's place in the box, 0 (top left) to 8 (bottom right), as `transform.set`'s `ref`. */
export const REF_POINTS = [0, 1, 2, 3, 4, 5, 6, 7, 8] as const
export type RefPoint = (typeof REF_POINTS)[number]

/** How far across and down the box the reference point is (0, ½ or 1). */
export function refOffset(ref: number): readonly [number, number] {
  const at = Math.max(0, Math.min(8, Math.round(ref)))
  return [[0, 0.5, 1][at % 3], [0, 0.5, 1][Math.floor(at / 3)]]
}

/** The box `transform.info` measures (top left, page-relative) as X and Y read at the reference point. */
export function shownPosition(box: { x: number; y: number; width: number; height: number }, ref: number) {
  const [fx, fy] = refOffset(ref)
  return { x: box.x + box.width * fx, y: box.y + box.height * fy }
}

/** The other side when the proportions are kept: a new width's height, or a new height's width. */
export function keptProportion(box: { width: number; height: number }, changed: 'width' | 'height', value: number) {
  if (!(box.width > 0) || !(box.height > 0)) return null
  return changed === 'width' ? (value * box.height) / box.width : (value * box.width) / box.height
}

/** The panel's fixed-value buttons: catalog rows (`menus/catalog.ts` `VARIANTS`) run through the dialog host. */
export const ALIGN_ROWS = ['object.align|left', 'object.align|hcenter', 'object.align|right', 'object.align|top', 'object.align|vcenter', 'object.align|bottom'] as const
export const DISTRIBUTE_ROWS = ['object.distribute|horizontalCenters', 'object.distribute|verticalCenters', 'object.distribute|horizontalSpace', 'object.distribute|verticalSpace'] as const
export const WRAP_ROWS = ['object.textWrap|none', 'object.textWrap|boundingBox', 'object.textWrap|contour', 'object.textWrap|jumpObject', 'object.textWrap|jumpToNextColumn'] as const
export const PATHFINDER_ROWS = ['object.pathfinder|add', 'object.pathfinder|subtract', 'object.pathfinder|intersect', 'object.pathfinder|exclude', 'object.pathfinder|minusBack'] as const
export const FIT_ROWS = ['object.fit|fillProportionally', 'object.fit|fitProportionally', 'object.fit|fitContentToFrame', 'object.fit|fitFrameToContent', 'object.fit|centerContent'] as const
export const PANEL_ROWS: readonly string[] = [...ALIGN_ROWS, ...DISTRIBUTE_ROWS, ...WRAP_ROWS, ...PATHFINDER_ROWS, ...FIT_ROWS]

/** What Align aligns to (`object.align`'s `to`; the key object needs a way to pick one, not built). */
export const ALIGN_TO = ['selection', 'margins', 'page', 'spread'] as const
export type AlignTo = (typeof ALIGN_TO)[number]

/** The stroke types the engine draws (`object.stroke`'s `type.kind`), as the Stroke panel lists them. */
export const STROKE_TYPES = ['solid', 'dashed', 'dotted', 'thickThin', 'thinThick', 'thinThin', 'thickThick', 'thinThickThin', 'thickThinThick', 'wavy', 'hashed'] as const

/** `object.stroke`'s `type` for a kind: a dash is three weights long with a gap of two. */
export function strokeType(kind: string, weight: number): Record<string, unknown> {
  if (kind !== 'dashed') return { kind }
  const w = Math.max(1, weight)
  return { kind, pattern: [3 * w, 2 * w] }
}

export const ARROWHEADS = ['none', 'simple', 'simpleWide', 'triangle', 'triangleWide', 'barbed', 'curved', 'circle', 'circleSolid', 'square', 'squareSolid', 'bar'] as const
export const CORNER_SHAPES = ['none', 'rounded', 'inverseRounded', 'inset', 'bevel', 'fancy'] as const

/** Paragraph alignment (`text.para`'s `align`), in the Paragraph panel's order. */
export const PARAGRAPH_ALIGNS = ['left', 'center', 'right', 'leftJustified', 'centerJustified', 'rightJustified', 'fullyJustified'] as const

/** Languages a person sets text in, by the names the engine reads (hyphenation, spelling, quotes). */
export const LANGUAGES = [
  'English: USA',
  'English: UK',
  'English: Canadian',
  'German: 2006 Reform',
  'German: Swiss 2006 Reform',
  'French',
  'French: Canadian',
  'Spanish: Castilian',
  'Italian',
  'Portuguese',
  'Portuguese: Brazilian',
  'Dutch: 2005 Reform',
  'Danish',
  'Swedish',
  'Norwegian: Bokmål',
  'Finnish',
  'Polish',
  'Czech',
  'Hungarian',
  'Russian',
  'Ukrainian',
  'Greek',
  'Turkish',
  'Chinese: Simplified',
  'Chinese: Traditional',
  'Japanese',
  'Korean',
  'Arabic',
  'Hebrew',
] as const

type LayerLike = { id: number; visible: boolean; locked: boolean }

/**
 * The layer new objects should go on (plan 2026-11-25, 0b): the layer of the selected objects
 * when they share one that takes objects; with nothing selected, when the active layer is empty
 * and another shown, unlocked layer has objects, the topmost of those (a document opened on an
 * empty notes layer). Null when the active layer stays. `layers` are front first; `counts` is
 * the objects on each layer.
 */
export function layerToActivate(layers: readonly LayerLike[], counts: ReadonlyMap<number, number>, active: number | null, selectedLayers: readonly number[]): number | null {
  const takes = (layer: LayerLike | undefined) => layer != null && layer.visible && !layer.locked
  if (selectedLayers.length) {
    const first = selectedLayers[0]
    if (selectedLayers.some((layer) => layer !== first) || first === active) return null
    return takes(layers.find((layer) => layer.id === first)) ? first : null
  }
  if (active != null && (counts.get(active) ?? 0) > 0) return null
  const filled = layers.find((layer) => takes(layer) && (counts.get(layer.id) ?? 0) > 0)
  return filled && filled.id !== active ? filled.id : null
}

type Painted = { id: number; fill: string; fillTint?: number; stroke: { swatch: string; weight: number }; strokeDetail?: { tint?: number } }

/**
 * X (InDesign's Swap Fill and Stroke; the engine has no command for it): each object's fill takes
 * its stroke's colour and tint and the stroke its fill's, objects alike in one pair of commands.
 */
export function swapFillStroke(items: readonly Painted[]): Array<[string, Record<string, unknown>]> {
  const groups = new Map<string, { item: Painted; ids: number[] }>()
  for (const item of items) {
    const key = JSON.stringify([item.fill, item.fillTint ?? 1, item.stroke.swatch, item.strokeDetail?.tint ?? 1])
    const group = groups.get(key) ?? { item, ids: [] }
    group.ids.push(item.id)
    groups.set(key, group)
  }
  return [...groups.values()].flatMap(({ item, ids }) => [
    ['object.fill', { swatch: item.stroke.swatch, tint: item.strokeDetail?.tint ?? 1, ids }],
    ['object.stroke', { swatch: item.fill, tint: item.fillTint ?? 1, ids }],
  ] as Array<[string, Record<string, unknown>]>)
}

/** D (Default Fill and Stroke): no fill and a black 1 pt stroke, as new shapes have. */
export function defaultFillStroke(ids: readonly number[]): Array<[string, Record<string, unknown>]> {
  return [
    ['object.fill', { swatch: '[None]', ids: [...ids] }],
    ['object.stroke', { swatch: '[Black]', tint: 1, weight: 1, ids: [...ids] }],
  ]
}
