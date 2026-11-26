import { useKitEditor } from '@kition/sdk/editor'
import { errorText, type Words } from './errors'
import { REASONS } from './menus/context'

/** The words `errorText` needs, from the editor's strings and its commands' titles. */
export function errorWords(t: Words['t'], phrase: (english: string) => string, title: (command: string) => string | undefined): Words {
  return { t, title: (command) => phrase(title(command) ?? command), reasons: REASONS }
}

/** `errorText` with the editor's words: what to tell the person about an engine error. */
export function useErrorText() {
  const { t, phrase, byId } = useKitEditor()
  const words = errorWords(t, phrase, (command) => byId.get(command)?.title)
  return (error: unknown) => errorText(error, words)
}
