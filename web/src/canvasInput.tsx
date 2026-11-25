/**
 * The canvas input kit-editor does not give the engine yet (plan 2026-11-25, stage 2), shimmed
 * here with the editor's own listeners until kition-sdk does (plan section 9 lists the requests):
 * Space and Ctrl as the engine tools' modifiers, a modifier pressed or released mid-drag taking
 * effect at once, typing ending its undo step at a pause, Cmd/Ctrl or Alt with the wheel zooming
 * about the pointer by at most 1.2 times a notch, Shift with the wheel scrolling sideways, Alt-drag
 * panning while typing, the Zoom tool's drag zooming into the dragged area, a double click on the
 * Hand (fit the spread) and Zoom (100%) tools, focus in the editor when it opens and back on the
 * canvas after Enter in a panel's field, and a blinking text caret.
 */
import { useEffect, useMemo, useRef, type RefObject } from 'react'
import type { EngineOverlay, KitCanvasHandle, KitEngine, KitTool, OverlayDrawer } from '@kition/sdk/editor'
import { wheelZoomFactor, zoomAbout } from './view'

const SURFACE = 'kit-canvas-surface'
/** A pause this long ends the typing's undo step. */
export const TYPING_PAUSE_MS = 1000

function onSurface(target: EventTarget | null): target is HTMLElement {
  return target instanceof HTMLElement && target.getAttribute('data-testid') === SURFACE
}

/** An element that takes Space as a key of its own (a field, the text sink, a button, a menu row). */
function takesSpace(target: EventTarget | null) {
  if (!(target instanceof Element)) return false
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true
  if ((target as HTMLElement).isContentEditable) return true
  return Boolean(target.closest('button, [role="button"], [role="menuitem"], [role="tab"], [role="option"], a[href]'))
}

type Held = { space: boolean; control: boolean }

/**
 * Wraps the open document's tool calls: a pointer event carries Space and Ctrl as they are held
 * (kit-editor sends both as false), a modifier pressed or released during a drag sends the drag
 * again where the pointer is (the engine applies Shift or Alt at once, not at the next move), and
 * typing that pauses ends its undo step (the engine's `endTyping`).
 */
export function useEngineModifiers(doc: KitEngine | null) {
  useEffect(() => {
    if (!doc) return undefined
    const held: Held = { space: false, control: false }
    let dragging = false
    let pointer: { x: number; y: number; id: number; buttons: number; surface: HTMLElement | null } = { x: 0, y: 0, id: 1, buttons: 0, surface: null }
    let pause: number | undefined
    const own = doc as unknown as { tool: KitEngine['tool'] }
    const original = doc.tool.bind(doc)
    own.tool = ((name, args = [], settled = false) => {
      let sent = args
      if (name === 'pointer' && typeof args[0] === 'string') {
        try {
          const event = JSON.parse(args[0]) as { action?: string; mods?: Record<string, boolean> }
          dragging = event.action === 'down' || event.action === 'drag'
          event.mods = { ...event.mods, space: held.space, control: held.control }
          sent = [JSON.stringify(event), ...args.slice(1)]
        } catch {
          // Not the engine tools' event shape: sent as it is.
        }
      }
      if (name === 'toolText') {
        window.clearTimeout(pause)
        pause = window.setTimeout(() => void doc.invoke('endTyping').catch(() => undefined), TYPING_PAUSE_MS)
      }
      return original(name, sent, settled)
    }) as KitEngine['tool']

    const track = (event: PointerEvent) => {
      if (!event.isTrusted) return
      pointer = { x: event.clientX, y: event.clientY, id: event.pointerId, buttons: event.buttons, surface: onSurface(event.target) ? event.target : pointer.surface }
      if (event.type === 'pointerup' || event.type === 'pointercancel') dragging = false
    }
    const key = (event: KeyboardEvent) => {
      const down = event.type === 'keydown'
      const before = { ...held }
      if (event.code === 'Space' && !event.isComposing && !(down && takesSpace(event.target))) held.space = down
      held.control = event.ctrlKey
      const modifier = ['Shift', 'Alt', 'Meta', 'Control'].includes(event.key) || event.code === 'Space'
      if (!modifier || event.repeat || !dragging || !pointer.buttons || !pointer.surface) return
      if (before.space === held.space && before.control === held.control && event.code === 'Space') return
      // The drag again at the same place, with the modifiers now held (a synthetic move the canvas
      // gives the active gesture; it reports itself as its own coalesced sample).
      const move = new PointerEvent('pointermove', { clientX: pointer.x, clientY: pointer.y, pointerId: pointer.id, buttons: pointer.buttons, shiftKey: event.shiftKey, altKey: event.altKey, metaKey: event.metaKey, ctrlKey: event.ctrlKey, bubbles: true, cancelable: true })
      Object.defineProperty(move, 'getCoalescedEvents', { value: () => [move] })
      pointer.surface.dispatchEvent(move)
    }
    const reset = () => {
      held.space = false
      held.control = false
    }
    window.addEventListener('pointerdown', track, true)
    window.addEventListener('pointermove', track, true)
    window.addEventListener('pointerup', track, true)
    window.addEventListener('pointercancel', track, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('keyup', key, true)
    window.addEventListener('blur', reset)
    return () => {
      window.clearTimeout(pause)
      delete (own as { tool?: unknown }).tool
      window.removeEventListener('pointerdown', track, true)
      window.removeEventListener('pointermove', track, true)
      window.removeEventListener('pointerup', track, true)
      window.removeEventListener('pointercancel', track, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('keyup', key, true)
      window.removeEventListener('blur', reset)
    }
  }, [doc])
}

/**
 * The wheel over the canvas: Cmd, Ctrl (a trackpad's pinch) or Alt zooms about the pointer, a
 * notch by at most 1.2 times; Shift scrolls sideways. Alt-drag pans while typing (Space types a
 * space then). Other wheels and drags stay kit-editor's.
 */
export function useCanvasWheel(canvas: RefObject<KitCanvasHandle | null>, typing: () => boolean) {
  const latest = useRef(typing)
  latest.current = typing
  useEffect(() => {
    const wheel = (event: WheelEvent) => {
      if (!onSurface(event.target)) return
      const handle = canvas.current
      const view = handle?.view()
      if (!handle || !view) return
      if (event.ctrlKey || event.metaKey || event.altKey) {
        event.preventDefault()
        event.stopPropagation()
        const delta = event.deltaY || event.deltaX
        if (!delta) return
        const box = event.target.getBoundingClientRect()
        const next = zoomAbout(view, wheelZoomFactor(delta, event.deltaMode), event.clientX - box.left, event.clientY - box.top)
        handle.zoomTo(next.zoom)
        handle.panTo(next.x, next.y)
      } else if (event.shiftKey && event.deltaY && !event.deltaX) {
        event.preventDefault()
        event.stopPropagation()
        handle.panTo(view.x + event.deltaY / view.zoom, view.y)
      }
    }
    const press = (event: PointerEvent) => {
      if (!onSurface(event.target) || event.button !== 0 || !event.altKey || !latest.current()) return
      const handle = canvas.current
      const start = handle?.view()
      if (!handle || !start) return
      event.preventDefault()
      event.stopPropagation()
      const from = { x: event.clientX, y: event.clientY }
      const move = (next: PointerEvent) => {
        next.stopPropagation()
        handle.panTo(start.x - (next.clientX - from.x) / start.zoom, start.y - (next.clientY - from.y) / start.zoom)
      }
      const up = (next: PointerEvent) => {
        next.stopPropagation()
        window.removeEventListener('pointermove', move, true)
        window.removeEventListener('pointerup', up, true)
      }
      window.addEventListener('pointermove', move, true)
      window.addEventListener('pointerup', up, true)
    }
    window.addEventListener('wheel', wheel, { capture: true, passive: false })
    window.addEventListener('pointerdown', press, true)
    return () => {
      window.removeEventListener('wheel', wheel, { capture: true })
      window.removeEventListener('pointerdown', press, true)
    }
  }, [canvas])
}

const MIN_ZOOM_DRAG = 4

/**
 * The Zoom tool: a click zooms in at the pointer (Alt: out), a drag zooms into the area it
 * draws (kit-editor's own Zoom tool ignores a drag).
 */
export const LAYOUT_ZOOM_TOOL: KitTool = {
  id: 'Zoom',
  cursor: 'zoom-in',
  begin: (start, { overlay }) => {
    let last = start
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || '#4c6ef5'
    return {
      move: (pointer) => {
        last = pointer
        overlay.draw((context, toScreen) => {
          const a = toScreen(start.x, start.y)
          const b = toScreen(pointer.x, pointer.y)
          context.strokeStyle = accent
          context.setLineDash([4, 3])
          context.lineWidth = 1
          context.strokeRect(Math.min(a.x, b.x) + 0.5, Math.min(a.y, b.y) + 0.5, Math.abs(b.x - a.x), Math.abs(b.y - a.y))
        })
      },
      end: (pointer, { viewport }) => {
        overlay.clear()
        const moved = Math.hypot(pointer.clientX - start.clientX, pointer.clientY - start.clientY) > MIN_ZOOM_DRAG
        if (!moved) {
          viewport.zoomTo(viewport.view.zoom * (pointer.alt ? 0.5 : 2), pointer.clientX, pointer.clientY)
          return
        }
        const end = Number.isFinite(pointer.x) ? pointer : last
        viewport.fitArea({ x0: Math.min(start.x, end.x), y0: Math.min(start.y, end.y), x1: Math.max(start.x, end.x), y1: Math.max(start.y, end.y) })
      },
      cancel: () => overlay.clear(),
    }
  },
}

/** A double click on the Hand tool's button fits the spread; on the Zoom tool's, 100%. */
export function useViewToolDoubleClicks(onHand: () => void, onZoom: () => void) {
  const latest = useRef({ onHand, onZoom })
  latest.current = { onHand, onZoom }
  useEffect(() => {
    const onDoubleClick = (event: MouseEvent) => {
      const button = event.target instanceof Element ? event.target.closest('[data-kit-ui-id]') : null
      const id = button?.getAttribute('data-kit-ui-id')
      if (id === 'tool:hand') latest.current.onHand()
      else if (id === 'tool:zoom') latest.current.onZoom()
    }
    document.addEventListener('dblclick', onDoubleClick)
    return () => document.removeEventListener('dblclick', onDoubleClick)
  }, [])
}

/**
 * The editor takes the keyboard when the layout opens, so a tool key works without a click
 * first: when the page around it (a same-origin host such as dev.html) has nothing else focused.
 * A host on another origin gives its frame focus itself.
 */
export function useFocusOnOpen(ready: boolean) {
  useEffect(() => {
    if (!ready) return
    let parentIdle = window.parent === window
    try {
      const active = window.parent.document.activeElement
      parentIdle = parentIdle || active === null || active === window.parent.document.body || active === window.frameElement
    } catch {
      parentIdle = parentIdle || false
    }
    if (!parentIdle) return
    const app = document.querySelector<HTMLElement>('.kui-app')
    if (!app || (document.activeElement && document.activeElement !== document.body)) return
    window.focus()
    app.focus({ preventScroll: true })
  }, [ready])
}

/** After Enter in a panel's number or text field, the keys go back to the canvas (or the text being typed). */
export function usePanelEnterReturnsFocus(typing: () => boolean) {
  const latest = useRef(typing)
  latest.current = typing
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Enter' || event.isComposing) return
      const target = event.target
      if (!(target instanceof HTMLInputElement) || !['text', 'number', ''].includes(target.type)) return
      if (!target.closest('.layout-side') || target.closest('.kui-dialog') || target.closest('[data-testid="layout-rename"]')) return
      window.setTimeout(() => {
        if (document.activeElement !== target) return
        const sink = document.querySelector<HTMLTextAreaElement>('.kui-text-sink')
        const app = document.querySelector<HTMLElement>('.kui-app')
        target.blur()
        ;(sink && latest.current() ? sink : app)?.focus({ preventScroll: true })
      }, 0)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
}

/** The text caret (a black line from the engine) blinks: shown, then hidden, this long each. */
export const CARET_BLINK_MS = 530

/** Whether an engine overlay is the text caret. */
export function isCaret(overlay: EngineOverlay) {
  return overlay.kind === 'line' && !overlay.dashed && overlay.color[0] === 0 && overlay.color[1] === 0 && overlay.color[2] === 0 && overlay.color[3] === 255 && Math.abs(overlay.ax - overlay.bx) < Math.abs(overlay.ay - overlay.by)
}

/**
 * The overlay drawer that blinks the caret: it stays on for a moment after each change (typing
 * keeps it steady), then hides and shows every `CARET_BLINK_MS`. `changed` is the document's
 * change revision.
 */
export function useBlinkingCaret(changed: number): OverlayDrawer {
  const since = useRef(performance.now())
  useEffect(() => {
    since.current = performance.now()
  }, [changed])
  return useMemo<OverlayDrawer>(() => (overlay) => isCaret(overlay) && Math.floor((performance.now() - since.current) / CARET_BLINK_MS) % 2 === 1, [])
}
