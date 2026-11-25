import { useEffect, useRef, type RefObject } from 'react'
import type { KitCanvasHandle, KitEngine } from '@kition/sdk/editor'

/** kit-editor's hidden field that takes typed text while an engine tool takes text. */
const SINK = '[data-testid="kit-text-sink"]'

function sinkOf(target: EventTarget | null): HTMLTextAreaElement | null {
  return target instanceof HTMLTextAreaElement && target.matches(SINK) ? target : null
}

/**
 * Where a caret line `[x0, y0, x1, y1]` in document pixels shows on the screen, for a canvas
 * surface at `box` showing `view` (its top-left document point and zoom): the candidate window's
 * spot (left, top) and the line's height. Pure, so it is tested in Node.
 */
export function caretOnScreen(caret: readonly number[], view: { x: number; y: number; zoom: number }, box: { left: number; top: number }) {
  const [x0, y0, x1, y1] = caret
  if (![x0, y0, x1, y1, view.x, view.y, view.zoom].every(Number.isFinite) || view.zoom <= 0) return null
  const left = box.left + (Math.min(x0, x1) - view.x) * view.zoom
  const top = box.top + (Math.min(y0, y1) - view.y) * view.zoom
  const height = Math.max(1, Math.abs(y1 - y0) * view.zoom)
  return { left, top, height }
}

/**
 * Input methods in text (plan 2026-11-25, stage 5). kit-editor's hidden field takes the
 * composition and gives the engine only the committed text; the editor adds the rest:
 * - while composing, the marked text shows in place at the caret, underlined (the engine's
 *   `imePreedit`, a preview with no undo step); when the composition ends the marked text goes
 *   (`imeCancel`) and kit-editor types the committed text in its place as one step, and a
 *   cancelled composition leaves the text as it was;
 * - the hidden field sits at the caret's place on the screen (the engine's `imeCaret`), so the
 *   input method's candidate window opens next to the caret, not in a corner.
 * kit-editor should do both itself (an SDK change request, plan section 9).
 */
export function useInputMethod({ doc, canvas, typing, repaint, revision }: { doc: KitEngine | null; canvas: RefObject<KitCanvasHandle | null>; typing: () => boolean; repaint: () => void; revision: number }) {
  const latest = useRef({ typing, repaint })
  latest.current = { typing, repaint }

  /** Moves the hidden field to the caret (the candidate window opens there). */
  const placeSink = useRef(async (_sink?: HTMLTextAreaElement | null) => {})
  placeSink.current = async (given) => {
    const sink = given ?? document.querySelector<HTMLTextAreaElement>(SINK)
    const handle = canvas.current
    const view = handle?.view()
    const surface = document.querySelector('[data-testid="kit-canvas-surface"]')
    if (!doc || !sink || !view || !surface || !latest.current.typing()) return
    const answer = await doc.invoke<string>('imeCaret', view.zoom).catch(() => 'null')
    const caret = JSON.parse(answer) as number[] | null
    const spot = caret ? caretOnScreen(caret, view, surface.getBoundingClientRect()) : null
    if (!spot) return
    sink.style.position = 'fixed'
    sink.style.left = `${spot.left}px`
    sink.style.top = `${spot.top}px`
    sink.style.height = `${spot.height}px`
    sink.dataset.caret = `${Math.round(spot.left)} ${Math.round(spot.top)}`
  }

  // The caret moved (typing, a click, an arrow): the field follows it.
  useEffect(() => {
    if (doc && latest.current.typing()) void placeSink.current()
  }, [doc, revision])

  useEffect(() => {
    if (!doc) return undefined
    // Composition events come in order; each engine call waits for the one before.
    let chain = Promise.resolve()
    const then = (work: () => Promise<unknown>) => {
      chain = chain.then(work).then(
        async () => {
          await doc.refreshInfo()
          latest.current.repaint()
        },
        () => undefined,
      )
    }
    const onStart = (event: CompositionEvent) => {
      const sink = sinkOf(event.target)
      if (sink) void placeSink.current(sink)
    }
    const onUpdate = (event: CompositionEvent) => {
      const sink = sinkOf(event.target)
      if (!sink) return
      const text = event.data ?? ''
      then(async () => {
        await doc.invoke('imePreedit', text, -1, -1)
        await placeSink.current(sink)
      })
    }
    // The composition ended (committed or cancelled): the marked text goes; the committed text
    // reaches the engine as typed text, which also takes the marked text away first.
    const onEnd = (event: CompositionEvent) => {
      if (!sinkOf(event.target)) return
      then(() => doc.invoke('imeCancel'))
    }
    document.addEventListener('compositionstart', onStart, true)
    document.addEventListener('compositionupdate', onUpdate, true)
    document.addEventListener('compositionend', onEnd, true)
    return () => {
      document.removeEventListener('compositionstart', onStart, true)
      document.removeEventListener('compositionupdate', onUpdate, true)
      document.removeEventListener('compositionend', onEnd, true)
    }
  }, [doc])
}
