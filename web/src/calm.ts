import { startTransition, useEffect, useRef, useState } from 'react'

/** How often, at most, the panels follow the document while text is typed. */
export const CALM_MS = 150

/**
 * `value`, and while `busy` (text is being typed) the value as it was at most `ms` ago: the
 * panels show the document a moment after the canvas, so a key's change reaches the screen
 * without the dock rendering first (plan 2026-11-25, phase 12: 94–108 ms from a key to the page).
 * Not busy, the value is the current one at once. The catching up is a transition: a key's
 * render goes before it.
 */
export function useCalmValue<T>(value: T, busy: boolean, ms = CALM_MS): T {
  const shown = useRef(value)
  const latest = useRef(value)
  latest.current = value
  if (!busy) shown.current = value
  const [, bump] = useState(0)
  const timer = useRef<number | null>(null)
  useEffect(() => {
    if (!busy || shown.current === value || timer.current !== null) return
    timer.current = window.setTimeout(() => {
      timer.current = null
      shown.current = latest.current
      startTransition(() => bump((count) => count + 1))
    }, ms)
  }, [busy, value, ms])
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current)
    },
    [],
  )
  return shown.current
}
