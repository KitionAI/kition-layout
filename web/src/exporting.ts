/**
 * Export's file handling, apart from the dialog: what the engine is asked for, the names of the
 * files beside the document, and writing them through the host (`host/write`, mode `export`,
 * which Kition allows only for the extensions `kit.json` `exports` declares). Nothing here reaches
 * the engine or the host, so it is tested on its own (`node --test src/exporting.test.mjs`).
 */

export const FORMATS = ['png', 'jpeg', 'pdf', 'idml', 'epub', 'html'] as const
export type Format = (typeof FORMATS)[number]

/** Each format's file extension, as `kit.json` `exports[].extension` declares it. */
export const EXTENSION: Record<Format, string> = { png: 'png', jpeg: 'jpg', pdf: 'pdf', idml: 'idml', epub: 'epub', html: 'html' }

/** PDF/X-4 and PDF/A-2b, or none: the standards the engine's PDF export writes. */
export const PDF_STANDARDS = ['none', 'x4', 'a2b'] as const
export type PdfStandard = (typeof PDF_STANDARDS)[number]

/**
 * The PDF options the engine's export takes besides pages and bleed (plan stage 8): each spread on
 * one sheet, printer's marks (crop, bleed, page information), a standard, images compressed as
 * JPEG, a tagged (accessible) file. Nothing the exporter ignores.
 */
export type PdfOptions = { spreads: boolean; cropMarks: boolean; bleedMarks: boolean; pageInfo: boolean; standard: PdfStandard; compressImages: boolean; tagged: boolean }

export const DEFAULT_PDF: PdfOptions = { spreads: false, cropMarks: false, bleedMarks: false, pageInfo: false, standard: 'none', compressImages: false, tagged: false }

/** The engine's PDF parameters for `options`: only those that differ from the engine's defaults. */
export function pdfParams(options: PdfOptions): Record<string, unknown> {
  const params: Record<string, unknown> = {}
  if (options.spreads) params.spreads = true
  if (options.cropMarks || options.bleedMarks || options.pageInfo) params.marks = { crop: options.cropMarks, bleed: options.bleedMarks, pageInfo: options.pageInfo }
  if (options.standard !== 'none') params.standard = options.standard
  if (options.compressImages) params.compressImages = true
  if (options.tagged) params.tagged = true
  return params
}

/** A one-file export: `file.exportBytes` answers the file as base64 in `data`. */
export function bytesRequest(format: Format, options: { pages?: string; bleed: boolean; pdf?: PdfOptions }): { command: string; params: Record<string, unknown> } {
  const params: Record<string, unknown> = { format: EXTENSION[format] }
  if (format === 'pdf') Object.assign(params, options.pages ? { pages: options.pages } : {}, { bleed: options.bleed }, options.pdf ? pdfParams(options.pdf) : {})
  return { command: 'file.exportBytes', params }
}

export function decodeBase64(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
}

/** The file in an engine answer: `data` or `base64` (base64), or `text`; null without one. */
export function bytesOf(answer: Record<string, unknown> | null | undefined): Uint8Array | null {
  if (!answer) return null
  if (typeof answer.data === 'string') return decodeBase64(answer.data)
  if (typeof answer.base64 === 'string') return decodeBase64(answer.base64)
  if (typeof answer.text === 'string') return new TextEncoder().encode(answer.text)
  return null
}

/** The document's name without its extension. */
export function exportStem(name: string): string {
  return name.replace(/\.[^.]+$/, '').trim() || 'Untitled'
}

/** The folder of the document's workspace path, with its trailing slash ('' at the top). */
export function exportFolder(path: string): string {
  return path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : ''
}

/** The new file beside the document: `folder/Name.ext`. */
export function exportPath(document: { name: string; path: string }, format: Format, suffix = ''): string {
  return `${exportFolder(document.path)}${exportStem(document.name)}${suffix}.${EXTENSION[format]}`
}

/**
 * Writes each file through the host, in order, and answers the paths the host wrote (it may pick
 * an unused name). The host's refusal ends the export with its own error: nothing is reported
 * written that the host did not answer for.
 */
export async function writeExports(files: ReadonlyArray<{ path: string; bytes: Uint8Array }>, write: (path: string, bytes: Uint8Array) => Promise<string>): Promise<string[]> {
  const written: string[] = []
  for (const file of files) {
    const path = await write(file.path, file.bytes)
    if (typeof path !== 'string' || !path) throw new Error(`the host did not write ${file.path}`)
    written.push(path)
  }
  return written
}

/** The scale a resolution is of 72 ppi, with at most two decimals (300 ppi is 4.17×, not 4.166666666666667×). */
export function scaleLabel(ppi: number) {
  return String(Math.round((ppi / 72) * 100) / 100)
}
