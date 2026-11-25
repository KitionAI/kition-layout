import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import type { PlaceQueued } from './layout'
import { placeBadge, thumbnailOf } from './images'

/**
 * The place cursor's load (plan 2026-11-25, stage 6): beside the pointer over the canvas, a picture
 * of the graphic a click places next and, with several loaded, how many. The arrow keys bring the
 * next one to the front, Escape drops it.
 */
export function PlaceCursorBadge({ queue, tool }: { queue: readonly PlaceQueued[]; tool: string }) {
  const { t } = useKitEditor()
  const [at, setAt] = useState<{ x: number; y: number } | null>(null)
  const badge = tool === 'placeGun' ? placeBadge(queue) : null
  const shown = badge != null
  useEffect(() => {
    if (!shown) return undefined
    const move = (event: PointerEvent) => {
      const surface = event.target instanceof Element && event.target.closest('[data-testid="kit-canvas-surface"]')
      setAt(surface ? { x: event.clientX, y: event.clientY } : null)
    }
    window.addEventListener('pointermove', move, true)
    return () => window.removeEventListener('pointermove', move, true)
  }, [shown])
  if (!badge) return null
  const picture = thumbnailOf(badge.next.asset)
  return (
    <div
      className="layout-place-badge"
      style={at ? { left: at.x + 18, top: at.y + 18 } : { visibility: 'hidden' }}
      aria-hidden={at ? undefined : true}
      data-testid="layout-place-badge"
      data-count={badge.count}
      data-next={badge.next.name}
      title={badge.next.name}
    >
      {picture ? <img src={picture} alt="" /> : null}
      {badge.count > 1 ? <span className="layout-place-count">{t('placeLoaded', { count: badge.count })}</span> : null}
    </div>
  )
}
