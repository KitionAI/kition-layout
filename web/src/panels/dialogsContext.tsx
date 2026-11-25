import { createContext, useContext, type RefObject } from 'react'
import type { Dialogs } from '../dialogs/DialogHost'

/**
 * The dialog host for the panels (plan 2026-11-25, stage 4): a panel's click runs a command the way
 * a menu row does (`dialogs.current.invoke`), so it asks first, takes the page in view, and says
 * a refusal in the person's language.
 */
const DialogsContext = createContext<RefObject<Dialogs | null> | null>(null)

export const DialogsProvider = DialogsContext.Provider

export function useDialogs() {
  return useContext(DialogsContext)
}
