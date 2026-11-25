/**
 * The drawn cursors (plan 2026-11-25, stage 2): the engine names one CSS cursor for several of
 * its own, so the editor tells them apart by the active tool and draws them as SVG images (its
 * own drawings, no asset files). Pure, so it is tested in Node.
 */

/** A CSS cursor from an SVG drawing (24 by 24, the editor's own), its hot spot, and the cursor where it does not load. */
function drawn(svg: string, x: number, y: number, fallback: string) {
  return `url("data:image/svg+xml,${svg.replace(/</g, '%3C').replace(/>/g, '%3E')}") ${x} ${y}, ${fallback}`
}

const ARROW = "<path d='M2 2v13l3.5-3.5 2.5 5 2-0.9-2.4-4.9H12z' fill='black' stroke='white' stroke-width='1' stroke-linejoin='round'/><rect x='12.5' y='12.5' width='10' height='10' fill='white' stroke='black'/>"
const svg = (body: string) => `<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24'>${body}</svg>`

/** The drawn cursors: rotate, the Direct Selection tool's hollow arrow, the eyedropper, and the loaded text and place cursors. */
export const DRAWN_CURSORS = {
  rotate: drawn(svg("<path d='M6 15a7 7 0 1 0 2.5-9' fill='none' stroke='white' stroke-width='4'/><path d='M6 15a7 7 0 1 0 2.5-9' fill='none' stroke='black' stroke-width='1.6'/><path d='M3.5 3.5l7 1.5-4.5 4.5z' fill='black' stroke='white' stroke-width='1'/>"), 12, 12, 'grab'),
  hollowArrow: drawn(svg("<path d='M3 2v17l4.5-4.5 3 6.5 2.6-1.1-3-6.4H16z' fill='white' stroke='black' stroke-width='1.2' stroke-linejoin='round'/>"), 3, 2, 'default'),
  eyedropper: drawn(svg("<path d='M3 21l1.5-4.5 9-9 3 3-9 9z' fill='white' stroke='black' stroke-width='1.2' stroke-linejoin='round'/><path d='M12.5 6.5l2.5-2.5a2.1 2.1 0 0 1 3 0l2 2a2.1 2.1 0 0 1 0 3l-2.5 2.5z' fill='black' stroke='white' stroke-width='0.8'/>"), 3, 21, 'copy'),
  loadedText: drawn(svg(`${ARROW}<path d='M14.5 15.5h6M14.5 17.5h6M14.5 19.5h4' stroke='black'/>`), 2, 2, 'copy'),
  /** The loaded text cursor over a frame the text can flow into: a chain link. */
  threadLink: drawn(svg(`${ARROW}<path d='M14.5 18.5a2 2 0 0 1 2-2h1.5M21.5 18.5a2 2 0 0 1-2 2H18' fill='none' stroke='black' stroke-width='1.3'/><path d='M16 18.5h4' stroke='black' stroke-width='1.3'/>`), 2, 2, 'alias'),
  /** The loaded text cursor over the frame already linked on its side: a broken link (a click unthreads). */
  unthread: drawn(svg(`${ARROW}<path d='M14.5 18.5a2 2 0 0 1 2-2h0.5M21.5 18.5a2 2 0 0 1-2 2H19' fill='none' stroke='black' stroke-width='1.3'/><path d='M18 14.5v1.5M18 21v1.5M15.5 15l1 1M20.5 22l-1-1' stroke='black'/>`), 2, 2, 'no-drop'),
  loadedGraphic: drawn(svg(`${ARROW}<path d='M13.5 21.5l3-4 2 2 1.5-1.5 1.5 3.5z' fill='black'/><circle cx='19.5' cy='15.5' r='1.2' fill='black'/>`), 2, 2, 'copy'),
}

/**
 * The cursor to show for the engine's (`cursor`, a CSS name) with tool `tool` active: the
 * engine names one CSS cursor for several of its own (rotate is `grab`, the eyedropper and both
 * loaded cursors are `copy`), so the editor tells them apart by the tool and draws them. The
 * loaded text cursor shows what a click does (plan stage 5): link a frame (`alias`), break the
 * link to the frame already threaded there (`no-drop`), or nothing (`not-allowed`).
 */
export function drawnCursor(tool: string, cursor: string) {
  if (tool === 'loadedText' && cursor === 'alias') return DRAWN_CURSORS.threadLink
  if (tool === 'loadedText' && cursor === 'no-drop') return DRAWN_CURSORS.unthread
  if (cursor === 'copy') {
    if (tool === 'loadedText') return DRAWN_CURSORS.loadedText
    if (tool === 'placeGun') return DRAWN_CURSORS.loadedGraphic
    if (tool === 'eyedropper') return DRAWN_CURSORS.eyedropper
  }
  // The selection tools show `grab` only outside a corner handle, to rotate (the Hand tool is the editor's).
  if (cursor === 'grab' && (tool === 'selection' || tool === 'directSelection')) return DRAWN_CURSORS.rotate
  if (cursor === 'default' && tool === 'directSelection') return DRAWN_CURSORS.hollowArrow
  return cursor
}
