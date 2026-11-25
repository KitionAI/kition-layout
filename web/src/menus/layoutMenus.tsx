import { useEffect, type KeyboardEvent } from 'react'
import { catalogTabs, shortcutMatches, useKitEditor, type FrameTab, type KitCommand, type MenuAction, type Phrases } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { asks } from '../dialogs/routes'
import { VARIANT, engineCommandOf, layoutMenus } from './catalog'
import { contextReason, disabledReason } from './context'
import { menuContext, type Dialogs } from '../dialogs/DialogHost'

/** Menus kept in a read-only document. */
const VIEW_MENUS = ['View', 'Help']

/** The rows each menu tab shows in Single line's row (InDesign's most-used), by menu. */
export const LAYOUT_PRIMARY: Record<string, readonly string[]> = {
  Edit: ['edit.undo', 'edit.redo', 'edit.cut', 'edit.copy', 'edit.paste', 'edit.duplicate', 'edit.stepAndRepeat', 'edit.find'],
  Layout: ['layout.pages.insert', 'layout.parents.new', 'layout.marginsAndColumns', 'layout.addGuides', 'toc.generate', 'layout.section'],
  Type: ['footnote.insert', 'text.convertToOutlines', 'text.changeCase|upper', 'list.define', 'text.fillWithPlaceholder'],
  Object: ['arrange.group', 'arrange.ungroup', 'arrange.bringForward', 'arrange.sendBackward', 'object.fit|fitProportionally', 'object.textFrameOptions', 'object.dropShadow', 'object.lock'],
  Table: ['table.insert', 'table.insertRowBelow', 'table.insertColumnRight', 'table.deleteRow', 'table.deleteColumn', 'table.merge', 'table.unmerge', 'table.convertFromText'],
  View: ['kit:view.fitPage', 'kit:view.fitSpread', 'kit:view.actualSize', 'kit:view.rulers', 'view.showTextThreads', 'guide.deleteAll'],
}

/**
 * Layout's menus as the editor frame's tabs: the catalog's rows for this engine, an engine row
 * running through the dialog host (`onCommand`: a row that asks opens its dialog, a variant runs
 * with its values), a `kit:` row through `onKitCommand`, and the editor's own actions at the top
 * of a menu (`before`).
 */
export function layoutMenuTabs({ commands, phrase, before, onCommand, onKitCommand, checked }: { commands: readonly KitCommand[]; phrase: Phrases; before: Readonly<Record<string, readonly MenuAction[]>>; onCommand: (command: string) => void; onKitCommand: (command: string) => void; checked?: (command: string) => boolean | undefined }): FrameTab[] {
  return catalogTabs(layoutMenus(commands, asks), { phrase, view: VIEW_MENUS, before, primary: LAYOUT_PRIMARY, onCommand, onKitCommand, checked })
}

/**
 * Whether a catalog command can run now: a variant as its engine command, and neither when the
 * selection is not one it works on (Group with one object): greyed out instead of refused.
 */
export function menuEnabled(enabled: (id: string) => boolean, info: DocumentInfo | null) {
  const context = menuContext(info)
  return (id: string) => {
    const command = engineCommandOf(id) ?? id
    if (!enabled(command)) return false
    return contextReason(command, context) === null
  }
}

/**
 * Says why a greyed-out menu row or ribbon button cannot run, in the editor's language, when the
 * pointer rests on it (its title). Kit-editor's menu items take no hint, so the editor sets the
 * title on the drawn element (plan section 9 asks kition-sdk for a `hint`).
 */
export function MenuHints({ info }: { info: DocumentInfo | null }) {
  const { t } = useKitEditor()
  useEffect(() => {
    const context = menuContext(info)
    // The title the element had before a hint replaced it (a ribbon button's label).
    const OWN = 'data-layout-title'
    const mark = () => {
      for (const element of document.querySelectorAll<HTMLButtonElement>('button[data-kit-ui-id^="command:"]')) {
        const id = element.getAttribute('data-kit-ui-id')!.slice('command:'.length)
        const command = engineCommandOf(id) ?? id
        const reason = element.disabled ? (contextReason(command, context) ?? disabledReason(command)) : null
        if (reason) {
          if (!element.hasAttribute(OWN)) element.setAttribute(OWN, element.title)
          const own = element.getAttribute(OWN)
          const title = own ? `${own} — ${t(`hint.${reason}`)}` : t(`hint.${reason}`)
          if (element.title !== title) element.title = title
        } else if (element.hasAttribute(OWN)) {
          const own = element.getAttribute(OWN)
          element.removeAttribute(OWN)
          if (own) element.title = own
          else element.removeAttribute('title')
        }
      }
    }
    mark()
    const observer = new MutationObserver(mark)
    observer.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['disabled'] })
    return () => observer.disconnect()
  }, [info, t])
  return null
}

/** A key handler the frame calls before its own: true when it took the key. */
export type MenuKeys = (event: KeyboardEvent<HTMLDivElement>) => boolean

/**
 * The keys of rows that ask first (Cmd+B Text Frame Options, Cmd+Alt+U Step and Repeat, Cmd+Shift+P
 * Insert Pages) open their dialog instead of the frame running the command bare; the `kit:` rows'
 * keys (Cmd+D Place…, Cmd+J Go to Page…) run the editor's own action.
 */
export function menuKey(event: KeyboardEvent<HTMLDivElement>, { commands, enabled, dialogs, onKitCommand }: { commands: readonly KitCommand[]; enabled: (id: string) => boolean; dialogs: Dialogs; onKitCommand: (command: string) => void }): boolean {
  if (!(event.metaKey || event.ctrlKey || event.altKey)) return false
  const own = KIT_KEYS.find(([shortcut]) => shortcutMatches(shortcut, event, undefined, true))
  if (own) {
    onKitCommand(own[1])
    return true
  }
  const asking = commands.find((command) => command.shortcut && !command.id.includes(VARIANT) && shortcutMatches(command.shortcut, event, undefined, true) && (dialogs.asks(command.id) || INVOKED.includes(command.id)))
  if (!asking) return false
  if (!enabled(asking.id)) return true
  if (INVOKED.includes(asking.id)) dialogs.invoke(asking.id)
  else dialogs.open(asking.id)
  return true
}

/** Commands whose key runs as their menu row does (Find Next runs the last search, or opens Find/Change). */
const INVOKED = ['edit.findNext']

/** The `kit:` rows' keys. */
const KIT_KEYS: ReadonlyArray<readonly [string, string]> = [
  ['Cmd+D', 'kit:file.place'],
  ['Cmd+J', 'kit:layout.goToPage'],
]
