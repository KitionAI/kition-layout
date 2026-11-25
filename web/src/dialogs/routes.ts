// Imports carry `.ts` so `node --test` loads these modules as they are.
import { hiddenReason } from '../menus/hidden.ts'

/**
 * Which dialog a command opens (plan 2026-11-25, stage 1). The engine's titles carry no "…", so
 * the editor says which commands ask first: the purpose-built dialogs, and the commands the
 * generic dialog asks (each has values a person sets; run bare they apply the engine's defaults,
 * or fail for a missing value). Pure, so the rules are tested without a browser.
 */

/** The purpose-built dialogs, and `params`, the generic one drawn from a command's parameters. */
export type DialogKind = 'documentSetup' | 'marginsColumns' | 'insertPages' | 'movePages' | 'deletePages' | 'textFrameOptions' | 'insertTable' | 'findChange' | 'params'

/** Commands with a dialog of their own. */
export const PURPOSE_BUILT: Readonly<Record<string, DialogKind>> = {
  'layout.documentSetup': 'documentSetup',
  'layout.marginsAndColumns': 'marginsColumns',
  'layout.pages.insert': 'insertPages',
  'layout.pages.move': 'movePages',
  'layout.pages.delete': 'deletePages',
  'object.textFrameOptions': 'textFrameOptions',
  'table.insert': 'insertTable',
  'find.find': 'findChange',
  'find.change': 'findChange',
}

/** Commands the generic dialog asks before they run. */
export const GENERIC: ReadonlySet<string> = new Set([
  'edit.stepAndRepeat',
  'spelling.addWord',
  'hyphenation.addException',
  'layout.pages.applyParent',
  'layout.parents.new',
  'layout.pages.toSpread',
  'layout.spreadShuffle',
  'layout.createGuides',
  'layout.createAlternate',
  'layout.pageSize',
  'liquid.pageRule',
  'layout.section',
  'toc.generate',
  'index.addReference',
  'index.generate',
  'font.replace',
  'footnote.options',
  'endnote.options',
  'xref.insert',
  'xref.defineFormat',
  'variables.define',
  'variables.insert',
  'list.define',
  'math.insert',
  'type.pathOptions',
  'transform.move',
  'transform.scale',
  'transform.rotate',
  'transform.shear',
  'object.qrCode',
  'object.cornerOptions',
  'object.fittingOptions',
  'object.dropShadow',
  'object.innerShadow',
  'object.outerGlow',
  'object.innerGlow',
  'object.bevel',
  'object.satin',
  'object.feather',
  'object.directionalFeather',
  'object.gradientFeather',
  'object.globalLight',
  'object.exportOptions',
  'object.textWrap',
  'object.caption',
  'table.convertFromText',
  'table.sortRows',
])

/** The dialog `command` opens instead of running, or null when it runs at once (and for hidden commands, which agents run). */
export function dialogFor(command: string): DialogKind | null {
  const own = PURPOSE_BUILT[command]
  if (own) return own
  if (hiddenReason(command)) return null
  return GENERIC.has(command) ? 'params' : null
}

/** Whether a menu row of `command` asks first (its label ends in "…"). */
export function asks(command: string) {
  return dialogFor(command) !== null
}
