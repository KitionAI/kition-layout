import type { MenuRow } from '@kition/sdk/editor'
import { VIEW_ACTIONS } from '../view.ts'

/**
 * Layout's menus (plan 2026-11-25, stage 1): the reference application's menu tables, in its
 * order and with its separators, for the commands this engine has. Engine rows run through the
 * dialog host (a row that asks first opens its dialog); a "variant" row runs one command with
 * fixed values (Align Left is `object.align` with `{edge: "left"}`) and is listed as
 * `<command>|<variant>`; a `kit:` row is the editor's own (Place…, Go to Page…). The engine's
 * menu commands left out are in `hidden.ts`, with their reasons. Pure, so it is tested in Node.
 */

/** The separator between a variant row's command and its variant name. */
export const VARIANT = '|'

/** What a variant row runs. */
export type Variant = { command: string; params: Record<string, unknown> }

type Entry =
  | '-'
  | `>${string}`
  | '<'
  | { cmd: string; label?: string }
  | { cmd: string; variant: string; label: string; params: Record<string, unknown> }
  | { kit: string; label: string; shortcut?: string }

const c = (cmd: string, label?: string): Entry => ({ cmd, label })
const v = (cmd: string, variant: string, label: string, params: Record<string, unknown>): Entry => ({ cmd, variant, label, params })
const k = (kit: string, label: string, shortcut?: string): Entry => ({ kit, label, shortcut })

/** A run of variant rows: one command, each with its own value of one key. */
const each = (cmd: string, key: string, rows: ReadonlyArray<readonly [string, unknown, string]>): Entry[] => rows.map(([variant, value, label]) => v(cmd, variant, label, { [key]: value }))

/** Text typed at the insertion point as it is (no typographer's quotes). */
const insert = (variant: string, label: string, text: string): Entry => v('text.insert', variant, label, { text, raw: true })
/** A marker or break at the insertion point. */
const special = (character: string, label: string): Entry => v('text.insertSpecial', character, label, { character })

const MENUS: ReadonlyArray<readonly [string, readonly Entry[]]> = [
  // File: New…, New from Template…, Document Setup… and Export… are the editor's own actions at the top (App).
  ['File', [k('file.place', 'Place…', 'Cmd+D')]],
  [
    'Edit',
    [
      c('edit.undo'),
      c('edit.redo'),
      '-',
      c('edit.cut'),
      c('edit.copy'),
      c('edit.paste'),
      c('edit.pasteWithoutFormatting'),
      c('edit.pasteInto'),
      c('edit.pasteInPlace'),
      c('edit.delete'),
      '-',
      c('edit.duplicate'),
      c('edit.stepAndRepeat'),
      '-',
      c('edit.selectAll'),
      c('edit.deselectAll'),
      '-',
      c('edit.find', 'Find/Change'),
      c('edit.findNext'),
      '-',
      '>Spelling',
      c('spelling.addWord'),
      c('hyphenation.addException'),
      '<',
      '>Transparency Blend Space',
      v('edit.transparencyBlendSpace', 'rgb', 'Document RGB', { space: 'rgb' }),
      v('edit.transparencyBlendSpace', 'cmyk', 'Document CMYK', { space: 'cmyk' }),
      '<',
    ],
  ],
  [
    'Layout',
    [
      '>Pages',
      c('layout.pages.insert'),
      c('layout.pages.move'),
      c('layout.pages.duplicateSpread'),
      c('layout.pages.delete'),
      '-',
      c('layout.pages.applyParent'),
      c('layout.parents.new'),
      '-',
      c('layout.overrideParentItems'),
      c('layout.removeOverrides'),
      c('layout.detachAll'),
      '-',
      c('layout.pages.toSpread'),
      c('layout.spreadShuffle'),
      '<',
      c('layout.marginsAndColumns'),
      c('layout.addGuides'),
      c('layout.addAlternate'),
      c('liquid.pageRule'),
      '-',
      k('layout.goToPage', 'Go to Page…', 'Cmd+J'),
      '-',
      c('layout.section'),
      c('toc.generate'),
      c('toc.update'),
      '>Index',
      c('index.addReference'),
      c('index.generate'),
      c('index.update'),
      '<',
    ],
  ],
  [
    'Type',
    [
      '>Size',
      ...[6, 7, 8, 9, 10, 11, 12, 14, 18, 24, 30, 36, 48, 60, 72].map((size) => v('text.char', `size${size}`, `${size} pt`, { attrs: { size } })),
      '<',
      '-',
      c('text.convertToOutlines'),
      c('font.replace'),
      '>Change Case',
      ...each('text.changeCase', 'case', [
        ['upper', 'upper', 'UPPERCASE'],
        ['lower', 'lower', 'lowercase'],
        ['title', 'title', 'Title Case'],
        ['sentence', 'sentence', 'Sentence case'],
      ]),
      '<',
      '-',
      '>Type on a Path',
      c('text.pathOptions'),
      v('text.pathOptions', 'delete', 'Delete Type from Path', { delete: true }),
      '<',
      '-',
      c('footnote.insert'),
      c('footnote.options'),
      c('endnote.insert'),
      c('endnote.options'),
      c('footnote.goToReference'),
      '-',
      '>Hyperlinks & Cross-References',
      c('xref.insert'),
      c('xref.defineFormat'),
      '<',
      '>Text Variables',
      c('variable.define'),
      c('variable.insert'),
      '<',
      '-',
      '>Bulleted & Numbered Lists',
      c('list.define'),
      '<',
      '-',
      '>Insert Special Character',
      '>Symbols',
      insert('bullet', 'Bullet Character', '•'),
      insert('copyright', 'Copyright Symbol', '©'),
      insert('ellipsis', 'Ellipsis', '…'),
      insert('paragraph', 'Paragraph Symbol', '¶'),
      insert('registered', 'Registered Trademark Symbol', '®'),
      insert('section', 'Section Symbol', '§'),
      insert('trademark', 'Trademark Symbol', '™'),
      '<',
      '>Markers',
      special('currentPageNumber', 'Current Page Number'),
      special('nextPageNumber', 'Next Page Number'),
      special('previousPageNumber', 'Previous Page Number'),
      special('sectionMarker', 'Section Marker'),
      '<',
      '>Hyphens and Dashes',
      insert('emDash', 'Em Dash', '—'),
      insert('enDash', 'En Dash', '–'),
      insert('discretionaryHyphen', 'Discretionary Hyphen', '­'),
      insert('nonbreakingHyphen', 'Nonbreaking Hyphen', '‑'),
      '<',
      '>Quotation Marks',
      insert('doubleLeft', 'Double Left Quotation Marks', '“'),
      insert('doubleRight', 'Double Right Quotation Marks', '”'),
      insert('singleLeft', 'Single Left Quotation Mark', '‘'),
      insert('singleRight', 'Single Right Quotation Mark', '’'),
      insert('straightDouble', 'Straight Double Quotation Marks', '"'),
      insert('straightSingle', 'Straight Single Quotation Mark (Apostrophe)', "'"),
      '<',
      '>Other',
      insert('tab', 'Tab', '\t'),
      special('rightIndentTab', 'Right Indent Tab'),
      special('indentToHere', 'Indent to Here'),
      '<',
      '<',
      '>Insert White Space',
      insert('emSpace', 'Em Space', ' '),
      insert('enSpace', 'En Space', ' '),
      insert('nonbreakingSpace', 'Nonbreaking Space', ' '),
      insert('hairSpace', 'Hair Space', ' '),
      insert('sixthSpace', 'Sixth Space', ' '),
      insert('thinSpace', 'Thin Space', ' '),
      insert('quarterSpace', 'Quarter Space', ' '),
      insert('thirdSpace', 'Third Space', ' '),
      insert('punctuationSpace', 'Punctuation Space', ' '),
      insert('figureSpace', 'Figure Space', ' '),
      insert('flushSpace', 'Flush Space', ' '),
      '<',
      '>Insert Break Character',
      special('columnBreak', 'Column Break'),
      special('frameBreak', 'Frame Break'),
      special('pageBreak', 'Page Break'),
      special('oddPageBreak', 'Odd Page Break'),
      special('evenPageBreak', 'Even Page Break'),
      insert('paragraphReturn', 'Paragraph Return', '\n'),
      special('forcedLineBreak', 'Forced Line Break'),
      '<',
      c('text.fillWithPlaceholder'),
      '-',
      c('math.insert'),
      '>Story Column Direction',
      ...each('story.setDirection', 'direction', [
        ['leftToRight', 'leftToRight', 'Left to Right'],
        ['rightToLeft', 'rightToLeft', 'Right to Left'],
      ]),
      '<',
    ],
  ],
  [
    'Object',
    [
      '>Transform',
      c('transform.move'),
      c('transform.scale'),
      c('transform.rotate'),
      c('transform.shear'),
      '-',
      v('transform.rotate', 'cw90', 'Rotate 90° Clockwise', { angle: -90 }),
      v('transform.rotate', 'ccw90', 'Rotate 90° Counterclockwise', { angle: 90 }),
      v('transform.rotate', '180', 'Rotate 180°', { angle: 180 }),
      '-',
      ...each('transform.flip', 'axis', [
        ['horizontal', 'horizontal', 'Flip Horizontal'],
        ['vertical', 'vertical', 'Flip Vertical'],
      ]),
      '-',
      c('transform.clear'),
      '<',
      '>Transform Again',
      c('transform.again'),
      v('transform.again', 'individually', 'Transform Again Individually', { individually: true }),
      v('transform.again', 'sequence', 'Transform Sequence Again', { sequence: true }),
      v('transform.again', 'sequenceIndividually', 'Transform Sequence Again Individually', { sequence: true, individually: true }),
      '<',
      '>Arrange',
      c('arrange.bringToFront'),
      c('arrange.bringForward'),
      c('arrange.sendBackward'),
      c('arrange.sendToBack'),
      '<',
      '>Align',
      ...each('object.align', 'edge', [
        ['left', 'left', 'Align Left Edges'],
        ['hcenter', 'hcenter', 'Align Horizontal Centers'],
        ['right', 'right', 'Align Right Edges'],
        ['top', 'top', 'Align Top Edges'],
        ['vcenter', 'vcenter', 'Align Vertical Centers'],
        ['bottom', 'bottom', 'Align Bottom Edges'],
      ]),
      '-',
      v('object.distribute', 'horizontalCenters', 'Distribute Horizontal Centers', { axis: 'horizontal', by: 'centers' }),
      v('object.distribute', 'verticalCenters', 'Distribute Vertical Centers', { axis: 'vertical', by: 'centers' }),
      v('object.distribute', 'horizontalSpace', 'Distribute Horizontal Space', { axis: 'horizontal', by: 'spacing' }),
      v('object.distribute', 'verticalSpace', 'Distribute Vertical Space', { axis: 'vertical', by: 'spacing' }),
      '<',
      '>Select',
      c('select.firstAbove'),
      c('select.nextAbove'),
      c('select.nextBelow'),
      c('select.lastBelow'),
      '-',
      c('select.container'),
      c('select.content'),
      '-',
      c('select.previousInGroup'),
      c('select.nextInGroup'),
      '<',
      '-',
      c('arrange.group'),
      c('arrange.ungroup'),
      c('object.lock'),
      c('object.unlockAll'),
      c('object.hide'),
      c('object.showAll'),
      '-',
      c('object.qrCode'),
      '-',
      c('object.textFrameOptions'),
      c('object.primaryTextFrame'),
      '-',
      '>Fitting',
      v('object.fit', 'fillProportionally', 'Fill Frame Proportionally', { mode: 'fillProportionally' }),
      v('object.fit', 'fitProportionally', 'Fit Content Proportionally', { mode: 'fitProportionally' }),
      '-',
      v('object.fit', 'fitFrameToContent', 'Fit Frame to Content', { mode: 'fitFrameToContent' }),
      v('object.fit', 'fitContentToFrame', 'Fit Content to Frame', { mode: 'fitContentToFrame' }),
      v('object.fit', 'centerContent', 'Center Content', { mode: 'centerContent' }),
      c('frame.fitText'),
      '-',
      c('object.fittingOptions'),
      '<',
      '>Content',
      ...each('object.content', 'type', [
        ['graphic', 'graphic', 'Graphic'],
        ['text', 'text', 'Text'],
        ['unassigned', 'unassigned', 'Unassigned'],
      ]),
      '<',
      '>Text Wrap',
      ...each('object.textWrap', 'mode', [
        ['none', 'none', 'No Text Wrap'],
        ['boundingBox', 'boundingBox', 'Wrap around Bounding Box'],
        ['contour', 'contour', 'Wrap around Object Shape'],
        ['jumpObject', 'jumpObject', 'Jump Object'],
        ['jumpToNextColumn', 'jumpToNextColumn', 'Jump to Next Column'],
      ]),
      '-',
      c('object.textWrap', 'Text Wrap Options'),
      '<',
      '-',
      '>Effects',
      c('object.dropShadow'),
      c('object.innerShadow'),
      c('object.outerGlow'),
      c('object.innerGlow'),
      c('object.bevel'),
      c('object.satin'),
      c('object.feather'),
      c('object.directionalFeather'),
      c('object.gradientFeather'),
      '-',
      c('object.globalLight'),
      '<',
      c('object.cornerOptions'),
      '-',
      c('object.exportOptions'),
      '>Captions',
      c('object.caption'),
      '<',
      '>Clipping Path',
      ...each('object.clippingPath', 'type', [
        ['alpha', 'alpha', 'Alpha Channel'],
        ['edges', 'edges', 'Detect Edges'],
      ]),
      '<',
      '-',
      '>Paths',
      v('object.convertShape', 'openPath', 'Open Path', { to: 'openPath' }),
      v('object.convertShape', 'closedPath', 'Close Path', { to: 'closedPath' }),
      '-',
      c('object.makeCompoundPath'),
      c('object.releaseCompoundPath'),
      '<',
      '>Pathfinder',
      ...each('object.pathfinder', 'op', [
        ['add', 'add', 'Add'],
        ['subtract', 'subtract', 'Subtract'],
        ['intersect', 'intersect', 'Intersect'],
        ['exclude', 'exclude', 'Exclude Overlap'],
        ['minusBack', 'minusBack', 'Minus Back'],
      ]),
      '<',
      '>Convert Shape',
      ...each('object.convertShape', 'to', [
        ['rectangle', 'rectangle', 'Rectangle'],
        ['roundedRectangle', 'roundedRectangle', 'Rounded Rectangle'],
        ['beveledRectangle', 'beveledRectangle', 'Beveled Rectangle'],
        ['inverseRoundedRectangle', 'inverseRoundedRectangle', 'Inverse Rounded Rectangle'],
        ['ellipse', 'ellipse', 'Ellipse'],
        ['triangle', 'triangle', 'Triangle'],
        ['polygon', 'polygon', 'Polygon'],
        ['line', 'line', 'Line'],
        ['orthogonalLine', 'orthogonalLine', 'Orthogonal Line'],
      ]),
      '<',
    ],
  ],
  [
    'Table',
    [
      c('table.insert', 'Insert Table'),
      c('table.convertFromText'),
      c('table.convertToText'),
      '-',
      '>Insert',
      c('table.insertRowAbove'),
      c('table.insertRowBelow'),
      c('table.insertColumnLeft'),
      c('table.insertColumnRight'),
      '<',
      '>Delete',
      c('table.deleteRow'),
      c('table.deleteColumn'),
      c('table.delete'),
      '<',
      '>Select',
      c('table.selectRow'),
      c('table.selectColumn'),
      c('table.selectTable'),
      '<',
      '-',
      c('table.merge'),
      c('table.unmerge'),
      c('table.splitHorizontally'),
      c('table.splitVertically'),
      c('table.textCell'),
      '-',
      c('table.distributeRows'),
      c('table.distributeColumns'),
      c('table.sortRows'),
    ],
  ],
  [
    'View',
    [
      // The view's own actions (plan stage 2), each with its key (`view.ts`).
      ...VIEW_ACTIONS.slice(0, 6).map(([action, label, shortcut]) => k(`view.${action}`, label, shortcut)),
      '-',
      ...VIEW_ACTIONS.slice(6).map(([action, label, shortcut]) => k(`view.${action}`, label, shortcut)),
      '-',
      k('view.rulers', 'Rulers'),
      c('view.showTextThreads'),
      '-',
      c('guide.deleteAll'),
    ],
  ],
  ['Help', [c('file.newSample')]],
]

/** The menu bar's menus, in order. */
export const MENU_ORDER = MENUS.map(([menu]) => menu)

/** A catalog command (engine id, variant or `kit:` row) as the engine command it runs, or null for a `kit:` row. */
export function engineCommandOf(command: string): string | null {
  if (command.startsWith('kit:')) return null
  const at = command.indexOf(VARIANT)
  return at < 0 ? command : command.slice(0, at)
}

/** Every variant row's command and values, by its catalog command (`object.align|left`). */
export const VARIANTS: ReadonlyMap<string, Variant> = new Map(
  MENUS.flatMap(([, entries]) => entries.flatMap((entry) => (typeof entry === 'object' && 'variant' in entry ? [[`${entry.cmd}${VARIANT}${entry.variant}`, { command: entry.cmd, params: entry.params }] as const] : []))),
)

/** Every variant row's English label, by its catalog command (`object.align|left`: "Align Left Edges"). */
export const VARIANT_LABELS: ReadonlyMap<string, string> = new Map(
  MENUS.flatMap(([, entries]) => entries.flatMap((entry) => (typeof entry === 'object' && 'variant' in entry ? [[`${entry.cmd}${VARIANT}${entry.variant}`, entry.label] as const] : []))),
)

/** The engine commands the catalog lists (each variant's too). */
export const CATALOG_COMMANDS: ReadonlySet<string> = new Set(MENUS.flatMap(([, entries]) => entries.flatMap((entry) => (typeof entry === 'object' && 'cmd' in entry ? [entry.cmd] : []))))

/** The engine's commands as the catalog reads them. */
export type CatalogCommand = { id: string; title: string }

/**
 * The menus as catalog rows for this engine: a row whose command the engine lacks is left out; an
 * engine row's label is its title (or the catalog's), with "…" when it asks first (`asks`).
 */
export function layoutMenus(commands: readonly CatalogCommand[], asks: (command: string) => boolean): MenuRow[] {
  const byId = new Map(commands.map((command) => [command.id, command]))
  const rows: MenuRow[] = []
  for (const [menu, entries] of MENUS) {
    const path: string[] = [menu]
    for (const entry of entries) {
      if (entry === '-') rows.push({ path: [...path], label: '---', shortcut: null, command: '---' })
      else if (entry === '<') path.pop()
      else if (typeof entry === 'string') path.push(entry.slice(1))
      else if ('kit' in entry) rows.push({ path: [...path], label: entry.label, shortcut: entry.shortcut ?? null, command: `kit:${entry.kit}` })
      else if (!byId.has(entry.cmd)) continue
      else if ('variant' in entry) rows.push({ path: [...path], label: entry.label, shortcut: null, command: `${entry.cmd}${VARIANT}${entry.variant}` })
      else {
        const label = entry.label ?? byId.get(entry.cmd)?.title ?? entry.cmd
        rows.push({ path: [...path], label: asks(entry.cmd) ? `${label}…` : label, shortcut: null, command: entry.cmd })
      }
    }
  }
  return rows
}
