import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { usePanelQuery } from './panels/query'
import { findItem, type DocumentInfo } from './layout'
import { rulerOrigin } from './Rulers'
import { formatLength, type Unit } from './units'
import type { ViewStore } from './CanvasScrollbars'

/**
 * The status bar's Info (plan 2026-11-25, stage 4; InDesign's Info panel): the pointer's place on
 * the canvas from the rulers' origin, the selection's width and height, and the story's words and
 * the characters that do not fit (overset), all in the ruler unit.
 */
export function StatusInfo({ info, views, unit }: { info: DocumentInfo; views: ViewStore; unit: Unit }) {
  const { t } = useKitEditor()
  const pointer = usePointer(info, views)
  const ids = info.selection
  const size = usePanelQuery<{ width: number; height: number }>('transform.info', { ids }, ids.length && !info.editingText ? ids.join(',') : null)
  const first = ids.length ? findItem(info, ids[0])?.item : null
  const storyId = info.textStory ?? first?.story ?? null
  const story = storyId != null ? info.stories.find((entry) => entry.id === storyId) : undefined
  return (
    <>
      <span className="layout-status-info tabular-nums" data-testid="layout-status-pointer">
        {pointer ? `X ${formatLength(pointer.x, unit)}  Y ${formatLength(pointer.y, unit)}` : ''}
      </span>
      {size && ids.length && !info.editingText ? (
        <span className="layout-status-info tabular-nums" data-testid="layout-status-size">
          {`W ${formatLength(size.width, unit)}  H ${formatLength(size.height, unit)}`}
        </span>
      ) : null}
      {story ? (
        <span className="layout-status-info tabular-nums" data-testid="layout-status-story" data-words={story.words ?? 0} data-overset={story.oversetChars ?? 0}>
          {t('status.words', { words: story.words ?? 0 })}
          {story.oversetChars ? ` · ${t('status.overset', { chars: story.oversetChars })}` : ''}
        </span>
      ) : null}
    </>
  )
}

/** Where the pointer is over the canvas, in points from the rulers' origin; null off the canvas. */
function usePointer(info: DocumentInfo, views: ViewStore) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null)
  useEffect(() => {
    let frame = 0
    let last: PointerEvent | null = null
    const read = () => {
      frame = 0
      const event = last
      const view = views.get()
      const region = document.querySelector('.kui-canvas-region')
      if (!event || !view || !region) return
      const box = region.getBoundingClientRect()
      const inside = event.clientX >= box.left && event.clientX <= box.right && event.clientY >= box.top && event.clientY <= box.bottom
      if (!inside) {
        setAt(null)
        return
      }
      const origin = rulerOrigin(info, view)
      if (!origin) return
      const x = view.x + (event.clientX - box.left) / view.zoom
      const y = view.y + (event.clientY - box.top) / view.zoom
      setAt({ x: x - origin.x, y: y - origin.y })
    }
    const onMove = (event: PointerEvent) => {
      last = event
      if (!frame) frame = requestAnimationFrame(read)
    }
    window.addEventListener('pointermove', onMove, { passive: true })
    return () => {
      window.removeEventListener('pointermove', onMove)
      if (frame) cancelAnimationFrame(frame)
    }
  }, [info, views])
  return at
}
