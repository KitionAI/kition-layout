import { createContext, useContext } from 'react'
import { VIEW_DEFAULTS, type ViewState } from './viewOptions'

/**
 * The view options and the canvas as the panels and dialogs reach them (plan 2026-11-25, stage
 * 3): the ruler unit their lengths show in, and a repaint after the engine changed what it draws
 * without a document change (a dialog's preview, a view option).
 */
export type EditorView = {
  view: ViewState
  setView: (change: (view: ViewState) => ViewState) => void
  /** Draws the document and the engine's overlays again. */
  repaint: () => void
  /**
   * After an editor method that changed the document (`runSteps`): the session's info and the
   * panels follow (a settled tool call), and the canvas draws again.
   */
  settle: () => Promise<void>
}

const EditorViewContext = createContext<EditorView>({ view: VIEW_DEFAULTS, setView: () => undefined, repaint: () => undefined, settle: async () => undefined })

export const EditorViewProvider = EditorViewContext.Provider

export function useEditorView() {
  return useContext(EditorViewContext)
}
