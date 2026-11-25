/**
 * The system clipboard and files dropped on the canvas (plan 2026-11-25, stage 2).
 *
 * Copy and Cut in text put the copied text on the system clipboard as well as the engine's; Paste
 * in text takes the system clipboard's text (the engine keeps its formatting when it is what was
 * copied here), and an image on it is placed as a frame. Out of text, Paste places a copied image,
 * pastes the engine's copied objects, or else makes a text frame of the copied text. The browser
 * hands over the clipboard only to a paste event, so the editor lets the key make one: it stops the
 * frame's own Paste shortcut and, out of text, focuses a hidden field of its own for the moment.
 *
 * A file dropped on the canvas: an image fills the empty frame under it, else lands where it was
 * dropped at its own size; a text file flows into the frame under it, else into a new frame there.
 */
import { useEffect, useRef, type RefObject } from 'react'
import { base64, toast, useKitEditor, useReadOnly, type KitCanvasHandle } from '@kition/sdk/editor'
import { spreadAt, type DocumentInfo, type Item } from './layout'
import { itemAt } from './view'

/** How long a paste waits for the browser's paste event before it pastes the engine's own copy. */
const PASTE_WAIT_MS = 400
/** A new text frame for pasted or dropped text, in points. */
const TEXT_FRAME = { width: 240, height: 120 }

/** Files the canvas takes as text. */
const TEXT_FILE = /\.(txt|md|markdown|rtf|docx)$/i

function isImage(file: File) {
  return file.type.startsWith('image/') || /\.(png|jpe?g|gif|webp|bmp|tiff?|svg)$/i.test(file.name)
}

/** A frame a placed image can fill: empty (or holding an image already). */
function takesImage(item: Item) {
  return item.story == null && (item.asset != null || ['<rectangle>', '<ellipse>', '<polygon>', '<image>'].includes(item.kind))
}

/** Writes `text` (when the engine answers with it) to the system clipboard, within the key's user gesture. */
function writeClipboard(text: Promise<string | null>) {
  const plain = () => void text.then((value) => (value ? navigator.clipboard?.writeText(value) : undefined)).catch(() => undefined)
  if (typeof ClipboardItem === 'undefined' || !navigator.clipboard?.write) return plain()
  const blob = text.then((value) => {
    if (value == null) throw new Error('nothing copied')
    return new Blob([value], { type: 'text/plain' })
  })
  navigator.clipboard.write([new ClipboardItem({ 'text/plain': blob })]).catch(plain)
}

/** The text an `edit.copy` or `edit.cut` answer carries. */
function copiedText(answer: string | null) {
  if (!answer) return null
  try {
    const value = JSON.parse(answer) as { text?: unknown }
    return typeof value.text === 'string' ? value.text : null
  } catch {
    return null
  }
}

/** A field the person types in (the text sink is the editor's, not one). */
function inField(target: EventTarget | null) {
  if (target instanceof HTMLTextAreaElement) return !target.classList.contains('kui-text-sink')
  return target instanceof HTMLInputElement || target instanceof HTMLSelectElement || (target instanceof HTMLElement && target.isContentEditable)
}

type ClipboardOptions = {
  info: DocumentInfo | null
  /** Whether the person is typing in a story (the engine's tool takes text). */
  typing: () => boolean
  canvas: RefObject<KitCanvasHandle | null>
}

/** The canvas point (document pixels) at the middle of the view. */
function viewMiddle(canvas: KitCanvasHandle | null) {
  const view = canvas?.view()
  return view ? { x: view.x + view.width / 2 / view.zoom, y: view.y + view.height / 2 / view.zoom } : null
}

/** Copy, Cut and Paste with the system clipboard (see the module's notes). */
export function useSystemClipboard({ info, typing, canvas }: ClipboardOptions) {
  const editor = useKitEditor()
  const readOnly = useReadOnly()
  const latest = useRef({ editor, info, typing, readOnly })
  latest.current = { editor, info, typing, readOnly }

  useEffect(() => {
    const catcher = document.createElement('textarea')
    catcher.className = 'layout-paste-catcher'
    catcher.setAttribute('aria-hidden', 'true')
    catcher.tabIndex = -1
    document.body.append(catcher)
    let pending: { plain: boolean; typing: boolean; back: HTMLElement | null; timer: number } | null = null

    const run = (command: string, params: Record<string, unknown> = {}) => {
      const { editor: now } = latest.current
      return now.session.run(command, params).catch((error: unknown) => {
        now.report(now.t('commandFailed'), error)
        return null
      })
    }
    /** Pastes the engine's own copy, as the frame's Paste shortcut would have. */
    const engineHasCopy = () => latest.current.editor.enabled('edit.paste')
    const finish = () => {
      if (!pending) return null
      const done = pending
      pending = null
      window.clearTimeout(done.timer)
      if (!done.typing) done.back?.focus({ preventScroll: true })
      return done
    }

    const onKey = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey
      if (!command || event.altKey || event.isComposing) return
      if (!['KeyC', 'KeyX', 'KeyV'].includes(event.code) || inField(event.target)) return
      if (document.querySelector('.kui-dialog-backdrop') || (event.target instanceof Element && event.target.closest('.kui-dialog'))) return
      const { typing: typingNow, readOnly: viewer, info: now } = latest.current
      if (!now) return
      const inText = typingNow() && now.editingText
      if (event.code !== 'KeyV') {
        if (event.shiftKey || !inText) return
        // Copy or Cut the text: the engine's copy, and its text on the system clipboard too.
        if (event.code === 'KeyX' && viewer) return
        event.preventDefault()
        event.stopPropagation()
        writeClipboard(run(event.code === 'KeyX' ? 'edit.cut' : 'edit.copy').then(copiedText))
        return
      }
      if (viewer) return
      // Paste: let the browser make its paste event, with the clipboard, and keep the frame's
      // shortcut out of it.
      event.stopPropagation()
      const back = document.activeElement instanceof HTMLElement ? document.activeElement : null
      if (!inText) catcher.focus({ preventScroll: true })
      const plain = event.shiftKey
      pending = {
        plain,
        typing: inText,
        back,
        // No paste event (a browser that gives none here): the engine's own copy.
        timer: window.setTimeout(() => {
          const done = finish()
          if (done && engineHasCopy()) void run(done.plain && done.typing ? 'edit.pasteWithoutFormatting' : 'edit.paste')
        }, PASTE_WAIT_MS),
      }
    }

    const onPaste = (event: ClipboardEvent) => {
      if (!pending) return
      event.preventDefault()
      event.stopPropagation()
      const done = finish()
      if (!done) return
      const data = event.clipboardData
      const image = [...(data?.files ?? [])].find(isImage)
      const text = data?.getData('text/plain') ?? ''
      if (image) {
        void placeFile(image)
        return
      }
      if (done.typing) {
        void run(done.plain ? 'edit.pasteWithoutFormatting' : 'edit.paste', text ? { text } : {})
        return
      }
      if (engineHasCopy()) {
        void run('edit.paste')
        return
      }
      if (text) void textFrame(text)
    }

    /** An image from the clipboard: into the one selected empty frame, else onto the page in view. */
    const placeFile = async (file: File) => {
      const { info: now } = latest.current
      const bytes = await file.arrayBuffer()
      const frame = now?.selection.length === 1 && !now.editingText ? now.selection[0] : undefined
      await run('file.place', { base64: base64(bytes), name: file.name || 'pasted.png', ...(frame != null ? { frame } : {}) })
    }

    /** Copied text out of text: a new text frame in the middle of the view. */
    const textFrame = async (text: string) => {
      const { info: now } = latest.current
      const middle = viewMiddle(canvas.current)
      const spread = now && middle ? spreadAt(now, middle.x, middle.y) : null
      if (!now || !middle || !spread) return
      const x = middle.x - spread.area.x0 + spread.left - TEXT_FRAME.width / 2
      const y = middle.y - spread.area.y0 - TEXT_FRAME.height / 2
      await run('frame.create', { spread: spread.index, rect: [x, y, x + TEXT_FRAME.width, y + TEXT_FRAME.height], content: 'text', text, caret: false })
    }

    window.addEventListener('keydown', onKey, true)
    window.addEventListener('paste', onPaste, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('paste', onPaste, true)
      if (pending) window.clearTimeout(pending.timer)
      catcher.remove()
    }
  }, [canvas])
}

/** Files dropped on the canvas region (see the module's notes). */
export function useFileDrop(region: RefObject<HTMLElement | null>, info: DocumentInfo | null, canvas: RefObject<KitCanvasHandle | null>) {
  const editor = useKitEditor()
  const readOnly = useReadOnly()
  const latest = useRef({ editor, info, readOnly })
  latest.current = { editor, info, readOnly }
  useEffect(() => {
    const element = region.current
    if (!element) return undefined
    const files = (event: DragEvent) => Boolean(event.dataTransfer?.types.includes('Files'))
    const over = (event: DragEvent) => {
      if (!files(event) || latest.current.readOnly) return
      event.preventDefault()
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
    }
    const drop = (event: DragEvent) => {
      if (!files(event) || latest.current.readOnly) return
      event.preventDefault()
      const file = [...(event.dataTransfer?.files ?? [])].find((item) => isImage(item) || TEXT_FILE.test(item.name))
      const { editor: now, info: doc } = latest.current
      const handle = canvas.current
      const view = handle?.view()
      const surface = element.querySelector('[data-testid="kit-canvas-surface"]')
      if (!file || !doc || !view || !surface) {
        if (event.dataTransfer?.files.length) toast('info', now.t('dropUnsupported'))
        return
      }
      const box = surface.getBoundingClientRect()
      const docX = view.x + (event.clientX - box.left) / view.zoom
      const docY = view.y + (event.clientY - box.top) / view.zoom
      const spread = spreadAt(doc, docX, docY)
      if (!spread) return
      const x = docX - spread.area.x0 + spread.left
      const y = docY - spread.area.y0
      const under = itemAt(doc, spread, x, y)
      void (async () => {
        const source = { base64: base64(await file.arrayBuffer()), name: file.name }
        const run = (command: string, params: Record<string, unknown>) =>
          now.session.run(command, params).catch((error: unknown) => {
            now.report(now.t('commandFailed'), error)
            return null
          })
        if (isImage(file)) {
          if (under && takesImage(under)) await run('file.place', { ...source, frame: under.id })
          else {
            // Not into a frame that happens to be selected: where it was dropped.
            await run('selection.set', { ids: [] })
            await run('file.place', { ...source, spread: spread.index, x, y })
          }
          return
        }
        if (under && (under.story != null || takesImage(under))) {
          await run('file.place', { ...source, frame: under.id })
          return
        }
        const page = spread.pages.find((item) => x >= item.bounds[0] && x <= item.bounds[2]) ?? spread.pages[0]
        await run('selection.set', { ids: [] })
        await run('file.place', { ...source, page: (page?.index ?? 0) + 1, rect: [x, y, x + TEXT_FRAME.width, y + TEXT_FRAME.height] })
      })()
    }
    element.addEventListener('dragover', over)
    element.addEventListener('drop', drop)
    return () => {
      element.removeEventListener('dragover', over)
      element.removeEventListener('drop', drop)
    }
  }, [canvas, region])
}
