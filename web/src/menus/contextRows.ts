/**
 * The right-click menus (plan 2026-11-25, stage 2): what each place offers, as catalog commands
 * (engine ids and variant rows such as `transform.rotate|cw90`, run through the dialog host so a
 * row that asks opens its dialog) and the editor's own `kit:` actions (fit the page, show the
 * rulers). Pure, so it is tested in Node; `ContextMenus.tsx` draws them.
 */

/** Where the person right-clicked. */
export type Spot =
  | { kind: 'object'; textFrame: boolean; image: boolean; group: boolean; count: number }
  | { kind: 'text' }
  | { kind: 'canvas' }
  | { kind: 'ruler'; spread: number }
  | { kind: 'page'; page: number }
  | { kind: 'layer'; layer: number }

/** One row: a command (with values of its own), or a separator. */
export type ContextRow = { id: string; params?: Record<string, unknown> } | '-'

const row = (id: string, params?: Record<string, unknown>): ContextRow => (params ? { id, params } : { id })
const rows = (...ids: string[]): ContextRow[] => ids.map((id) => (id === '-' ? '-' : row(id)))

/** The rows of the menu at `spot`; separators never lead, trail or come twice. */
export function contextRows(spot: Spot): ContextRow[] {
  const out: ContextRow[] = (() => {
    switch (spot.kind) {
      case 'object':
        return [
          ...rows('edit.cut', 'edit.copy', 'edit.paste', 'edit.pasteInto', 'edit.pasteInPlace', '-'),
          ...rows('transform.move', 'transform.scale', 'transform.rotate', 'transform.rotate|cw90', 'transform.rotate|ccw90', 'transform.flip|horizontal', 'transform.flip|vertical', '-'),
          ...rows('object.bringToFront', 'object.bringForward', 'object.sendBackward', 'object.sendToBack', '-'),
          ...(spot.image ? rows('object.fit|fillProportionally', 'object.fit|fitProportionally', 'object.fit|fitFrameToContent', 'object.fit|centerContent', '-') : []),
          ...(spot.textFrame ? rows('object.textFrameOptions') : []),
          ...rows('object.dropShadow', 'object.cornerOptions', '-'),
          ...rows('object.lock', spot.group ? 'object.ungroup' : spot.count > 1 ? 'object.group' : '-', 'edit.clear'),
        ]
      case 'text':
        return rows(
          'edit.cut',
          'edit.copy',
          'edit.paste',
          'edit.pasteWithoutFormatting',
          '-',
          'edit.selectAll',
          '-',
          'text.insert|bullet',
          'text.insert|emDash',
          'text.insert|nonbreakingSpace',
          'text.insertSpecial|currentPageNumber',
          'text.insertSpecial|forcedLineBreak',
          '-',
          'object.textFrameOptions',
        )
      case 'canvas':
        return rows('edit.paste', 'edit.pasteInPlace', '-', 'kit:view.fitPage', 'kit:view.fitSpread', 'kit:view.actualSize', 'kit:view.entirePasteboard', '-', 'kit:view.rulers', 'view.showTextThreads', 'guide.deleteAll')
      case 'ruler':
        return [row('kit:view.rulers'), row('guide.deleteAll', { spread: spot.spread })]
      case 'page':
        return rows('layout.pages.insert', 'layout.pages.move', 'layout.pages.duplicateSpread', 'layout.pages.delete', '-', 'layout.pages.applyParent', 'layout.marginsAndColumns', '-', 'kit:view.fitPage', 'kit:view.fitSpread')
      case 'layer':
        return [row('layer.new'), row('layer.selectItems', { id: spot.layer }), '-', row('layer.delete', { id: spot.layer }), row('layer.deleteUnused')]
    }
  })()
  return out.filter((entry, index) => entry !== '-' || (index > 0 && index < out.length - 1 && out[index - 1] !== '-'))
}
