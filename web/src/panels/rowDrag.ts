import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'

/** Moves past this many pixels make a press a drag (less is a click). */
const THRESHOLD = 4

/** Within this many pixels of a scrolling list's top or bottom edge, a drag scrolls the list. */
export const AUTO_SCROLL_EDGE = 28

/** How far a list scrolls per frame while a drag is at its edge: faster nearer (and past) the edge. */
export function autoScrollStep(y: number, top: number, bottom: number, edge = AUTO_SCROLL_EDGE) {
  if (bottom - top < edge * 3) return 0
  if (y < top + edge) return -Math.ceil(Math.min(edge, top + edge - y) / 2)
  if (y > bottom - edge) return Math.ceil(Math.min(edge, y - (bottom - edge)) / 2)
  return 0
}

/** The nearest element around `element` that scrolls up and down. */
function scroller(element: Element | null) {
  for (let at = element; at; at = at.parentElement) {
    const style = getComputedStyle(at)
    if (/(auto|scroll)/.test(style.overflowY) && at.scrollHeight > at.clientHeight) return at as HTMLElement
  }
  return null
}

/** Where a drag is over: the drop target element (marked `data-drop`) and the pointer. */
export type DragOver = { target: HTMLElement | null; x: number; y: number }

/**
 * Dragging rows of a panel with the pointer (plan 2026-11-25, stage 7): a press on a row and a
 * move past a few pixels drags it; the element under the pointer with `data-drop` is the target
 * (marked `data-drop-over`), and the release drops there (`onDrop`), swallowing the click that
 * would follow. Pointer events, not the browser's drag and drop, so every browser and a test's
 * mouse drive it alike. `dragging` is the source while a drag is on (the panel marks it). Held
 * near the top or bottom of the list it scrolls the list (phase 9: a short Layers panel showed two
 * rows, and a row could not reach a layer out of view).
 */
export function useRowDrag<T>(onDrop: (source: T, over: DragOver) => void) {
  const [dragging, setDragging] = useState<T | null>(null)
  const latest = useRef(onDrop)
  latest.current = onDrop
  const cleanup = useRef<(() => void) | null>(null)
  useEffect(() => () => cleanup.current?.(), [])
  const start = (source: T, event: ReactPointerEvent) => {
    if (event.button !== 0 || event.shiftKey || event.metaKey || event.ctrlKey) return
    cleanup.current?.()
    const x0 = event.clientX
    const y0 = event.clientY
    let on = false
    let marked: HTMLElement | null = null
    const list = scroller(event.currentTarget instanceof Element ? event.currentTarget : null)
    let pointer = { x: x0, y: y0 }
    let frame = 0
    const targetAt = (x: number, y: number) => {
      const found = document.elementFromPoint(x, y)
      return found instanceof Element ? (found.closest('[data-drop]') as HTMLElement | null) : null
    }
    const mark = (next: HTMLElement | null) => {
      if (marked === next) return
      marked?.removeAttribute('data-drop-over')
      marked = next
      marked?.setAttribute('data-drop-over', 'true')
    }
    // While the pointer is at the list's edge, the list scrolls and the target under it follows.
    const scroll = () => {
      frame = 0
      if (!on || !list) return
      const box = list.getBoundingClientRect()
      const step = autoScrollStep(pointer.y, box.top, box.bottom)
      if (!step) return
      const before = list.scrollTop
      list.scrollTop = before + step
      if (list.scrollTop === before) return
      mark(targetAt(pointer.x, pointer.y))
      frame = window.requestAnimationFrame(scroll)
    }
    const move = (moved: PointerEvent) => {
      if (!on && Math.hypot(moved.clientX - x0, moved.clientY - y0) < THRESHOLD) return
      if (!on) {
        on = true
        setDragging(source)
      }
      pointer = { x: moved.clientX, y: moved.clientY }
      mark(targetAt(moved.clientX, moved.clientY))
      if (!frame) frame = window.requestAnimationFrame(scroll)
    }
    const swallow = (click: MouseEvent) => {
      click.stopPropagation()
      click.preventDefault()
    }
    const done = () => {
      window.removeEventListener('pointermove', move, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', cancel, true)
      mark(null)
      window.cancelAnimationFrame(frame)
      frame = 0
      cleanup.current = null
    }
    const cancel = () => {
      done()
      setDragging(null)
    }
    const up = (released: PointerEvent) => {
      const dropped = on
      done()
      if (!dropped) return
      setDragging(null)
      // The click the release makes is the drop's, not the row's.
      window.addEventListener('click', swallow, { capture: true, once: true })
      window.setTimeout(() => window.removeEventListener('click', swallow, true), 0)
      latest.current(source, { target: targetAt(released.clientX, released.clientY), x: released.clientX, y: released.clientY })
    }
    window.addEventListener('pointermove', move, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', cancel, true)
    cleanup.current = cancel
  }
  return { start, dragging }
}
