// Imports carry `.ts` so `node --test` loads this module as it is.
import { humanize } from './params.ts'

/** The editor's strings and its phrases (`useKitEditor`'s `t` and `phrase`). */
export type Words = { t: (key: string, values?: Record<string, string | number>) => string; phrase: (english: string) => string }

/** The editor's own string `key`, or null when its catalogs lack it (`t` answers the key itself then). */
function own({ t }: Words, key: string) {
  const text = t(key)
  return text === key ? null : text
}

/** A parameter's label: the editor's own (`param.<key>`), else its name in Title Case through the phrases. Never the raw key. */
export function paramLabel(words: Words, key: string) {
  return own(words, `param.${key}`) ?? words.phrase(humanize(key))
}

/** A choice's label: the editor's own (`option.<value>`), else the value in Title Case through the phrases. */
export function optionLabel(words: Words, value: string) {
  if (value === '') return own(words, 'option.none') ?? '—'
  return own(words, `option.${value}`) ?? words.phrase(humanize(value))
}

/** A command's title for its dialog: its phrase, without a trailing "…". */
export function dialogTitle(words: Words, title: string) {
  return words.phrase(title).replace(/\s*(…|\.\.\.)\s*$/, '')
}
