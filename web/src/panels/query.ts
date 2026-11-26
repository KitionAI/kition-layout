import { useEffect, useRef, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'

/**
 * How long the document stays still before the panels ask the engine again after a change that
 * came soon after the one before (typing). Each query costs the engine an inspection of the whole
 * layout (kit-editor's worker brings its info up to date after every command), so in a long
 * story the panels' queries after every key cost more than the key (plan 2026-11-25, phase 9).
 */
export const QUERY_IDLE_MS = 180

/** Answers asked for at one revision, shared by the panels that ask the same thing. */
const asked = new Map<string, Promise<unknown>>()

/**
 * kit-editor's `useEngineQuery`, for the panels: the answer to a query command, asked again after
 * each change. The same query at the same revision is asked once for every panel that shows it
 * (the Properties and Styles panels both read `text.selectionAttrs`); and when changes come in a
 * burst, the panel keeps its last answer and asks once the document has been still for
 * `QUERY_IDLE_MS`. A new `extra` (another selection) asks at once.
 */
export function usePanelQuery<R>(id: string, params: Record<string, unknown> = {}, extra: unknown = undefined): R | null {
  const { query, session } = useKitEditor()
  const { doc } = session
  const revision = session.change.revision
  const [answer, setAnswer] = useState<R | null>(null)
  const shown = JSON.stringify(params)
  const lastChange = useRef({ revision, at: 0 })
  const askedFor = useRef<string | null>(null)
  if (lastChange.current.revision !== revision) lastChange.current = { revision, at: performance.now() }
  useEffect(() => {
    if (!doc) return undefined
    let stale = false
    const subject = `${id}|${shown}|${String(extra)}`
    // A burst of changes (typing): wait for a still moment, unless the panel shows something else now.
    const wait = askedFor.current === subject ? Math.max(0, lastChange.current.at + QUERY_IDLE_MS - performance.now()) : 0
    const ask = () => {
      const key = `${revision}|${id}|${shown}`
      let pending = asked.get(key)
      if (!pending) {
        pending = query(id, JSON.parse(shown) as Record<string, unknown>)
        asked.set(key, pending)
        // Only this revision's answers are kept.
        for (const old of asked.keys()) if (!old.startsWith(`${revision}|`)) asked.delete(old)
      }
      void pending.then(
        (value) => {
          if (stale) return
          askedFor.current = subject
          setAnswer(value as R)
        },
        () => undefined,
      )
    }
    const timer = wait > 0 ? window.setTimeout(ask, wait) : null
    if (timer === null) ask()
    return () => {
      stale = true
      if (timer !== null) window.clearTimeout(timer)
    }
  }, [doc, extra, id, query, revision, shown])
  return answer
}
