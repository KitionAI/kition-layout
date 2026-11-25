/**
 * Ruler guides by hand (plan 2026-11-25, stage 3): a guide dragged out of a ruler shows its line
 * and where it is while it moves and snaps (Shift to the ruler's ticks, Cmd not at all); a press
 * on a guide with the Selection tools selects it (even over an object) and a drag moves it,
 * dropped on a ruler it is deleted; a selected guide moves with the arrows (Shift ten times as
 * far) and goes with Delete. Locked or hidden guides, Preview and a read-only layout take none of
 * it. The engine draws the guides (`setView` names the selected one and the one being dragged,
 * which this layer draws where it goes).
 */
import { useEffect, useRef, useState, type RefObject } from 'react'
import type { KitCanvasHandle, KitCanvasView } from '@kition/sdk/editor'
import { spreadAt, toDocument, type DocumentInfo, type Spread } from './layout'
import { guideAt, listedGuides, pageUnder, snapGuide, snapTargets, toSpread, type Guide } from './guides'
import { formatLength } from './units'
import type { GuideRef, ViewState } from './viewOptions'

const SURFACE = 'kit-canvas-surface'
/** How far (screen pixels) a press may move and still be a click. */
const CLICK_SLOP = 3

/** Runs a command in the editor's session: its answer as JSON text (null when refused). */
export type Run = (command: string, params: Record<string, unknown>) => Promise<string | null>

/** The guides a person can take on every spread, from the engine's report. */
export function guidesOf(info: DocumentInfo | null): Guide[] {
  return info ? info.spreads.flatMap((spread) => listedGuides(spread.index, spread.guides)) : []
}

/** A guide being dragged: out of a ruler (`from` null) or one already there. */
export type GuideDrag = {
  orientation: Guide['orientation']
  from: Guide | null
  /** The spread under the pointer (an existing guide's own), and the guide's place on it (spread points), snapped. */
  spread: number | null
  position: number | null
  /** The pointer's other coordinate on the spread (spread points), for the page under it. */
  across: number
  /** Over the canvas, over a ruler, and moved past a click. */
  inside: boolean
  onRuler: boolean
  moved: boolean
}

type Options = {
  run: Run
  info: DocumentInfo | null
  guides: readonly Guide[]
  view: ViewState
  canvas: RefObject<KitCanvasHandle | null>
  tool: string
  readOnly: boolean
  selected: GuideRef | null
  onSelect: (guide: GuideRef | null) => void
  onMoving: (guide: GuideRef | null) => void
}

const same = (a: GuideRef | null, b: GuideRef | null) => a !== null && b !== null && a.spread === b.spread && a.page === b.page && a.index === b.index
const ref = (guide: Guide): GuideRef => ({ spread: guide.spread, page: guide.page, index: guide.index })

/** The guide gestures: answers the drag in progress (for `GuideOverlay`) and starts one from a ruler. */
export function useGuideGestures(options: Options) {
  const [drag, setDrag] = useState<GuideDrag | null>(null)
  const latest = useRef(options)
  latest.current = options
  const dragging = useRef<{ drag: GuideDrag; start: { x: number; y: number } } | null>(null)

  /** Where a pointer is: document pixels, and whether over the canvas or a ruler. */
  const locate = (clientX: number, clientY: number) => {
    const surface = document.querySelector<HTMLElement>(`[data-testid="${SURFACE}"]`)
    const view = latest.current.canvas.current?.view()
    if (!surface || !view) return null
    const box = surface.getBoundingClientRect()
    const inside = clientX >= box.left && clientX <= box.right && clientY >= box.top && clientY <= box.bottom
    const under = document.elementFromPoint(clientX, clientY)
    return { x: view.x + (clientX - box.left) / view.zoom, y: view.y + (clientY - box.top) / view.zoom, zoom: view.zoom, inside, onRuler: Boolean(under?.classList.contains('layout-ruler')) }
  }

  /** The drag at the pointer: its spread, snapped place and where the pointer is. */
  const follow = (base: GuideDrag, event: PointerEvent, start: { x: number; y: number }): GuideDrag => {
    const { info, guides, view } = latest.current
    const at = locate(event.clientX, event.clientY)
    const moved = base.moved || Math.hypot(event.clientX - start.x, event.clientY - start.y) > CLICK_SLOP
    if (!info || !at) return { ...base, moved }
    const spread: Spread | null = base.from ? (info.spreads.find((item) => item.index === base.from?.spread) ?? null) : spreadAt(info, at.x, at.y)
    if (!spread) return { ...base, inside: at.inside, onRuler: at.onRuler, moved, spread: null, position: null }
    const point = toSpread(spread, at.x, at.y)
    const vertical = base.orientation === 'vertical'
    const raw = vertical ? point.x : point.y
    const page = pageUnder(spread, point.x, point.y)
    const origin = vertical ? (spread.pages[page ?? 0]?.bounds[0] ?? spread.left) : 0
    // Cmd (Ctrl) holds snapping off; Shift snaps to the ruler's ticks.
    const position = event.metaKey || event.ctrlKey ? raw : snapGuide(raw, view.snapToGuides ? snapTargets(spread, guides, base.orientation, base.from) : [], at.zoom, { shift: event.shiftKey, unit: view.unit, origin })
    return { ...base, spread: spread.index, position, across: vertical ? point.y : point.x, inside: at.inside, onRuler: at.onRuler, moved }
  }

  const begin = (base: GuideDrag, event: { clientX: number; clientY: number; pointerId?: number }) => {
    const start = { x: event.clientX, y: event.clientY }
    dragging.current = { drag: base, start }
    setDrag(base)
    const move = (next: PointerEvent) => {
      const current = dragging.current
      if (!current) return
      next.stopPropagation()
      const was = current.drag.moved
      current.drag = follow(current.drag, next, current.start)
      if (!was && current.drag.moved && current.drag.from) latest.current.onMoving(ref(current.drag.from))
      setDrag(current.drag)
    }
    const up = (next: PointerEvent) => {
      window.removeEventListener('pointermove', move, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', up, true)
      const current = dragging.current
      dragging.current = null
      setDrag(null)
      latest.current.onMoving(null)
      if (!current || next.type === 'pointercancel') return
      next.stopPropagation()
      const done = follow(current.drag, next, current.start)
      void drop(done)
    }
    window.addEventListener('pointermove', move, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', up, true)
  }

  /** What the drop does: add the new guide, move the guide or delete it over a ruler. */
  const drop = async (done: GuideDrag) => {
    const { run, info, onSelect } = latest.current
    const act = (command: string, params: Record<string, unknown>) => run(command, params).catch(() => null)
    if (done.from) {
      if (!done.moved) return
      const target = ref(done.from)
      if (done.onRuler) {
        await act('guide.delete', target)
        onSelect(null)
      } else if (done.inside && done.position !== null && done.spread === done.from.spread) {
        await act('guide.move', { ...target, position: done.position })
      }
      return
    }
    if (!done.inside || done.spread === null || done.position === null || !info) return
    const spread = info.spreads.find((item) => item.index === done.spread)
    if (!spread) return
    const vertical = done.orientation === 'vertical'
    const x = vertical ? done.position : done.across
    const y = vertical ? done.across : done.position
    const page = pageUnder(spread, x, y)
    // Over the pasteboard a guide is the spread's, across it; on a page, the page's.
    await act('guide.add', { orientation: done.orientation, position: done.position, spread: done.spread, at: x, ...(page === null ? { spreadGuide: true } : { page }) })
  }

  // A press on a guide with a Selection tool takes it, before the canvas (even over an object).
  useEffect(() => {
    const press = (event: PointerEvent) => {
      const { info, guides, view, tool, readOnly, selected, onSelect, canvas } = latest.current
      const target = event.target
      if (!(target instanceof HTMLElement) || target.getAttribute('data-testid') !== SURFACE || event.button !== 0 || !info) return
      const takes = (tool === 'selection' || tool === 'directSelection') && view.guides && !view.lockGuides && view.screenMode === 'normal' && !readOnly
      const zoom = canvas.current?.view()?.zoom ?? 1
      const at = takes ? locate(event.clientX, event.clientY) : null
      const hit = at ? guideAt(info, guides, at.x, at.y, zoom) : null
      if (!hit) {
        if (selected) onSelect(null)
        return
      }
      event.preventDefault()
      event.stopPropagation()
      onSelect(ref(hit))
      begin({ orientation: hit.orientation, from: hit, spread: hit.spread, position: hit.position, across: 0, inside: true, onRuler: false, moved: false }, event)
    }
    window.addEventListener('pointerdown', press, true)
    return () => window.removeEventListener('pointerdown', press, true)
    // `begin` and `locate` read `latest`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // The selected guide leaves the selection when it is no longer there (undone, deleted).
  useEffect(() => {
    const { selected, onSelect } = latest.current
    if (selected && !options.guides.some((guide) => same(ref(guide), selected))) onSelect(null)
  }, [options.guides])

  /** A ruler's press: a new guide, horizontal from the top ruler, vertical from the left one. */
  const fromRuler = (axis: 'x' | 'y', event: { clientX: number; clientY: number; button: number }) => {
    const { view, readOnly } = latest.current
    if (event.button !== 0 || readOnly || !view.guides || view.lockGuides) return
    begin({ orientation: axis === 'x' ? 'horizontal' : 'vertical', from: null, spread: null, position: null, across: 0, inside: false, onRuler: true, moved: false }, event)
  }

  /** The keys of a selected guide: arrows move it (Shift ten points), Delete deletes it, Escape lets it go. */
  const guideKey = (event: { key: string; shiftKey: boolean; metaKey: boolean; ctrlKey: boolean; altKey: boolean }) => {
    const { selected, guides, onSelect, run, readOnly } = latest.current
    if (!selected || event.metaKey || event.ctrlKey || event.altKey) return false
    const guide = guides.find((item) => same(ref(item), selected))
    if (!guide) return false
    if (event.key === 'Escape') {
      onSelect(null)
      return true
    }
    if (readOnly) return false
    if (event.key === 'Delete' || event.key === 'Backspace') {
      void run('guide.delete', selected).catch(() => null)
      onSelect(null)
      return true
    }
    const step = event.shiftKey ? 10 : 1
    const delta = guide.orientation === 'vertical' ? { ArrowLeft: -step, ArrowRight: step }[event.key] : { ArrowUp: -step, ArrowDown: step }[event.key]
    if (delta === undefined) return ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)
    void run('guide.move', { ...selected, position: guide.position + delta }).catch(() => null)
    return true
  }

  return { drag, fromRuler, guideKey }
}

/** The guide being dragged and where it is (`X: 25 mm`), drawn over the canvas (its children). */
export function GuideOverlay({ drag, info, view, unit }: { drag: GuideDrag | null; info: DocumentInfo | null; view: KitCanvasView | null; unit: ViewState['unit'] }) {
  if (!drag || !info || !view || drag.spread === null || drag.position === null || !drag.inside || (drag.from && !drag.moved)) return null
  const spread = info.spreads.find((item) => item.index === drag.spread)
  if (!spread) return null
  const vertical = drag.orientation === 'vertical'
  const at = vertical ? toDocument(spread, drag.position, 0).x : toDocument(spread, 0, drag.position).y
  const screen = vertical ? (at - view.x) * view.zoom : (at - view.y) * view.zoom
  const page = pageUnder(spread, vertical ? drag.position : drag.across, vertical ? drag.across : drag.position)
  const origin = vertical ? (spread.pages[page ?? 0]?.bounds[0] ?? spread.left) : 0
  const label = `${vertical ? 'X' : 'Y'}: ${formatLength(drag.position - origin, unit)}`
  return (
    <div className="layout-guide-drag" data-testid="layout-guide-drag" data-orientation={drag.orientation} data-position={Math.round(drag.position * 1000) / 1000}>
      <div className="layout-guide-drag-line" style={vertical ? { left: screen, top: 0, bottom: 0, width: 1 } : { top: screen, left: 0, right: 0, height: 1 }} />
      <div className="layout-guide-drag-label" style={vertical ? { left: screen + 6, top: 6 } : { top: screen + 6, left: 6 }} data-testid="layout-guide-readout">
        {label}
      </div>
    </div>
  )
}
