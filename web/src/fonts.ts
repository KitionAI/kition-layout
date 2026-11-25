/**
 * Fonts beyond the engine's own (plan 2026-11-25, stage 4). The engine sets text only in faces it
 * has: those it embeds, the package's, and faces the editor gives it (`addFontFile`). A person
 * adds a face two ways: a font installed on the computer, through the browser's Local Font Access
 * API (`queryLocalFonts`, Chromium browsers only, after the person allows it, and only where the
 * host lets the editor's frame ask), or a font file (TTF, OTF, TTC) the host picks. Either is
 * for this editor session: the document names the family, it does not carry the font.
 *
 * The font menu shows each family in its own face: the browser draws a family it knows (installed
 * ones, and the faces registered here from the package and from what the person added).
 */

/** A family installed on the computer, as the browser lists it. */
export type LocalFamily = { family: string; faces: LocalFace[] }
type LocalFace = { family: string; style: string; fullName: string; postscriptName: string; blob: () => Promise<Blob> }
type LocalFontWindow = Window & { queryLocalFonts?: () => Promise<LocalFace[]> }

/** Whether this browser can list the computer's fonts (it still asks the person first). */
export function canListLocalFonts() {
  return typeof window !== 'undefined' && typeof (window as LocalFontWindow).queryLocalFonts === 'function'
}

/** The computer's font families (the browser asks the person first; it must follow a click). */
export async function listLocalFonts(): Promise<LocalFamily[]> {
  const query = (window as LocalFontWindow).queryLocalFonts
  if (!query) return []
  const faces = await query.call(window)
  const byFamily = new Map<string, LocalFace[]>()
  for (const face of faces) {
    const list = byFamily.get(face.family) ?? []
    list.push(face)
    byFamily.set(face.family, list)
  }
  return [...byFamily.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([family, list]) => ({ family, faces: list }))
}

/** The font files of a local family, each face's bytes (a face the browser cannot read is left out). */
export async function localFamilyFiles(family: LocalFamily): Promise<Uint8Array[]> {
  const files: Uint8Array[] = []
  for (const face of family.faces) {
    try {
      files.push(new Uint8Array(await (await face.blob()).arrayBuffer()))
    } catch {
      // A face the browser will not hand over (a system font it protects).
    }
  }
  return files
}

/** The package's faces and the engine's own document faces, by family, for the menu's previews. */
const PREVIEW_FACES: Readonly<Record<string, string>> = {
  Inter: 'Inter-Variable.ttf',
  Lora: 'Lora-Variable.ttf',
  'JetBrains Mono': 'JetBrainsMono-Variable.ttf',
  'Bricolage Grotesque': 'BricolageGrotesque-Variable.ttf',
  'Source Serif 4': 'SourceSerif4-Regular.ttf',
  'Source Sans 3': 'SourceSans3-Regular.ttf',
}

const registered = new Set<string>()

/** The CSS family the menu draws `family` in: its own face once loaded, the UI's font until then. */
export function previewFamily(family: string) {
  return `"kition-preview-${family.replace(/["\\]/g, '')}", "${family.replace(/["\\]/g, '')}", var(--kit-font-sans, sans-serif)`
}

/** Starts loading the faces the menu shows `families` in (once each); the browser draws them when they arrive. */
export function loadPreviews(families: readonly string[]) {
  if (typeof document === 'undefined' || typeof FontFace === 'undefined') return
  for (const family of families) {
    const file = PREVIEW_FACES[family]
    if (!file || registered.has(family)) continue
    registered.add(family)
    const face = new FontFace(`kition-preview-${family}`, `url(${new URL(`./fonts/${file}`, document.baseURI).href})`)
    document.fonts.add(face)
    void face.load().catch(() => undefined)
  }
}

/** Registers a face the person added (its bytes) so the menu draws its family in it. */
export function registerPreview(family: string, bytes: Uint8Array) {
  if (typeof document === 'undefined' || typeof FontFace === 'undefined' || registered.has(family)) return
  registered.add(family)
  try {
    const face = new FontFace(`kition-preview-${family}`, bytes.slice().buffer)
    document.fonts.add(face)
    void face.load().catch(() => undefined)
  } catch {
    // Not a face the browser can draw: the menu shows the family in the UI's font.
  }
}

/** The family names in a font file's name table (ID 1, or 16 when present), for a file the person loaded. */
export function fontFamilies(bytes: Uint8Array): string[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const u16 = (at: number) => (at + 2 <= view.byteLength ? view.getUint16(at) : 0)
  const u32 = (at: number) => (at + 4 <= view.byteLength ? view.getUint32(at) : 0)
  const fonts: number[] = []
  if (u32(0) === 0x74746366) {
    // A collection (`ttcf`): each font's offset table.
    const count = Math.min(u32(8), 64)
    for (let i = 0; i < count; i += 1) fonts.push(u32(12 + i * 4))
  } else fonts.push(0)
  const names = new Set<string>()
  for (const start of fonts) {
    const tables = Math.min(u16(start + 4), 256)
    for (let i = 0; i < tables; i += 1) {
      const record = start + 12 + i * 16
      if (u32(record) !== 0x6e616d65) continue
      const table = u32(record + 8)
      const count = Math.min(u16(table + 2), 1024)
      const strings = table + u16(table + 4)
      const found = new Map<number, string>()
      for (let n = 0; n < count; n += 1) {
        const at = table + 6 + n * 12
        const platform = u16(at)
        const id = u16(at + 6)
        if (id !== 1 && id !== 16) continue
        const length = u16(at + 8)
        const offset = strings + u16(at + 10)
        if (offset + length > view.byteLength) continue
        const raw = bytes.subarray(offset, offset + length)
        const text = platform === 3 || platform === 0 ? new TextDecoder('utf-16be').decode(raw) : new TextDecoder('latin1').decode(raw)
        if (text && (!found.has(id) || platform === 3)) found.set(id, text)
      }
      const name = found.get(16) ?? found.get(1)
      if (name) names.add(name.trim())
    }
  }
  return [...names]
}
