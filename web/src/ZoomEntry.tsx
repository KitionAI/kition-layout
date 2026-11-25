import { useEffect, useRef, useState, type RefObject } from 'react'
import { useKitEditor, type KitCanvasHandle } from '@kition/sdk/editor'

/** The zoom a typed percentage sets, or null when it is not one (5% to 4000%). */
export function typedZoom(text: string): number | null {
  const value = Number.parseFloat(text.replace('%', '').replace(',', '.').trim())
  if (!Number.isFinite(value) || value <= 0) return null
  return Math.max(5, Math.min(4000, value)) / 100
}

/**
 * The status bar's zoom percentage takes a typed value: a click on it opens a field in its
 * place; Enter sets the zoom, Escape or leaving it keeps the zoom. (Kit-editor's zoom control
 * shows the value only; plan section 9 asks kition-sdk for the field.)
 */
export function ZoomEntry({ canvas, zoom }: { canvas: RefObject<KitCanvasHandle | null>; zoom: number }) {
  const { t } = useKitEditor()
  const [at, setAt] = useState<DOMRect | null>(null)
  const [text, setText] = useState('')
  const field = useRef<HTMLInputElement>(null)
  const zoomNow = useRef(zoom)
  zoomNow.current = zoom
  useEffect(() => {
    const click = (event: MouseEvent) => {
      const value = event.target instanceof Element ? event.target.closest('[data-testid="kit-zoom"]') : null
      if (!value) return
      setText(String(Math.round(zoomNow.current * 100)))
      setAt(value.getBoundingClientRect())
    }
    document.addEventListener('click', click, true)
    return () => document.removeEventListener('click', click, true)
  }, [])
  useEffect(() => {
    if (at) field.current?.select()
  }, [at])
  if (!at) return null
  const close = () => {
    setAt(null)
    document.querySelector<HTMLElement>('.kui-app')?.focus({ preventScroll: true })
  }
  return (
    <input
      ref={field}
      className="kui-input layout-zoom-entry"
      style={{ left: at.left - 8, top: at.top - 2, width: Math.max(56, at.width + 16), height: at.height + 4 }}
      value={text}
      aria-label={t('zoomPercent')}
      onChange={(event) => setText(event.target.value)}
      onBlur={() => setAt(null)}
      onKeyDown={(event) => {
        event.stopPropagation()
        if (event.key === 'Escape') close()
        if (event.key !== 'Enter') return
        const next = typedZoom(text)
        if (next != null) canvas.current?.zoomTo(next)
        close()
      }}
      data-testid="layout-zoom-entry"
    />
  )
}
