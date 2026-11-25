import { Fragment, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { toast, useKitEditor, useUi, type KitCommand } from '@kition/sdk/editor'
import { usePlaceImage } from '../HomeTab'
import type { DocumentInfo } from '../layout'
import { VARIANTS, engineCommandOf } from '../menus/catalog'
import { contextReason, placeParams, placeValues, refusalReason, type MenuContext, type Place } from '../menus/context'
import { DocumentSetupDialog } from './DocumentSetup'
import { ExportDialog } from './Export'
import { FrameSizeDialog, PolygonSettingsDialog, TextFrameOptionsDialog, type FrameSizeRequest } from './FrameDialogs'
import { DeletePagesDialog, GoToPageDialog, InsertPagesDialog, MarginsColumnsDialog, MovePagesDialog, NewDocumentDialog } from './PageDialogs'
import { ParamsDialog } from './ParamsDialog'
import { dialogFor, type DialogKind } from './routes'
import { TemplatesDialog } from './Templates'
import { ConfirmDialog, FindChangeDialog, InsertTableDialog, lastFind } from './TextDialogs'

/** The editor's own dialogs, opened by name (menus' `kit:` rows, Home, the Pages panel, the engine's tools). */
export type OwnDialog = 'newDocument' | 'goToPage' | 'polygon' | 'sample' | 'export' | 'templates' | 'documentSetup'

/**
 * The dialog host (plan 2026-11-25, stage 1): every way a person runs a command (a menu row, a
 * ribbon button, a shortcut, a click the engine's tool answers with a dialog request) goes
 * through `invoke`, which opens the command's dialog when it has one (`routes.ts`) instead of
 * running it with the engine's defaults, runs a variant row with its values, and adds what the
 * command takes from where the person is (the spread in view). One dialog is open at a time.
 */
export type Dialogs = {
  /**
   * Runs a catalog command (an engine id or a variant row) as a click does; a right-click menu
   * says where it was opened (`place`: a page in the Pages panel) and the values of its own row
   * (`params`: the layer it was opened on).
   */
  invoke: (command: string, options?: InvokeOptions) => void
  /** Opens `command`'s dialog (starting from `values`); false when it has none. */
  open: (command: string, values?: Record<string, unknown>) => boolean
  /** Whether `command` opens a dialog. */
  asks: (command: string) => boolean
  /** Opens one of the editor's own dialogs. */
  show: (dialog: OwnDialog) => void
  /** The engine's tool asks for a dialog (`frameSize`, `cmd:<command>`). */
  engine: (name: string, params: unknown) => void
  /** File › Place…: an image the host picks, into the selected frame or onto the page. */
  place: () => void
}

/** Where a run comes from, when not the page in view, and values of its own. */
export type InvokeOptions = { place?: Place; params?: Record<string, unknown> }

type Shown = ({ kind: DialogKind; command: KitCommand; values?: Record<string, unknown> } | { kind: OwnDialog } | { kind: 'frameSize'; request: FrameSizeRequest }) & { place?: Place }
type Open = Shown & { serial: number }

/** The selection as the menu rules read it. */
export function menuContext(info: DocumentInfo | null): MenuContext {
  if (!info) return { items: [], editingText: false }
  const items = info.spreads.flatMap((spread) => spread.items)
  const find = (id: number): { kind: string; asset?: number; story?: number } | undefined => {
    const search = (list: typeof items): (typeof items)[number] | undefined => {
      for (const item of list) {
        if (item.id === id) return item
        const inner = item.children ? search(item.children) : undefined
        if (inner) return inner
      }
      return undefined
    }
    return search(items)
  }
  return { items: info.selection.map((id) => find(id)).flatMap((item) => (item ? [{ kind: item.kind, image: item.asset != null, story: item.story != null }] : [])), editingText: info.editingText }
}

/** Reads the system clipboard's text where the browser lets the editor (Paste in text), else nothing. */
async function clipboardText(): Promise<string | null> {
  try {
    const text = await navigator.clipboard?.readText?.()
    return text ? text : null
  } catch {
    return null
  }
}

/**
 * Holds the open dialog and answers `api` (which App passes to the menus, built outside the
 * editor's provider). `place` is where the person is (the page in view); `onShowPage` shows a page.
 */
export function LayoutDialogs({ api, info, place, onShowPage }: { api: RefObject<Dialogs | null>; info: DocumentInfo | null; place: Place; onShowPage: (page: number) => void }) {
  const editor = useKitEditor()
  const ui = useUi()
  const placing = usePlaceImage()
  const [open, setOpen] = useState<Open | null>(null)
  const serial = useRef(0)
  const latest = useRef({ editor, ui, open, info, place, placing })
  latest.current = { editor, ui, open, info, place, placing }

  useLayoutEffect(() => {
    const show = (next: Shown) => {
      serial.current += 1
      setOpen({ ...next, serial: serial.current })
    }
    /** Runs `command` with `params`; a refusal a person can meet is said in their language. */
    const run = async (command: string, params: Record<string, unknown>) => {
      const { editor: now } = latest.current
      try {
        return await now.session.run(command, params)
      } catch (error) {
        const reason = refusalReason(error)
        if (reason) toast('info', now.t(`hint.${reason}`))
        else now.report(now.t('commandFailed'), error)
        return null
      }
    }
    const openDialog = (id: string, values?: Record<string, unknown>, at?: Place) => {
      const { editor: now, ui: uiNow, open: current, info: infoNow, place: inView } = latest.current
      const where = at ?? inView
      const kind = dialogFor(id)
      const command = now.byId.get(id)
      if (!kind || !command) return false
      if (current || !now.enabled(id) || uiNow.state(`command:${id}`, command.menu).denied) return true
      if (contextReason(id, menuContext(infoNow))) return true
      show({ kind, command, values: { ...placeValues(id, where), ...values }, place: where })
      return true
    }
    api.current = {
      open: openDialog,
      invoke: (id, options = {}) => {
        const { editor: now, info: infoNow } = latest.current
        const where = options.place ?? latest.current.place
        const variant = VARIANTS.get(id)
        const command = variant?.command ?? engineCommandOf(id) ?? id
        if (!variant && openDialog(command, options.params, where)) return
        const reason = contextReason(command, menuContext(infoNow))
        if (reason) {
          toast('info', now.t(`hint.${reason}`))
          return
        }
        // Find Next runs the last search; with none yet it opens Find/Change.
        if (command === 'edit.findNext') {
          const search = lastFind()
          if (!search) openDialog('edit.find')
          else
            void run(command, search).then((answer) => {
              if (answer === 'null') toast('info', now.t('find.none'))
            })
          return
        }
        const params = { ...placeParams(command, where), ...variant?.params, ...options.params }
        if (command === 'table.convertFromText') {
          void run(command, params).then((answer) => {
            // The caret goes into the new table's first cell, so typing goes on there.
            const table = answer ? (JSON.parse(answer) as { table?: number }).table : undefined
            if (table != null) void run('table.select', { table, rows: [0, 0], cols: [0, 0], what: 'cell' })
          })
          return
        }
        if ((command === 'edit.paste' || command === 'edit.pasteWithoutFormatting') && infoNow?.editingText) {
          // In text, Paste takes the system clipboard's text where the browser gives it.
          void clipboardText().then((text) => run(command, text ? { ...params, text } : params))
          return
        }
        if (command === 'file.newSample') {
          if (now.session.doc?.info.dirty) show({ kind: 'sample' })
          else void run(command, {})
          return
        }
        void run(command, params)
      },
      asks: (id) => dialogFor(id) !== null,
      show: (dialog) => {
        if (!latest.current.open) show({ kind: dialog })
      },
      place: () => {
        const { placing: now, editor: editorNow } = latest.current
        if (now.can) void now.place()
        else toast('info', editorNow.t('hint.noPick'))
      },
      engine: (name, params) => {
        if (latest.current.open) return
        if (name === 'frameSize' && params && typeof params === 'object') show({ kind: 'frameSize', request: params as FrameSizeRequest })
        else if (name.startsWith('cmd:')) openDialog(name.slice(4), (params ?? {}) as Record<string, unknown>)
      },
    }
  })

  if (!open || !info) return null
  const close = () => setOpen(null)
  const page = (open.place ?? latest.current.place).page
  const dialog = ((): ReactNode => {
    switch (open.kind) {
      case 'newDocument':
        return <NewDocumentDialog onClose={close} />
      case 'goToPage':
        return <GoToPageDialog info={info} page={page} onShow={onShowPage} onClose={close} />
      case 'polygon':
        return <PolygonSettingsDialog onClose={close} />
      case 'sample':
        return <ConfirmDialog title={editor.phrase('Sample Document')} message={editor.t('dialog.sampleNote')} confirm={editor.t('dialog.openSample')} onConfirm={() => void editor.run('file.newSample', {})} onClose={close} testId="layout-sample-confirm" />
      case 'export':
        return <ExportDialog info={info} page={page} onClose={close} />
      case 'templates':
        return <TemplatesDialog onClose={close} />
      case 'documentSetup':
        return <DocumentSetupDialog info={info} onClose={close} />
      case 'frameSize':
        return <FrameSizeDialog request={open.request} onClose={close} />
      case 'marginsColumns':
        return <MarginsColumnsDialog info={info} page={page} onClose={close} />
      case 'insertPages':
        return <InsertPagesDialog info={info} page={page} onClose={close} />
      case 'movePages':
        return <MovePagesDialog info={info} page={page} onClose={close} />
      case 'deletePages':
        return <DeletePagesDialog info={info} page={page} onClose={close} />
      case 'textFrameOptions':
        return <TextFrameOptionsDialog info={info} onClose={close} />
      case 'insertTable':
        return <InsertTableDialog onClose={close} />
      case 'findChange':
        return <FindChangeDialog onClose={close} />
      case 'params':
        return <ParamsDialog command={open.command} info={info} values={open.values} fixed={placeParams(open.command.id, open.place ?? latest.current.place)} onClose={close} />
    }
  })()
  // A new dialog (its serial) starts from its own values, never the last one's.
  return <Fragment key={open.serial}>{dialog}</Fragment>
}
