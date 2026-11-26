import { useState, type ReactNode } from 'react'
import { PreviewDialog, useKitEditor } from '@kition/sdk/editor'
import type { CommandPreview } from './preview'
import { refusalReason } from '../menus/context'

/** The test id every command dialog has (its buttons: `-ok`, `-cancel`). */
export const DIALOG_TEST_ID = 'layout-dialog'

/** Where the person last moved a dialog, for the session: the next one opens there too. */
let lastPlace: { x: number; y: number } | null = null

/** A dialog's place: where the person last moved one, kept as they move this one. */
export function useDialogPlace() {
  const [place, setPlace] = useState(lastPlace)
  return {
    position: place,
    onMove: (at: { x: number; y: number }) => {
      lastPlace = at
      setPlace(at)
    },
  }
}

/**
 * Runs `command` with `params` in the editor's session, as one history step; a refusal throws
 * with its reason in the editor's language when it is one a person can meet (else the engine's
 * own words), so the dialog shows it and stays open.
 */
export function useRunInDialog() {
  const { session, t } = useKitEditor()
  return async (command: string, params: Record<string, unknown>) => {
    try {
      const result = await session.run(command, params)
      if (result === null) throw new Error(t('dialog.refused'))
      return result
    } catch (error) {
      const reason = refusalReason(error)
      throw reason ? new Error(t(`hint.${reason}`)) : error
    }
  }
}

/**
 * A command's dialog: kit-editor's `PreviewDialog`. With `preview` (`useCommandPreview`) it shows
 * its values on the document live while its Preview box is checked, through the engine's own
 * preview, which Cancel takes back with no undo or redo step (plan 2026-11-25, stage 3); the
 * command runs once, on OK, as one history step. `onOk` does the work (it throws to keep the
 * dialog open with the reason); Alt turns Cancel into Reset when the dialog has `onReset`. Its
 * title bar moves it, and the next dialog opens where it was left.
 */
export function DialogFrame({
  command,
  title,
  onOk,
  onClose,
  onReset,
  okLabel,
  okDisabled,
  cancelLabel,
  keepOpen,
  modal,
  preview,
  testId = DIALOG_TEST_ID,
  children,
}: {
  command: string
  title: string
  onOk: () => Promise<unknown> | void
  onClose: (committed: boolean) => void
  onReset?: () => void
  okLabel?: string
  okDisabled?: boolean
  cancelLabel?: string
  keepOpen?: boolean
  modal?: boolean
  preview?: CommandPreview
  /** The dialog's test id (its buttons: `-ok`, `-cancel`, `-preview`). */
  testId?: string
  children: ReactNode
}) {
  const place = useDialogPlace()
  return (
    <PreviewDialog
      position={place.position}
      onMove={place.onMove}
      title={title}
      okLabel={okLabel}
      okDisabled={okDisabled}
      cancelLabel={cancelLabel}
      keepOpen={keepOpen}
      modal={modal}
      preview={preview}
      onOk={async () => {
        await onOk()
        if (!keepOpen) onClose(true)
      }}
      onCancel={() => {
        // The preview is taken back before the dialog goes, so nothing of it outlasts the dialog.
        if (preview) void preview.discard().finally(() => onClose(false))
        else onClose(false)
      }}
      onReset={onReset}
      testId={testId}
    >
      <div className="layout-dialog" data-command={command}>
        {children}
      </div>
    </PreviewDialog>
  )
}
