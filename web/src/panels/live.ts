import { useCallback, useRef } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { useEditorView } from '../viewContext'

/**
 * Runs a panel's command live (plan 2026-11-25, stage 4): while a control is dragged (a colour
 * picker's square, a scrubbed number) each value is the engine's dialog preview of the command
 * (`previewCommand`, replacing the last, no history step); the last value (`final`) keeps the
 * preview as one step when it shows just that value, else runs the command once. So a drag is one
 * undo step and the document follows the pointer. Calls are taken one after another.
 */
export function useLiveRun() {
  const editor = useKitEditor()
  const { repaint } = useEditorView()
  const latest = useRef({ editor, repaint })
  latest.current = { editor, repaint }
  const showing = useRef<string | null>(null)
  const queue = useRef<Promise<unknown>>(Promise.resolve())
  return useCallback((command: string, params: Record<string, unknown>, final = true) => {
    const step = async () => {
      const { editor: now, repaint: redraw } = latest.current
      const doc = now.session.doc
      const key = `${command} ${JSON.stringify(params)}`
      const settle = async () => {
        await doc?.refreshInfo()
        redraw()
      }
      if (!doc) return
      if (!final) {
        try {
          await doc.invoke('previewCommand', command, JSON.stringify(params))
          showing.current = key
        } catch {
          showing.current = null
        }
        await settle()
        return
      }
      if (showing.current === key) {
        showing.current = null
        await doc.invoke('commitPreview')
        await settle()
        return
      }
      if (showing.current !== null) {
        showing.current = null
        await doc.invoke('cancelPreview')
        await settle()
      }
      await now.run(command, params)
    }
    const next = queue.current.then(step, step)
    queue.current = next.catch(() => undefined)
    return next
  }, [])
}
