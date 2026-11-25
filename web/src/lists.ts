/**
 * Bullets and numbering by hand (plan 2026-11-25, stage 5): the Type menu's Bulleted List and
 * Numbered List, their keys and the Home tab's buttons switch the selected paragraphs' list type
 * (`text.para {listType}`) on, or off when it is already that type. Pure, so it is tested in Node.
 */

export type ListKind = 'bullets' | 'numbers'

/** The editor's own rows (`kit:` commands) and their keys. */
export const LIST_ROWS: ReadonlyArray<readonly [ListKind, string, string]> = [
  ['bullets', 'Bulleted List', 'Cmd+Alt+Shift+B'],
  ['numbers', 'Numbered List', 'Cmd+Alt+Shift+N'],
]

/** The list type a paragraph gets when `kind` is switched with `current` showing. */
export function nextListType(current: string | null | undefined, kind: ListKind): ListKind | 'none' {
  return current === kind ? 'none' : kind
}
