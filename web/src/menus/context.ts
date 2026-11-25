/**
 * What a menu row needs before it can run, and why it is greyed out (plan 2026-11-25, stage 1).
 * The engine greys out what it cannot run at all; these rules grey out the rows it would refuse
 * with an English error (Group with one object, Distribute with two), and every greyed-out row
 * says why when the pointer rests on it (`hint.*` in `strings.ts`). Pure, so it is tested in Node.
 */

/** The selection as the rules read it: the selected items' kinds and whether each holds a placed graphic. */
export type MenuContext = { items: ReadonlyArray<{ kind: string; image: boolean; story: boolean }>; editingText: boolean }

/** A reason's key in the editor's strings (`hint.<key>`). */
export type Reason = 'needTwo' | 'needThree' | 'needTwoShapes' | 'needImage' | 'needTypeOnPath' | 'needSelection' | 'needCaret' | 'needTextSelection' | 'needTable' | 'needClipboard' | 'nothingToUndo' | 'nothingToRedo' | 'needToc' | 'needIndex' | 'needFootnote' | 'needTransform' | 'needTextFrame' | 'needEmptyFrame' | 'needStoryText' | 'nothingToTrace' | 'textFrameShape' | 'noOverlap' | 'notNow'

/** Why `command` (an engine id) cannot run in `context` though the engine would try, or null. */
export function contextReason(command: string, context: MenuContext): Reason | null {
  const count = context.items.length
  switch (command) {
    case 'object.group':
      return count < 2 ? 'needTwo' : null
    case 'object.makeCompoundPath':
    case 'object.pathfinder':
      return count < 2 ? 'needTwoShapes' : null
    case 'object.distribute':
      return count < 3 ? 'needThree' : null
    case 'object.clippingPath':
      return context.items.some((item) => item.image) ? null : 'needImage'
    case 'type.pathOptions':
      return context.items.some((item) => item.kind === '<text path>') ? null : 'needTypeOnPath'
    case 'object.primaryTextFrame':
      return context.items.some((item) => item.kind === '<text frame>') ? null : 'needTextFrame'
    case 'story.setDirection':
      return context.editingText || context.items.some((item) => item.story) ? null : 'needTextFrame'
    case 'object.qrCode':
      return count === 1 && !context.items[0].story && !context.items[0].image ? null : 'needEmptyFrame'
    default:
      return null
  }
}

/** Why the engine greys out `command` (an engine id), worded for the person. */
export function disabledReason(command: string): Reason {
  if (command === 'edit.undo') return 'nothingToUndo'
  if (command === 'edit.redo') return 'nothingToRedo'
  if (command === 'toc.update') return 'needToc'
  if (command === 'index.update') return 'needIndex'
  if (command === 'footnote.goToReference') return 'needFootnote'
  if (command === 'transform.again') return 'needTransform'
  if (command === 'table.insert' || command === 'table.convertFromText') return command === 'table.insert' ? 'needCaret' : 'needTextSelection'
  if (command.startsWith('table.')) return 'needTable'
  if (command.startsWith('edit.paste')) return 'needClipboard'
  if (['footnote.insert', 'endnote.insert', 'text.insert', 'text.insertSpecial', 'xref.insert', 'index.addReference', 'variables.insert', 'math.insert', 'type.changeCase', 'type.char', 'story.setDirection'].includes(command)) return 'needCaret'
  if (/^(object|transform|select|path|frame)\./.test(command) || ['edit.duplicate', 'edit.cut', 'edit.copy', 'edit.clear', 'edit.stepAndRepeat', 'type.createOutlines'].includes(command)) return 'needSelection'
  return 'notNow'
}

/** The engine's refusals a person can meet from a menu, as the reason to show instead of the English text. */
const ENGINE_REFUSALS: ReadonlyArray<readonly [RegExp, Reason]> = [
  [/no transform to repeat/, 'needTransform'],
  [/clipboard is empty/, 'needClipboard'],
  [/select at least two objects/, 'needTwo'],
  [/select at least three objects/, 'needThree'],
  [/select two or more (paths|shapes)/, 'needTwoShapes'],
  [/select a frame with a placed graphic/, 'needImage'],
  [/not type on a path/, 'needTypeOnPath'],
  [/not in a table|insertion point is not in a table/, 'needTable'],
  [/go in story text, not tables/, 'needStoryText'],
  [/nothing to trace/, 'nothingToTrace'],
  [/a text frame (needs a closed path|can't become a line)/, 'textFrameShape'],
  [/shapes don't overlap/, 'noOverlap'],
  [/select a text frame/, 'needTextFrame'],
]

/** The reason behind an engine error a menu row can meet, or null for any other error. */
export function refusalReason(error: unknown): Reason | null {
  const text = error instanceof Error ? error.message : String(error)
  return ENGINE_REFUSALS.find(([pattern]) => pattern.test(text))?.[1] ?? null
}

/** Where the person is: the page in view (0-based) and its spread. */
export type Place = { page: number; spread: number }

/** Values a command takes from where the person is, not from a dialog: the spread or page in view. */
export function placeParams(command: string, place: Place): Record<string, unknown> {
  switch (command) {
    case 'layout.pages.duplicateSpread':
    case 'guide.deleteAll':
    case 'layout.spreadShuffle':
      return { spread: place.spread }
    case 'layout.overrideParentItems':
    case 'layout.removeOverrides':
    case 'layout.detachAll':
      return { page: place.page }
    default:
      return {}
  }
}

/** A dialog's starting values from where the person is (the page in view, counted from 1 where the command counts so). */
export function placeValues(command: string, place: Place): Record<string, unknown> {
  switch (command) {
    case 'layout.section':
    case 'layout.pages.toSpread':
      return { page: place.page + 1 }
    case 'layout.pages.applyParent':
      return { pages: [place.page + 1] }
    default:
      return {}
  }
}
