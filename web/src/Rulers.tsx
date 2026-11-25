import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import type { KitCanvasView } from '@kition/sdk/editor'
import { spreadAt, type DocumentInfo, type Spread } from './layout'
import { rulerLabel, rulerTicks, type Unit } from './units'

/** Ruler thickness in CSS pixels. */
export const RULER = 18

/** The major step (points) for `zoom` screen pixels per point in `unit`. */
export function rulerStep(zoom: number, unit: Unit = 'pt') {
  return rulerTicks(unit, zoom).major
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
 * One ruler along the canvas (InDesign's: the ruler unit from the top-left of the page in view;
 * ticks every major step, cut into parts between). A press starts a guide (`onGuide`), which the
 * guide layer drags onto the canvas. Its origin, scale and unit are on `data-origin`, `data-zoom`
 * and `data-unit`.
 */
export function Ruler({ axis, info, view, unit, onGuide }: { axis: 'x' | 'y'; info: DocumentInfo; view: KitCanvasView | null; unit: Unit; onGuide: (axis: 'x' | 'y', event: ReactPointerEvent<HTMLCanvasElement>) => void }) {
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
    const { major: step, parts } = rulerTicks(unit, view.zoom)
    const minor = step / Math.max(1, parts)
    const first = Math.floor(-at / view.zoom / minor)
    for (let n = first; (n * minor * view.zoom + at) <= length; n += 1) {
      const value = n * minor
      const position = Math.round(value * view.zoom + at) + 0.5
      const major = n % Math.max(1, parts) === 0
      const tick = major ? RULER : RULER * 0.3
      if (axis === 'x') {
        context.moveTo(position, RULER - tick)
        context.lineTo(position, RULER)
        if (major) context.fillText(rulerLabel(value, unit), position + 2, 9)
      } else {
        context.moveTo(RULER - tick, position)
        context.lineTo(RULER, position)
        if (major) {
          context.save()
          context.translate(9, position + 2)
          context.rotate(-Math.PI / 2)
          context.textAlign = 'right'
          context.fillText(rulerLabel(value, unit), 0, 0)
          context.restore()
        }
      }
    }
    context.stroke()
  }, [at, axis, line, size, surface, text, view, unit])

  return (
    <canvas
      ref={canvas}
      className="layout-ruler"
      data-axis={axis}
      data-origin={Math.round(at * 100) / 100}
      data-zoom={view?.zoom}
      data-step={view ? rulerStep(view.zoom, unit) : undefined}
      data-unit={unit}
      onPointerDown={(event) => onGuide(axis, event)}
      data-testid={`layout-ruler-${axis}`}
    />
  )
}
