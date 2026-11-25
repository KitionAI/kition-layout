import { useMemo, useRef } from 'react'
import { useKitEditor, type PreviewDialogPreview } from '@kition/sdk/editor'
import { useEditorView } from '../viewContext'
import { refusalReason } from '../menus/context'

/**
 * A dialog's live preview (plan 2026-11-25, stage 3), for kit-editor's `PreviewDialog`: while
 * Preview is on, each change of `params` shows `command` with them on the document as it was when
 * the dialog opened (the engine's `previewCommand`, replacing the last preview); Cancel, Preview
 * off and closing take it back (`cancelPreview`: no undo or redo step); OK takes it back too and
 * the dialog then runs the command once, as one history step. `params` null (values not valid
 * yet, or nothing changed) shows the document as it is. Undefined without an open document.
 */
export function useCommandPreview(command: string, params: Record<string, unknown> | null): PreviewDialogPreview | undefined {
  const { session, t } = useKitEditor()
  const { repaint } = useEditorView()
  const doc = session.doc
  const latest = useRef({ params, doc, repaint, t })
  latest.current = { params, doc, repaint, t }
  const shown = params && Object.keys(params).length ? params : null
  const api = useMemo<Omit<PreviewDialogPreview, 'draft'> | null>(() => {
    if (!doc) return null
    const settle = async () => {
      await doc.refreshInfo()
      latest.current.repaint()
    }
    const show = async () => {
      const { params: now, t: words } = latest.current
      const draft = now && Object.keys(now).length ? now : null
      if (!draft) {
        if (await doc.invoke<boolean>('cancelPreview')) await settle()
        return
      }
      try {
        await doc.invoke('previewCommand', command, JSON.stringify(draft))
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
      // Always asked: a preview whose command failed still holds the document's state to put back.
      end: async () => {
        if (await doc.invoke<boolean>('cancelPreview')) await settle()
      },
    }
  }, [doc, command])
  if (!api) return undefined
  return { ...api, draft: shown }
}
