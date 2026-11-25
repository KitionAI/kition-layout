import { useEffect, useState, type MouseEvent } from 'react'
import { ArrowDownIcon as ArrowDown, ArrowUpIcon as ArrowUp, CopyIcon as Copy, PlusIcon as Plus, Settings2Icon as Settings2, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Choice, IconButton, PanelFooter, PanelSection, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import type { DocumentInfo, Spread } from '../layout'
import { useMenuOpener } from '../menus/ContextMenus'

const THUMB = 92

/** A spread's thumbnail, drawn by the engine a moment after each change. */
function SpreadThumb({ spread }: { spread: Spread }) {
  const { session } = useKitEditor()
  const { doc, change } = session
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null)
  const width = spread.area.x1 - spread.area.x0
  const height = spread.area.y1 - spread.area.y0
  const scale = Math.min(THUMB / Math.max(1, width), THUMB / Math.max(1, height))
  const w = Math.max(1, Math.round(width * scale))
  const h = Math.max(1, Math.round(height * scale))
  useEffect(() => {
    if (!doc || !canvas) return undefined
    let stale = false
    const timer = window.setTimeout(async () => {
      const rgba = await doc.render(spread.area.x0, spread.area.y0, scale, w, h).catch(() => null)
      if (stale || !rgba) return
      canvas.width = w
      canvas.height = h
      canvas.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), w, h), 0, 0)
    }, 300)
    return () => {
      stale = true
      window.clearTimeout(timer)
    }
  }, [canvas, change.revision, doc, h, scale, spread.area.x0, spread.area.y0, w])
  return <canvas ref={setCanvas} width={w} height={h} className="layout-thumb" />
}

/**
 * InDesign's Pages panel: the parents, then every spread as a thumbnail (click to show it, its
 * page names under it). A page chosen in the panel can be moved, duplicated (its spread), deleted,
 * or given another parent; Document Setup sets the size of every page.
 */
export function PagesSection({ info, onShow, onSetup }: { info: DocumentInfo; onShow: (spread: Spread) => void; onSetup: () => void }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [chosen, setChosen] = useState(0)
  const openMenu = useMenuOpener()
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
    <PanelSection title={p('Pages')} actions={<IconButton icon={Plus} label={t('addPage')} disabled={readOnly} onClick={() => void run('layout.pages.insert', { count: 1, after: page })} data-testid="layout-add-page" />} testId="layout-pages-panel">
      <div className="layout-parents" data-testid="layout-parents">
        {info.parents.map((parent) => (
          <span key={parent.id} className="layout-parent" title={t('parentItems', { count: parent.items })}>
            {parent.label ?? t('noParent')}
          </span>
        ))}
        <IconButton icon={Plus} label={t('newParent')} disabled={readOnly} onClick={() => void run('layout.parents.new', {})} data-testid="layout-new-parent" />
      </div>
      <div className="layout-spreads">
        {info.spreads.map((spread) => (
          <div key={spread.index} className="layout-spread-cell">
            <button type="button" className="layout-spread" onClick={() => onShow(spread)} onContextMenu={pageMenu(spread.pages[0]?.index ?? 0)} data-testid={`layout-spread-${spread.index}`}>
              <SpreadThumb spread={spread} />
            </button>
            <span className="layout-page-names">
              {spread.pages.map((item) => (
                <button key={item.index} type="button" className="layout-page-name" data-active={item.index === page} onClick={() => setChosen(item.index)} onContextMenu={pageMenu(item.index)} data-testid={`layout-page-${item.index}`}>
                  {item.name}
                </button>
              ))}
            </span>
          </div>
        ))}
      </div>
      <fieldset className="layout-stack layout-fieldset" disabled={readOnly}>
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
            <IconButton icon={ArrowUp} label={t('movePageUp')} disabled={readOnly || page === 0} onClick={() => void run('layout.pages.move', { from: page, to: page - 1 }).then(() => setChosen(page - 1))} data-testid="layout-page-up" />
            <IconButton icon={ArrowDown} label={t('movePageDown')} disabled={readOnly || page + 1 >= info.pageCount} onClick={() => void run('layout.pages.move', { from: page, to: page + 1 }).then(() => setChosen(page + 1))} data-testid="layout-page-down" />
          </>
        }
        end={
          <>
            <IconButton icon={Settings2} label={p('Document Setup')} disabled={readOnly} onClick={onSetup} data-testid="layout-document-setup" />
            <IconButton icon={Copy} label={t('duplicateSpread')} disabled={readOnly || !spreadOf} onClick={() => spreadOf && void run('layout.pages.duplicateSpread', { spread: spreadOf.index })} data-testid="layout-duplicate-spread" />
            <IconButton icon={Trash} label={t('deletePage')} disabled={readOnly || info.pageCount < 2} onClick={() => void run('layout.pages.delete', { pages: [page] })} data-testid="layout-delete-page" />
          </>
        }
      />
    </PanelSection>
  )
}
