import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import type { KitCanvasView } from '@kition/kit-editor'
import { spreadAt, type DocumentInfo, type Spread } from './layout'

/** Ruler thickness in CSS pixels. */
export const RULER = 18
/** Major tick steps in points: the first at least `MIN_GAP` screen pixels apart is used. */
const STEPS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000, 10000]
const MIN_GAP = 56

/** The major step for `zoom` screen pixels per point. */
export function rulerStep(zoom: number) {
  return STEPS.find((step) => step * zoom >= MIN_GAP) ?? STEPS[STEPS.length - 1]
}

/** Where the rulers count from: the top-left of the page under the middle of the view, in document pixels. */
export function rulerOrigin(info: DocumentInfo, view: KitCanvasView): { x: number; y: number; spread: Spread } | null {
  const cx = view.x + view.width / 2 / view.zoom
  const cy = view.y + view.height / 2 / view.zoom
  const spread = spreadAt(info, cx, cy)
  if (!spread) return null
  const local = cx - spread.area.x0 + spread.left
  const page = spread.pages.find((item) => local >= item.bounds[0] && local <= item.bounds[2]) ?? spread.pages[0]
  return { x: spread.area.x0 + (page ? page.bounds[0] : spread.left) - spread.left, y: spread.area.y0, spread }
}

function useCssColor(name: string, fallback: string) {
  const [color, setColor] = useState(fallback)
  useEffect(() => {
    const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
    if (value) setColor(value)
  }, [name])
  return color
}

/**
 * One ruler along the canvas (InDesign's: points from the top-left of the page in view; ticks
 * every major step, four minor ones between). Dragging from it onto the canvas makes a guide
 * there (`guide.add`). Its origin and scale are on `data-origin` and `data-zoom`.
 */
export function Ruler({ axis, info, view, onGuide }: { axis: 'x' | 'y'; info: DocumentInfo; view: KitCanvasView | null; onGuide: (orientation: 'horizontal' | 'vertical', position: number, spread: number) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  const text = useCssColor('--kit-text-muted', '#6b6b76')
  const line = useCssColor('--kit-border', '#d9d9e0')
  const surface = useCssColor('--kit-surface', '#ffffff')
  const origin = view ? rulerOrigin(info, view) : null
  const at = view && origin ? (axis === 'x' ? (origin.x - view.x) * view.zoom : (origin.y - view.y) * view.zoom) : 0

  useEffect(() => {
    const element = canvas.current
    if (!element || typeof ResizeObserver === 'undefined') return undefined
    const observer = new ResizeObserver(([entry]) => setSize({ width: entry.contentRect.width, height: entry.contentRect.height }))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    const element = canvas.current
    const context = element?.getContext('2d')
    if (!element || !context || !view || !size.width || !size.height) return
    const scale = window.devicePixelRatio || 1
    element.width = Math.round(size.width * scale)
    element.height = Math.round(size.height * scale)
    context.setTransform(scale, 0, 0, scale, 0, 0)
    context.fillStyle = surface
    context.fillRect(0, 0, size.width, size.height)
    context.strokeStyle = line
    context.fillStyle = text
    context.lineWidth = 1
    context.font = '9px system-ui, sans-serif'
    context.beginPath()
    if (axis === 'x') {
      context.moveTo(0, size.height - 0.5)
      context.lineTo(size.width, size.height - 0.5)
    } else {
      context.moveTo(size.width - 0.5, 0)
      context.lineTo(size.width - 0.5, size.height)
    }
    const length = axis === 'x' ? size.width : size.height
    const step = rulerStep(view.zoom)
    const minor = step / 5
    const firstValue = Math.floor(-at / view.zoom / minor) * minor
    for (let value = firstValue; (value * view.zoom + at) <= length; value += minor) {
      const position = Math.round(value * view.zoom + at) + 0.5
      const major = Math.abs(value / step - Math.round(value / step)) < 1e-6
      const tick = major ? RULER : RULER * 0.3
      if (axis === 'x') {
        context.moveTo(position, RULER - tick)
        context.lineTo(position, RULER)
        if (major) context.fillText(String(Math.round(value)), position + 2, 9)
      } else {
        context.moveTo(RULER - tick, position)
        context.lineTo(RULER, position)
        if (major) {
          context.save()
          context.translate(9, position + 2)
          context.rotate(-Math.PI / 2)
          context.textAlign = 'right'
          context.fillText(String(Math.round(value)), 0, 0)
          context.restore()
        }
      }
    }
    context.stroke()
  }, [at, axis, line, size, surface, text, view])

  // Drag a guide out of the ruler: released over the canvas, it lands under the pointer.
  const drag = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0 || !view) return
    const ruler = event.currentTarget.getBoundingClientRect()
    const up = (done: PointerEvent) => {
      window.removeEventListener('pointerup', up)
      // Canvas left/top: the ruler's own far edge.
      const canvasLeft = axis === 'x' ? ruler.left : ruler.right
      const canvasTop = axis === 'x' ? ruler.bottom : ruler.top
      const dx = (done.clientX - canvasLeft) / view.zoom
      const dy = (done.clientY - canvasTop) / view.zoom
      if ((axis === 'x' && dy <= 0) || (axis === 'y' && dx <= 0)) return
      const docX = view.x + dx
      const docY = view.y + dy
      const spread = spreadAt(info, docX, docY)
      if (!spread) return
      if (axis === 'x') onGuide('horizontal', docY - spread.area.y0, spread.index)
      else onGuide('vertical', docX - spread.area.x0 + spread.left, spread.index)
    }
    window.addEventListener('pointerup', up)
  }

  return (
    <canvas
      ref={canvas}
      className="layout-ruler"
      data-axis={axis}
      data-origin={Math.round(at * 100) / 100}
      data-zoom={view?.zoom}
      data-step={view ? rulerStep(view.zoom) : undefined}
      onPointerDown={drag}
      data-testid={`layout-ruler-${axis}`}
    />
  )
}
