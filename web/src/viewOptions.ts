/**
 * The view options (plan 2026-11-25, stage 3): what the canvas shows over the pages, the screen
 * mode, what drags snap to and the ruler unit. They are the editor's state, kept for the person
 * in the browser's storage, never in the document: the engine is told them (`setView`) and draws
 * and snaps by them, and a `.klayout` saves the same bytes whatever they are. Pure, so it is
 * tested in Node; `App` holds them.
 */
import { ENGINE_UNITS, isUnit, type Unit } from './units.ts'

export type ScreenMode = 'normal' | 'preview' | 'bleed'

export type ViewState = {
  rulers: boolean
  guides: boolean
  lockGuides: boolean
  margins: boolean
  frameEdges: boolean
  bleed: boolean
  documentGrid: boolean
  baselineGrid: boolean
  hiddenCharacters: boolean
  screenMode: ScreenMode
  snapToGuides: boolean
  snapToDocumentGrid: boolean
  smartGuides: boolean
  unit: Unit
}

/** A view option shown as a checked row. */
export type ViewToggle = Exclude<keyof ViewState, 'screenMode' | 'unit'>

/** InDesign's defaults: the aids on, the grids and hidden characters off, points. */
export const VIEW_DEFAULTS: ViewState = {
  rulers: true,
  guides: true,
  lockGuides: false,
  margins: true,
  frameEdges: true,
  bleed: true,
  documentGrid: false,
  baselineGrid: false,
  hiddenCharacters: false,
  screenMode: 'normal',
  snapToGuides: true,
  snapToDocumentGrid: false,
  smartGuides: true,
  unit: 'pt',
}

/** Each checked row: its option, English label (translated by `PHRASES`) and InDesign's key. */
export const VIEW_TOGGLES: ReadonlyArray<readonly [ViewToggle, string, string | null]> = [
  ['rulers', 'Rulers', 'Cmd+R'],
  ['frameEdges', 'Frame Edges', null],
  ['guides', 'Guides', 'Cmd+;'],
  ['lockGuides', 'Lock Guides', 'Cmd+Alt+;'],
  ['margins', 'Margins and Columns', null],
  ['bleed', 'Bleed and Slug', null],
  ['snapToGuides', 'Snap to Guides', 'Cmd+Shift+;'],
  ['smartGuides', 'Smart Guides', 'Cmd+U'],
  ['baselineGrid', 'Baseline Grid', "Cmd+Alt+'"],
  ['documentGrid', 'Document Grid', "Cmd+'"],
  ['snapToDocumentGrid', 'Snap to Document Grid', "Cmd+Shift+'"],
  ['hiddenCharacters', 'Hidden Characters', 'Cmd+Alt+I'],
]

/** The screen modes with their English labels; W switches between Normal and Preview. */
export const SCREEN_MODES: ReadonlyArray<readonly [ScreenMode, string]> = [
  ['normal', 'Normal'],
  ['preview', 'Preview'],
  ['bleed', 'Bleed'],
]

/** The `kit:` command of a view row: `kit:view.guides`, `kit:view.screenMode.preview`. */
export function viewCommand(key: ViewToggle | ScreenMode, screenMode = false) {
  return screenMode ? `kit:view.screenMode.${key}` : `kit:view.${key}`
}

/** What a `kit:view.…` row does to the view, or null when it is no view option (a fit, a page step). */
export function viewChange(command: string, view: ViewState): ViewState | null {
  if (!command.startsWith('kit:view.')) return null
  const rest = command.slice('kit:view.'.length)
  if (rest.startsWith('screenMode.')) {
    const mode = rest.slice('screenMode.'.length)
    return SCREEN_MODES.some(([known]) => known === mode) ? { ...view, screenMode: mode as ScreenMode } : null
  }
  if (rest === 'togglePreview') return { ...view, screenMode: view.screenMode === 'normal' ? 'preview' : 'normal' }
  if (rest.startsWith('unit.')) {
    const unit = rest.slice('unit.'.length)
    return isUnit(unit) ? { ...view, unit } : null
  }
  const toggle = VIEW_TOGGLES.find(([key]) => key === rest)
  return toggle ? { ...view, [toggle[0]]: !view[toggle[0]] } : null
}

/** The English label of a view option's row (`PHRASES` translates it), or null for another row. */
export function viewRowLabel(command: string): string | null {
  if (!command.startsWith('kit:view.')) return null
  const rest = command.slice('kit:view.'.length)
  if (rest === 'togglePreview') return 'Preview'
  if (rest.startsWith('screenMode.')) return SCREEN_MODES.find(([mode]) => mode === rest.slice('screenMode.'.length))?.[1] ?? null
  return VIEW_TOGGLES.find(([key]) => key === rest)?.[1] ?? null
}

/** Whether a `kit:view.…` row is checked now (undefined: not a checked row). */
export function viewChecked(command: string, view: ViewState): boolean | undefined {
  if (!command.startsWith('kit:view.')) return undefined
  const rest = command.slice('kit:view.'.length)
  if (rest.startsWith('screenMode.')) return view.screenMode === rest.slice('screenMode.'.length)
  if (rest === 'togglePreview') return view.screenMode === 'preview'
  if (rest.startsWith('unit.')) return view.unit === rest.slice('unit.'.length)
  const toggle = VIEW_TOGGLES.find(([key]) => key === rest)
  return toggle ? view[toggle[0]] : undefined
}

/** A ruler guide, as the engine numbers it: spread, page in the spread, guide on the page. */
export type GuideRef = { spread: number; page: number; index: number }

/** The engine's view options (`setView`) for `view`, with the selected and the dragged guide. */
export function engineView(view: ViewState, guides: { selected?: GuideRef | null; moving?: GuideRef | null } = {}) {
  return {
    guides: view.guides,
    margins: view.margins,
    frameEdges: view.frameEdges,
    bleed: view.bleed,
    documentGrid: view.documentGrid,
    baselineGrid: view.baselineGrid,
    hiddenCharacters: view.hiddenCharacters,
    screenMode: view.screenMode,
    snapToGuides: view.snapToGuides,
    snapToDocumentGrid: view.snapToDocumentGrid,
    smartGuides: view.smartGuides,
    unit: ENGINE_UNITS[view.unit],
    selectedGuide: guides.selected ?? null,
    movingGuide: guides.moving ?? null,
  }
}

/** Where the view options are kept for the person. */
export const VIEW_STORAGE_KEY = 'kition-layout.view'

/** The view options kept in `storage`, each unknown or missing one at its default; the screen mode starts Normal. */
export function loadView(storage: Pick<Storage, 'getItem'> | null): ViewState {
  let kept: Record<string, unknown> = {}
  try {
    const text = storage?.getItem(VIEW_STORAGE_KEY)
    const parsed: unknown = text ? JSON.parse(text) : null
    if (parsed && typeof parsed === 'object') kept = parsed as Record<string, unknown>
  } catch {
    kept = {}
  }
  const view: ViewState = { ...VIEW_DEFAULTS }
  for (const [key] of VIEW_TOGGLES) if (typeof kept[key] === 'boolean') view[key] = kept[key] as boolean
  if (isUnit(kept.unit)) view.unit = kept.unit
  return view
}

/** Keeps the view options in `storage` (where the browser allows it). */
export function saveView(storage: Pick<Storage, 'setItem'> | null, view: ViewState) {
  try {
    const { screenMode: _session, ...kept } = view
    storage?.setItem(VIEW_STORAGE_KEY, JSON.stringify(kept))
  } catch {
    // Storage refused (private mode, a full quota): the options last this session.
  }
}

/** The page's storage, or null where the browser refuses it (a sandboxed frame). */
export function browserStorage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}
