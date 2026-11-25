/**
 * The engine's menu commands Layout's menus leave out, each with its reason (plan 2026-11-25,
 * stage 1): the host's own file commands, what a browser cannot do, rows the editor's own dialogs
 * stand for, and the panels and features not built before the launch (the plan's "not done" list).
 * Agents still run every one of them. `menuRows.test.mjs` checks that every engine command with a
 * menu path is either in the catalog or here.
 */

const HOST_FILE = 'The host opens, saves and closes documents.'
const OWN_DIALOG = 'The editor\'s own dialog stands for it in the menus.'
const LOCAL_FILE = 'Reads or writes a file in a local folder; the host gives files to the editor.'
const NO_OUTPUT = 'Answers bytes the menu has nowhere to put; File › Export… writes the formats the editor exports.'

/** Panels and features not built before the launch, as the plan lists them. */
const NOT_BUILT_PANEL = (panel: string) => `The ${panel} panel is not built before the launch.`

export const HIDDEN: Readonly<Record<string, string>> = {
  // The host's file commands (File › New… is the editor's own New Document dialog).
  'file.new': HOST_FILE,
  'file.open': HOST_FILE,
  'file.save': HOST_FILE,
  'file.saveAs': HOST_FILE,
  'file.close': HOST_FILE,
  'file.revert': HOST_FILE,
  'book.new': NOT_BUILT_PANEL('Book'),
  'library.new': NOT_BUILT_PANEL('Library'),
  'file.saveACopy': LOCAL_FILE,
  'file.package': 'Packaging copies the linked files into a local folder; not on the web.',
  'file.print': 'Printing goes through the operating system; not on the web.',
  'file.importXml': LOCAL_FILE,
  // The editor's own dialogs (File › Export…, File › Document Setup…, the Place button).
  'file.exportPdf': OWN_DIALOG,
  'file.exportEpub': OWN_DIALOG,
  'file.exportFixedEpub': OWN_DIALOG,
  'file.exportHtml': OWN_DIALOG,
  'layout.documentSetup': OWN_DIALOG,
  'file.place': OWN_DIALOG,
  // Exports that answer bytes and write nothing (batch export is not done before the launch).
  'file.printBooklet': NO_OUTPUT,
  'file.exportText': NO_OUTPUT,
  'file.exportXml': NO_OUTPUT,
  'snippet.export': NO_OUTPUT,
  // Edit › Find/Change… has Change All.
  'find.change': 'Edit › Find/Change… has Change All.',
  // Spelling results have nowhere to show yet (a later stage).
  'spelling.check': 'Its results have nowhere to show yet.',
  'preflight.run': 'Its results have nowhere to show yet.',
  // The Links panel works on the link chosen in it.
  'links.relink': 'The Links panel relinks the link chosen in it.',
  'links.relinkFolder': LOCAL_FILE,
  'links.update': 'The Links panel updates its links.',
  'links.embed': 'The Links panel works on the link chosen in it.',
  'links.goTo': 'The Links panel selects the frames of the link chosen in it.',
  // Not built before the launch (the plan's "not done" list).
  'note.new': NOT_BUILT_PANEL('Notes'),
  'note.delete': NOT_BUILT_PANEL('Notes'),
  'note.convertToText': NOT_BUILT_PANEL('Notes'),
  'changes.track': NOT_BUILT_PANEL('Track Changes'),
  'changes.acceptAll': NOT_BUILT_PANEL('Track Changes'),
  'changes.rejectAll': NOT_BUILT_PANEL('Track Changes'),
  'condition.new': NOT_BUILT_PANEL('Conditional Text'),
  'hyperlink.create': NOT_BUILT_PANEL('Hyperlinks'),
  'anchor.create': NOT_BUILT_PANEL('Hyperlinks'),
  'article.new': NOT_BUILT_PANEL('Articles'),
  'object.attributes': NOT_BUILT_PANEL('Attributes'),
  'object.label': NOT_BUILT_PANEL('Script Label'),
  'xml.newTag': NOT_BUILT_PANEL('Tags'),
  'xml.loadDtd': NOT_BUILT_PANEL('Tags'),
  'states.create': NOT_BUILT_PANEL('Object States'),
  'button.set': NOT_BUILT_PANEL('Buttons and Forms'),
  'button.clear': NOT_BUILT_PANEL('Buttons and Forms'),
  'form.set': NOT_BUILT_PANEL('Buttons and Forms'),
  'story.placeAndLink': 'Needs a story and a place on the page chosen together; not built yet.',
  'object.layerOptions': 'Needs the layer names of a placed PDF; not built yet.',
  'anchored.insert': 'Anchoring an object into text is not built in the editor yet.',
  'anchored.options': 'Anchoring an object into text is not built in the editor yet.',
  'anchored.release': 'Anchoring an object into text is not built in the editor yet.',
  'table.placeGraphic': 'Placing a file into a cell is not built yet.',
  // Two rows with one title: Object Export Options… asks for the alternative text too.
  'object.altText': 'Object › Object Export Options… sets the alternative text.',
  // Object › Arrange lists its four rows (Bring to Front, Bring Forward, …).
  'object.arrange': 'Object › Arrange has a row for each way to arrange.',
  'strokeStyle.new': 'A stroke style needs its dashes or stripes drawn; the stroke style editor is not built before the launch.',
  // Object › Paths › Close Path is Convert Shape's closed path, which takes the selection.
  'path.close': 'Object › Paths › Close Path closes the selected path.',
}

/** Why `command` is not in the menus, or null when it is. */
export function hiddenReason(command: string): string | null {
  return HIDDEN[command] ?? null
}
