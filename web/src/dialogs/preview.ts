import { useMemo, useRef } from 'react'
import { useKitEditor, type PreviewDialogPreview } from '@kition/sdk/editor'
import { useEditorView } from '../viewContext'
import { refusalReason } from '../menus/context'

/** A dialog's live preview, with `keep`: what OK calls first (true when it kept the preview as the step). */
export type CommandPreview = PreviewDialogPreview & { keep: (params: Record<string, unknown>) => Promise<boolean> }

/**
 * A dialog's live preview (plan 2026-11-25, stage 3), for kit-editor's `PreviewDialog`: while
 * Preview is on, each change of `params` shows `command` with them on the document as it was when
 * the dialog opened (the engine's `previewCommand`, replacing the last preview); Cancel, Preview
 * off and closing take it back (`cancelPreview`: no undo or redo step). The dialog's OK asks
 * `keep(params)` first: when the preview shows just those values it stays as one history step
 * (`commitPreview`), with no moment where the document goes back; otherwise it is taken back and
 * the dialog runs the command once. `params` null (values not valid yet, or nothing changed)
 * shows the document as it is. Undefined without an open document.
 */
export function useCommandPreview(command: string, params: Record<string, unknown> | null): CommandPreview | undefined {
  const { session, t } = useKitEditor()
  const { repaint } = useEditorView()
  const doc = session.doc
  const latest = useRef({ params, repaint, t })
  latest.current = { params, repaint, t }
  const shown = params && Object.keys(params).length ? params : null
  const api = useMemo<Omit<CommandPreview, 'draft'> | null>(() => {
    if (!doc) return null
    // The values the engine's preview shows now (JSON), and a take-back `end` left for OK to decide.
    let showing: string | null = null
    let ending: ReturnType<typeof setTimeout> | null = null
    const settle = async () => {
      await doc.refreshInfo()
      latest.current.repaint()
    }
    const cancel = async () => {
      showing = null
      if (await doc.invoke<boolean>('cancelPreview')) await settle()
    }
    const show = async () => {
      if (ending) {
        clearTimeout(ending)
        ending = null
      }
      const { params: now, t: words } = latest.current
      const draft = now && Object.keys(now).length ? now : null
      if (!draft) return cancel()
      try {
        showing = null
        await doc.invoke('previewCommand', command, JSON.stringify(draft))
        showing = JSON.stringify(draft)
      } catch (error) {
        const reason = refusalReason(error)
        throw reason ? new Error(words(`hint.${reason}`)) : error
      } finally {
        await settle()
      }
    }
    return {
      begin: show,
      update: show,
      // Taken back a moment later unless OK keeps it (the dialog ends its preview before its OK
      // runs). Always asked: a preview whose command failed still holds the state to put back.
      end: () => {
        if (ending) clearTimeout(ending)
        ending = setTimeout(() => {
          ending = null
          void cancel()
        }, 0)
      },
      keep: async (values) => {
        const waiting = ending !== null
        if (ending) clearTimeout(ending)
        ending = null
        if (waiting && showing !== null && showing === JSON.stringify(values)) {
          showing = null
          const kept = await doc.invoke<boolean>('commitPreview')
          await settle()
          return kept
        }
        await cancel()
        return false
      },
    }
  }, [doc, command])
  if (!api) return undefined
  return { ...api, draft: shown }
}
