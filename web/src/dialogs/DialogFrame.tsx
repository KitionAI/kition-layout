import { useState, type ReactNode } from 'react'
import { PreviewDialog, useKitEditor } from '@kition/sdk/editor'
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
 * A command's dialog: kit-editor's `PreviewDialog` without a live preview. The engine keeps an
 * undone step for Redo, so a preview taken back on Cancel would come back with Redo (plan
 * 2026-11-25, stage 1, section 9); the dialog runs once, on OK, as one history step. `onOk` does
 * the work (it throws to keep the dialog open with the reason); Alt turns Cancel into Reset when
 * the dialog has `onReset`. Its title bar moves it, and the next dialog opens where it was left.
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
      onOk={async () => {
        await onOk()
        if (!keepOpen) onClose(true)
      }}
      onCancel={() => onClose(false)}
      onReset={onReset}
      testId={DIALOG_TEST_ID}
    >
      <div className="layout-dialog" data-command={command}>
        {children}
      </div>
    </PreviewDialog>
  )
}
