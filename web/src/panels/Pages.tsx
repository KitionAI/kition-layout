import { useEffect, useState, type MouseEvent } from 'react'
import { ArrowDownIcon as ArrowDown, ArrowLeftIcon as ArrowLeft, ArrowUpIcon as ArrowUp, CopyIcon as Copy, EllipsisIcon as Ellipsis, PlusIcon as Plus, Settings2Icon as Settings2, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Button, Choice, IconButton, PanelFooter, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import type { DocumentInfo, Spread } from '../layout'
import { useMenuOpener } from '../menus/ContextMenus'
import { useEditorView } from '../viewContext'
import { PanelHeading } from './fields'
import { dropSide, moveTarget } from './reorder'
import { useRowDrag, type DragOver } from './rowDrag'
import { useDialogs } from './dialogsContext'
import { pageHasObjects } from './panelLogic'

/** What a drag in the Pages panel carries: a page (to move it) or a parent (to apply it). */
type Dragged = { kind: 'page'; index: number } | { kind: 'parent'; label: string | null }

const THUMB = 92

/**
 * How long the document stays still before the thumbnails are drawn again. Typing in a long story
 * changes it with every key: the thumbnails wait for a pause, as InDesign's do, so the engine
 * spends its time on the text (plan 2026-11-25, phase 9).
 */
export const THUMB_IDLE_MS = 600

/**
 * The thumbnails are drawn one at a time, each after the document has been still for
 * `THUMB_IDLE_MS`: a change while one waits drops it (the next pause draws it again), so a burst
 * of changes costs no thumbnail at all, and only the thumbnails in view are queued.
 */
const thumbQueue = (() => {
  let tail: Promise<void> = Promise.resolve()
  let changedAt = 0
  return {
    changed() {
      changedAt = performance.now()
    },
    /** Queues `draw` (it runs unless `stale()` by its turn, after a still moment). */
    add(draw: () => Promise<void>, stale: () => boolean) {
      tail = tail.then(async () => {
        for (;;) {
          if (stale()) return
          const wait = changedAt + THUMB_IDLE_MS - performance.now()
          if (wait <= 0) break
          await new Promise((done) => window.setTimeout(done, wait))
        }
        await draw().catch(() => undefined)
      })
    },
  }
})()

/** Whether an element is in view (in its scrolling panel and on the screen). */
function useInView(element: Element | null) {
  const [shown, setShown] = useState(false)
  useEffect(() => {
    if (!element || typeof IntersectionObserver === 'undefined') {
      setShown(Boolean(element))
      return undefined
    }
    const observer = new IntersectionObserver((entries) => setShown(entries.some((entry) => entry.isIntersecting)), { rootMargin: '64px' })
    observer.observe(element)
    return () => observer.disconnect()
  }, [element])
  return shown
}

/**
 * A spread's thumbnail, drawn by the engine once the document is still after a change, and only
 * while it is in view; one scrolled into view after a change is drawn then.
 */
function SpreadThumb({ spread }: { spread: Spread }) {
  const { session } = useKitEditor()
  const { doc, change } = session
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null)
  const shown = useInView(canvas)
  /** The revision this thumbnail shows. */
  const [drawn, setDrawn] = useState<string | null>(null)
  const width = spread.area.x1 - spread.area.x0
  const height = spread.area.y1 - spread.area.y0
  const scale = Math.min(THUMB / Math.max(1, width), THUMB / Math.max(1, height))
  const w = Math.max(1, Math.round(width * scale))
  const h = Math.max(1, Math.round(height * scale))
  const wanted = `${change.revision}:${spread.area.x0}:${spread.area.y0}:${w}:${h}`
  useEffect(() => thumbQueue.changed(), [change.revision])
  useEffect(() => {
    if (!doc || !canvas || !shown || drawn === wanted) return undefined
    let stale = false
    thumbQueue.add(async () => {
      const rgba = await doc.render(spread.area.x0, spread.area.y0, scale, w, h).catch(() => null)
      if (stale || !rgba) return
      canvas.width = w
      canvas.height = h
      canvas.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), w, h), 0, 0)
      setDrawn(wanted)
    }, () => stale)
    return () => {
      stale = true
    }
  }, [canvas, doc, drawn, h, scale, shown, spread.area.x0, spread.area.y0, w, wanted])
  return <canvas ref={setCanvas} width={w} height={h} className="layout-thumb" data-drawn={drawn ?? undefined} />
}

/**
 * InDesign's Pages panel: the parents, then every spread as a thumbnail (click to show it, its
 * page names under it). A page chosen in the panel can be moved, duplicated (its spread), deleted,
 * or given another parent; Document Setup sets the size of every page. Plan stage 7: a page's
 * name dragged before or after another page moves it there; a parent dragged onto a page (or a
 * spread's thumbnail) applies it; a double-click on a page shows it; a double-click on a parent
 * edits it (the canvas shows the parents until Back to Pages); the panel menu (…) has the page
 * rows of the right-click menu.
 */
export function PagesSection({ info, onShow, onSetup }: { info: DocumentInfo; onShow: (spread: Spread) => void; onSetup: () => void }) {
  const { run, phrase: p, t } = useKitEditor()
  const { settle } = useEditorView()
  const readOnly = useReadOnly()
  const [chosen, setChosen] = useState(0)
  const openMenu = useMenuOpener()
  const editing = info.editingParents
  const drop = (source: Dragged, over: DragOver) => {
    const target = over.target
    if (readOnly || editing || !target) return
    const [kind, raw] = (target.getAttribute('data-drop') ?? '').split(':')
    const at = Number(raw)
    if (!Number.isFinite(at)) return
    // A spread's thumbnail stands for its pages; a page's name for that page.
    const pages = kind === 'spread' ? (info.spreads.find((spread) => spread.index === at)?.pages.map((page) => page.index) ?? []) : kind === 'page' ? [at] : []
    if (!pages.length) return
    if (source.kind === 'parent') {
      void run('layout.pages.applyParent', { pages: pages.map((page) => page + 1), parent: source.label })
      return
    }
    const side = dropSide(target.getBoundingClientRect(), over.x, over.y, true)
    const before = side === 'before' ? pages[0] : pages[pages.length - 1] + 1
    const to = moveTarget(source.index, before)
    if (to != null) void run('layout.pages.move', { from: source.index, to }).then(() => setChosen(to))
  }
  const drag = useRowDrag<Dragged>(drop)
  const dialogs = useDialogs()
  /** The trash: a page with objects on it is asked about first (in the Delete Pages dialog, as from the menu). */
  const deletePage = (index: number) => {
    const spread = info.spreads.find((item) => item.pages.some((one) => one.index === index))
    if (spread && pageHasObjects(spread, index) && dialogs?.current) dialogs.current.invoke('layout.pages.delete', { place: { page: index, spread: spread.index } })
    else void run('layout.pages.delete', { pages: [index] })
  }
  /** Edit a parent: the canvas shows the parent spreads (and the panel lists them) until Back to Pages. */
  const editParents = async (on: boolean, label?: string | null) => {
    await run('layout.parents.edit', { on })
    await settle()
    // The parents now stand where the spreads were: the one chosen is shown once the panel lists them.
    if (on && label) setShowParent(info.parents.findIndex((parent) => parent.label === label))
  }
  const [showParent, setShowParent] = useState<number | null>(null)
  useEffect(() => {
    if (showParent == null || !editing) return
    const spread = info.spreads[showParent]
    setShowParent(null)
    if (spread) onShow(spread)
  }, [editing, info.spreads, onShow, showParent])
  /** A right-click on a page (or a spread's first page) chooses it and opens the page menu. */
  const pageMenu = (index: number) => (event: MouseEvent<HTMLElement>) => {
    if (!openMenu) return
    event.preventDefault()
    setChosen(index)
    openMenu({ kind: 'page', page: index }, event.clientX, event.clientY)
  }
  const page = Math.min(chosen, Math.max(0, info.pageCount - 1))
  const spreadOf = info.spreads.find((spread) => spread.pages.some((item) => item.index === page))
  const parentOf = spreadOf?.pages.find((item) => item.index === page)?.parent ?? null
  const parents = info.parents.filter((parent) => parent.label).map((parent) => [parent.label ?? '', parent.label ?? ''] as const)
  return (
    <div className="layout-panel" data-testid="layout-pages-panel">
      <PanelHeading
        title={p('Pages')}
        actions={
          <>
            <IconButton icon={Plus} label={t('addPage')} disabled={readOnly || editing} onClick={() => void run('layout.pages.insert', { count: 1, after: page })} data-testid="layout-add-page" />
            <IconButton
              icon={Ellipsis}
              label={t('pages.menu')}
              disabled={readOnly || editing || !openMenu}
              onClick={(event) => {
                const box = event.currentTarget.getBoundingClientRect()
                openMenu?.({ kind: 'page', page }, box.left, box.bottom)
              }}
              data-testid="layout-pages-menu"
            />
          </>
        }
      />
      {editing ? (
        <div className="layout-bar layout-editing-parents" data-testid="layout-editing-parents">
          <span className="kui-muted">{t('pages.editingParents')}</span>
          <Button onClick={() => void editParents(false)} data-testid="layout-parents-done">
            <ArrowLeft size={13} aria-hidden="true" /> {t('pages.backToPages')}
          </Button>
        </div>
      ) : null}
      <div className="layout-parents" data-testid="layout-parents">
        {info.parents.map((parent) => (
          <span
            key={parent.id}
            className="layout-parent"
            title={t('parentHint', { count: parent.items })}
            data-dragging={drag.dragging?.kind === 'parent' && drag.dragging.label === parent.label ? 'true' : undefined}
            onPointerDown={(event) => !readOnly && !editing && drag.start({ kind: 'parent', label: parent.label }, event)}
            onDoubleClick={() => parent.label && !readOnly && void editParents(true, parent.label)}
            data-testid={`layout-parent-${parent.label ?? 'none'}`}
          >
            {parent.label ?? t('noParent')}
          </span>
        ))}
        <IconButton icon={Plus} label={t('newParent')} disabled={readOnly} onClick={() => void run('layout.parents.new', {})} data-testid="layout-new-parent" />
      </div>
      <div className="layout-spreads">
        {info.spreads.map((spread) => (
          <div key={spread.index} className="layout-spread-cell">
            <button
              type="button"
              className="layout-spread"
              onClick={() => onShow(spread)}
              onContextMenu={pageMenu(spread.pages[0]?.index ?? 0)}
              data-drop={`spread:${spread.index}`}
              data-testid={`layout-spread-${spread.index}`}
            >
              <SpreadThumb spread={spread} />
            </button>
            <span className="layout-page-names">
              {spread.pages.map((item) => (
                <button
                  key={item.index}
                  type="button"
                  className="layout-page-name"
                  data-active={item.index === page}
                  data-drop={`page:${item.index}`}
                  data-dragging={drag.dragging?.kind === 'page' && drag.dragging.index === item.index ? 'true' : undefined}
                  onPointerDown={(event) => !readOnly && !editing && drag.start({ kind: 'page', index: item.index }, event)}
                  onClick={() => setChosen(item.index)}
                  onDoubleClick={() => {
                    setChosen(item.index)
                    onShow(spread)
                  }}
                  onContextMenu={pageMenu(item.index)}
                  data-testid={`layout-page-${item.index}`}
                >
                  {item.name}
                </button>
              ))}
            </span>
          </div>
        ))}
      </div>
      <fieldset className="layout-stack layout-fieldset" disabled={readOnly || editing}>
        <Choice
          label={t('pageParent')}
          value={parentOf ?? ''}
          choices={[['', t('noParent')] as const, ...parents]}
          onChange={(parent) => void run('layout.pages.applyParent', { pages: [page + 1], parent: parent || null })}
          testId="layout-page-parent"
        />
      </fieldset>
      <PanelFooter
        start={
          <>
            <IconButton icon={ArrowUp} label={t('movePageUp')} disabled={readOnly || editing || page === 0} onClick={() => void run('layout.pages.move', { from: page, to: page - 1 }).then(() => setChosen(page - 1))} data-testid="layout-page-up" />
            <IconButton icon={ArrowDown} label={t('movePageDown')} disabled={readOnly || editing || page + 1 >= info.pageCount} onClick={() => void run('layout.pages.move', { from: page, to: page + 1 }).then(() => setChosen(page + 1))} data-testid="layout-page-down" />
          </>
        }
        end={
          <>
            <IconButton icon={Settings2} label={p('Document Setup')} disabled={readOnly} onClick={onSetup} data-testid="layout-document-setup" />
            <IconButton icon={Copy} label={t('duplicateSpread')} disabled={readOnly || editing || !spreadOf} onClick={() => spreadOf && void run('layout.pages.duplicateSpread', { spread: spreadOf.index })} data-testid="layout-duplicate-spread" />
            <IconButton icon={Trash} label={t('deletePage')} disabled={readOnly || editing || info.pageCount < 2} onClick={() => deletePage(page)} data-testid="layout-delete-page" />
          </>
        }
      />
    </div>
  )
}
