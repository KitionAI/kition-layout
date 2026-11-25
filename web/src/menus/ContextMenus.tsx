import { createContext, useContext, useEffect, useRef, type RefObject } from 'react'
import { ContextMenu, MenuItem, MenuSeparator, useKitEditor, type KitCanvasHandle } from '@kition/sdk/editor'
import { spreadAt, type DocumentInfo } from '../layout'
import { VIEW_ACTIONS, itemAt, spreadOfPage, type ViewAction } from '../view'
import type { Dialogs } from '../dialogs/DialogHost'
import { asks } from '../dialogs/routes'
import { VARIANT_LABELS, engineCommandOf } from './catalog'
import { contextRows, type Spot } from './contextRows'

/** An open right-click menu: where it was opened and what is there. */
export type OpenMenu = { spot: Spot; x: number; y: number }

/** Opens a right-click menu (the canvas, the rulers, the Pages and Layers panels). */
export type MenuOpener = (spot: Spot, x: number, y: number) => void

const MenuOpenerContext = createContext<MenuOpener | null>(null)
export const MenuOpenerProvider = MenuOpenerContext.Provider

/** The opener panels use for their rows' right-click menus (null outside the editor). */
export function useMenuOpener() {
  return useContext(MenuOpenerContext)
}

type CanvasMenuOptions = {
  info: DocumentInfo | null
  canvas: RefObject<KitCanvasHandle | null>
  onOpen: MenuOpener
  /** The spread in view, for a ruler's menu. */
  spread: () => number
}

/**
 * Right-clicks on the canvas and the rulers (kit-editor's canvas drops the right button, so the
 * editor listens itself): on an object, the object is selected first if it was not; in the text
 * being typed, the text menu (the caret or selection stays); elsewhere, the canvas menu.
 */
export function useCanvasContextMenu({ info, canvas, onOpen, spread }: CanvasMenuOptions) {
  const editor = useKitEditor()
  const latest = useRef({ info, onOpen, editor, spread })
  latest.current = { info, onOpen, editor, spread }
  useEffect(() => {
    const menu = (event: MouseEvent) => {
      const target = event.target
      if (!(target instanceof Element)) return
      const { info: now, onOpen: open, editor: editorNow, spread: inView } = latest.current
      if (target.classList.contains('layout-ruler')) {
        event.preventDefault()
        open({ kind: 'ruler', spread: inView() }, event.clientX, event.clientY)
        return
      }
      if (target.getAttribute('data-testid') !== 'kit-canvas-surface' || !now) return
      event.preventDefault()
      event.stopPropagation()
      const view = canvas.current?.view()
      const box = target.getBoundingClientRect()
      const docX = view ? view.x + (event.clientX - box.left) / view.zoom : 0
      const docY = view ? view.y + (event.clientY - box.top) / view.zoom : 0
      const at = view ? spreadAt(now, docX, docY) : null
      const item = at ? itemAt(now, at, docX - at.area.x0 + at.left, docY - at.area.y0) : null
      if (now.editingText && (!item || item.story != null)) {
        open({ kind: 'text' }, event.clientX, event.clientY)
        return
      }
      if (!item) {
        open({ kind: 'canvas' }, event.clientX, event.clientY)
        return
      }
      const chosen = now.selection.includes(item.id)
      if (!chosen) void editorNow.session.run('selection.set', { ids: [item.id] }).catch(() => undefined)
      const count = chosen ? now.selection.length : 1
      open({ kind: 'object', textFrame: item.story != null, image: item.asset != null, group: item.kind === '<group>', count }, event.clientX, event.clientY)
    }
    // The right button's release is the menu's: the text being typed would take the keyboard
    // back from the menu on it (kit-editor's text field refocuses on every release).
    const release = (event: PointerEvent) => {
      if (event.button === 2) event.stopPropagation()
    }
    window.addEventListener('contextmenu', menu, true)
    window.addEventListener('pointerup', release, true)
    return () => {
      window.removeEventListener('contextmenu', menu, true)
      window.removeEventListener('pointerup', release, true)
    }
  }, [canvas])
}

type MenuProps = {
  menu: OpenMenu | null
  info: DocumentInfo | null
  onClose: () => void
  dialogs: RefObject<Dialogs | null>
  onView: (action: ViewAction) => void
  rulers: boolean
  onRulers: () => void
}

const VIEW_ROW = new Map(VIEW_ACTIONS.map(([action, label, shortcut]) => [`kit:view.${action}`, { action, label, shortcut }]))

/** The open right-click menu's rows, each run as its menu bar row is (through the dialog host). */
export function LayoutContextMenu({ menu, info, onClose, dialogs, onView, rulers, onRulers }: MenuProps) {
  const { byId, enabled, phrase: p, t } = useKitEditor()
  if (!menu) return <ContextMenu at={null} onClose={onClose}>{null}</ContextMenu>
  // A Pages panel menu acts on its page (and that page's spread), not the page in view.
  const place = menu.spot.kind === 'page' && info ? { page: menu.spot.page, spread: spreadOfPage(info, menu.spot.page)?.index ?? 0 } : undefined
  const rows = contextRows(menu.spot)
  return (
    <ContextMenu at={{ x: menu.x, y: menu.y }} onClose={onClose} label={t(`context.${menu.spot.kind}`)} testId={`layout-context-${menu.spot.kind}`}>
      {rows.map((row, index) => {
        if (row === '-') return <MenuSeparator key={`separator-${index}`} />
        const { id, params } = row
        if (id === 'kit:view.rulers') return <MenuItem key={id} label={t(rulers ? 'hideRulers' : 'showRulers')} onSelect={onRulers} testId={`layout-context-row-${id}`} />
        const view = VIEW_ROW.get(id)
        if (view) return <MenuItem key={id} label={p(view.label)} shortcut={view.shortcut} onSelect={() => onView(view.action)} testId={`layout-context-row-${id}`} />
        const command = engineCommandOf(id) ?? id
        const variant = VARIANT_LABELS.get(id)
        const title = variant ?? byId.get(command)?.title ?? command
        const label = p(!variant && asks(command) ? `${title}…` : title)
        return (
          <MenuItem
            key={id}
            label={label}
            shortcut={variant ? null : byId.get(command)?.shortcut}
            disabled={!byId.has(command) || !enabled(id)}
            onSelect={() => dialogs.current?.invoke(id, { place, params })}
            testId={`layout-context-row-${id}`}
          />
        )
      })}
    </ContextMenu>
  )
}
