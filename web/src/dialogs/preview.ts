import { useMemo, useRef } from 'react'
import { useKitEditor, type PreviewDialogPreview } from '@kition/sdk/editor'
import { useEditorView } from '../viewContext'
import { refusalReason } from '../menus/context'

/**
 * A dialog's live preview, with `keep`: what OK calls first (true when it kept the preview as the
 * step), and `discard`: what Cancel waits for (the preview taken back before the dialog closes).
 */
export type CommandPreview = PreviewDialogPreview & { keep: (params: Record<string, unknown>) => Promise<boolean>; discard: () => Promise<void> }

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
  return usePreview(command, params, (doc, draft) => doc.invoke('previewCommand', command, JSON.stringify(draft)))
}

/** Several commands a dialog runs together (`[command, params]` each), one history step named by its label. */
export type Steps = Array<[string, Record<string, unknown>]>

/**
 * A dialog's live preview of several commands (plan 2026-11-25, stage 7: the Effects dialog sets
 * the opacity and each effect), as `useCommandPreview` previews one: the engine's `previewSteps`
 * shows them in order on the document as it was when the dialog opened; OK keeps them as one step
 * named `label` when the preview shows just those steps (`keep({steps})`). `steps` null or empty
 * shows the document as it is.
 */
export function useStepsPreview(label: string, steps: Steps | null): CommandPreview | undefined {
  return usePreview(`steps:${label}`, steps && steps.length ? { steps } : null, (doc, draft) => doc.invoke('previewSteps', label, JSON.stringify(draft.steps)))
}

type PreviewDoc = NonNullable<ReturnType<typeof useKitEditor>['session']['doc']>

function usePreview(key: string, params: Record<string, unknown> | null, showDraft: (doc: PreviewDoc, draft: Record<string, unknown>) => Promise<unknown>): CommandPreview | undefined {
  const command = key
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
        await showDraft(doc, draft)
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
      // Cancel takes the preview back at once and the dialog closes after it (plan stage 4: with
      // the panels reading every change, a deferred take-back could land after the dialog closed).
      discard: async () => {
        if (ending) clearTimeout(ending)
        ending = null
        await cancel()
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
