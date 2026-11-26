/**
 * Dragging rows in the Pages and Layers panels (plan 2026-11-25, stage 7): where a drop puts a
 * page or a layer (`layout.pages.move`, `layer.move`: taken out, then put in at `to`), and the
 * place `arrange.reorder` takes for objects dropped among a layer's objects (`index` counts that
 * layer's other objects from the front). Pure, so it is tested in Node.
 */

/** Where a page or layer at `from` goes when dropped before the one at `before` (the count: at the end); null when it stays. */
export function moveTarget(from: number, before: number): number | null {
  const to = before > from ? before - 1 : before
  return to === from ? null : to
}

/** A drop target's half the pointer is over: before (top or left half) or after it. */
export function dropSide(rect: { left: number; top: number; width: number; height: number }, x: number, y: number, across: boolean): 'before' | 'after' {
  return across ? (x < rect.left + rect.width / 2 ? 'before' : 'after') : y < rect.top + rect.height / 2 ? 'before' : 'after'
}

/**
 * `arrange.reorder`'s `index` for objects `moving` dropped on a layer whose top-level objects on
 * the spread are `front` (front first): before the object `before` (null: in front of them all);
 * the objects being moved are not counted.
 */
export function reorderIndex(front: readonly number[], moving: readonly number[], before: number | null): number {
  const others = front.filter((id) => !moving.includes(id))
  if (before == null) return 0
  const at = others.indexOf(before)
  return at < 0 ? others.length : at
}

/** InDesign's layer colours, as the engine names them (a new layer takes the next). */
export const LAYER_COLORS: ReadonlyArray<readonly [string, readonly [number, number, number]]> = [
  ['Light Blue', [43, 155, 255]],
  ['Red', [255, 0, 0]],
  ['Green', [79, 255, 79]],
  ['Blue', [0, 0, 255]],
  ['Yellow', [255, 255, 79]],
  ['Magenta', [255, 79, 255]],
  ['Cyan', [0, 255, 255]],
  ['Gray', [128, 128, 128]],
  ['Black', [0, 0, 0]],
  ['Orange', [255, 102, 0]],
  ['Dark Green', [0, 84, 0]],
  ['Teal', [0, 153, 153]],
  ['Tan', [204, 153, 102]],
  ['Brown', [153, 51, 0]],
  ['Violet', [153, 51, 255]],
  ['Gold', [255, 153, 0]],
]

/** Layer Options' values. */
export type LayerDraft = { name: string; color: readonly number[]; visible: boolean; locked: boolean; printable: boolean }

/** The `layer.set` parameters for what changed (null: nothing did). */
export function layerEdit(id: number, from: LayerDraft, to: LayerDraft): Record<string, unknown> | null {
  const params: Record<string, unknown> = { id }
  const name = to.name.trim()
  if (name && name !== from.name) params.name = name
  if (to.color.join(',') !== from.color.join(',')) params.color = [...to.color]
  for (const key of ['visible', 'locked', 'printable'] as const) if (to[key] !== from[key]) params[key] = to[key]
  return Object.keys(params).length > 1 ? params : null
}
