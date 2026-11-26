/**
 * Parent Page Options and Delete Parent Spread (plan 2026-11-25, phase 11): the
 * `layout.parents.set` a dialog's changes make, which parents a parent may be based on (none that
 * would make a loop), and which may take a deleted parent's place. Pure, so it is tested in Node.
 */

/** A parent as `inspect()` lists it. */
export type ParentEntry = { id: number; label: string | null; prefix?: string | null; name?: string | null; basedOn?: string | null; pages?: number; items: number }

/** Parent Page Options' values: prefix, name, the parent it is based on ('' for none), its number of pages. */
export type ParentDraft = { prefix: string; name: string; basedOn: string; pages: number }

/** The values the dialog opens with. */
export function parentDraft(parent: ParentEntry): ParentDraft {
  return { prefix: parent.prefix ?? '', name: parent.name ?? '', basedOn: parent.basedOn ?? '', pages: parent.pages ?? 1 }
}

/** Why a prefix cannot be used ('' when it can): InDesign's 1 to 4 characters, unique, no hyphen. */
export function prefixProblem(prefix: string, parents: readonly ParentEntry[], own: string | null): '' | 'empty' | 'long' | 'hyphen' | 'taken' {
  const text = prefix.trim()
  if (!text) return 'empty'
  if ([...text].length > 4) return 'long'
  if (text.includes('-') || text.startsWith('[')) return 'hyphen'
  if (parents.some((parent) => parent.prefix === text && parent.label !== own)) return 'taken'
  return ''
}

/**
 * The `layout.parents.set` parameters for what changed (null: nothing did, or a value is not
 * valid). `parent` is the parent's label as it was when the dialog opened.
 */
export function parentEdit(parent: string, from: ParentDraft, to: ParentDraft, parents: readonly ParentEntry[]): Record<string, unknown> | null {
  const params: Record<string, unknown> = { parent }
  const prefix = to.prefix.trim()
  if (prefix !== from.prefix) {
    if (prefixProblem(prefix, parents, parent)) return null
    params.prefix = prefix
  }
  const name = to.name.trim()
  if (name !== from.name) {
    if (!name) return null
    params.name = name
  }
  if (to.basedOn !== from.basedOn) params.basedOn = to.basedOn || null
  if (to.pages !== from.pages) {
    if (!Number.isInteger(to.pages) || to.pages < 1 || to.pages > 10) return null
    params.pages = to.pages
  }
  return Object.keys(params).length > 1 ? params : null
}

/** The parents `label` may be based on: every other parent that is not based on it (directly or not). */
export function basedOnChoices(parents: readonly ParentEntry[], label: string): string[] {
  const byLabel = new Map(parents.map((parent) => [parent.label ?? '', parent]))
  const loops = (start: string) => {
    let at: string | null | undefined = start
    for (let steps = 0; at && steps <= parents.length; steps += 1) {
      if (at === label) return true
      at = byLabel.get(at)?.basedOn
    }
    return false
  }
  return parents.flatMap((parent) => (parent.label && !loops(parent.label) ? [parent.label] : []))
}

/** What may take a deleted parent's place: none ('') first, then every other parent. */
export function parentReplacements(parents: readonly ParentEntry[], deleting: string): string[] {
  return ['', ...parents.flatMap((parent) => (parent.label && parent.label !== deleting ? [parent.label] : []))]
}

/** How many document pages use `label` (a deleted parent's pages are asked about). */
export function pagesUsing(spreads: ReadonlyArray<{ pages: ReadonlyArray<{ parent: string | null }> }>, label: string): number {
  return spreads.reduce((count, spread) => count + spread.pages.filter((page) => page.parent === label).length, 0)
}
