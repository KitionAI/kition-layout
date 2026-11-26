/**
 * Images and objects by hand (plan 2026-11-25, stage 6), the editor's side: what Escape does with
 * a frame's graphic selected, several graphics loaded into the place cursor at once and the
 * pictures the cursor shows of them, and the editor actions the engine spells as several commands
 * (Swap Fill and Stroke) run as one undo step. Pure where it can be, so it is tested in Node.
 */
import type { DocumentInfo, PlaceQueued } from './layout'

/** Files the place cursor takes: images (text and other files are placed at once). */
export const LOADABLE = /\.(png|jpe?g|gif|webp|bmp|tiff?|svg)$/i

/** What Escape does on the canvas: the graphic selected goes back to its frame, the place cursor drops its front graphic. */
export function escapeAction(info: DocumentInfo | null, tool: string): 'frame' | 'placeSkip' | null {
  if (tool === 'placeGun') return 'placeSkip'
  if (info?.selectionContent && info.selection.length) return 'frame'
  return null
}

/** The place cursor's badge: how many graphics are loaded and the one a click places next, or null when none is. */
export function placeBadge(queue: readonly PlaceQueued[]): { count: number; next: PlaceQueued } | null {
  const next = queue[0]
  return next ? { count: queue.length, next } : null
}

/**
 * `place.load` calls for files dropped or chosen together: each adds to the place cursor, the
 * first replacing what it held unless graphics are loaded already (`adding`).
 */
export function loadRequests(files: ReadonlyArray<{ base64: string; name: string }>, adding: boolean): Array<Record<string, unknown>> {
  return files.map((file, index) => ({ base64: file.base64, name: file.name, ...(adding || index > 0 ? { queue: true } : {}) }))
}

/** The pictures of the graphics loaded in the place cursor, by asset id: object URLs of their files. */
const thumbnails = new Map<number, string>()

/** Keeps a picture of a loaded graphic for the place cursor (its file's bytes). */
export function rememberThumbnail(asset: number, bytes: ArrayBuffer | Uint8Array, name: string) {
  if (typeof URL === 'undefined' || typeof Blob === 'undefined' || thumbnails.has(asset)) return
  const type = /\.svg$/i.test(name) ? 'image/svg+xml' : ''
  const data = bytes instanceof Uint8Array ? bytes.slice() : bytes
  thumbnails.set(asset, URL.createObjectURL(new Blob([data], type ? { type } : {})))
}

/** The picture of a loaded graphic, when the editor loaded it. */
export function thumbnailOf(asset: number): string | null {
  return thumbnails.get(asset) ?? null
}

/** The file a graphic is kept in when it is unembedded: beside the document, under its own name. */
export function unembedPath(documentPath: string, name: string): string {
  const folder = documentPath.includes('/') ? documentPath.slice(0, documentPath.lastIndexOf('/') + 1) : ''
  const leaf = name.split(/[\\/]/).pop()?.trim() || 'image.png'
  return `${folder}${leaf}`
}

/** The picture type the browser draws a link's thumbnail from, by the file's extension; null when it draws none. */
export function thumbnailType(name: string): string | null {
  const extension = /\.([A-Za-z0-9]+)$/.exec(name)?.[1]?.toLowerCase() ?? ''
  const types: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml' }
  return types[extension] ?? null
}

/** Whether the host takes a graphic's file to keep (it writes only what the kit exports: PNG and JPEG). */
export function unembeddable(name: string): boolean {
  return /\.(png|jpe?g)$/i.test(name)
}
