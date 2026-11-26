import { useEffect, useState } from 'react'
import { FileInputIcon as FileInput, FileOutputIcon as FileOutput, ImageIcon, ImagePlusIcon as ImagePlus, Link2Icon as Link2, RefreshCwIcon as RefreshCw, RotateCwIcon as RotateCw } from '@kition/tessera/icons/react'
import { IconButton, PanelFooter, PICTURE_TYPES, base64, toast, useHostPick, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { usePanelQuery } from './query'
import type { DocumentInfo } from '../layout'
import { thumbnailType, unembedPath, unembeddable } from '../images'
import { useErrorText } from '../errorWords'

type Link = { asset: number; name: string; path: string | null; status: 'ok' | 'modified' | 'missing' | 'embedded'; pixels?: [number, number] | null; uses: Array<{ id: number; page: number | null; ppi: number | null }> }

/** What Place and Relink accept from the host's picker. */
export const PLACE_TYPES = [...PICTURE_TYPES, 'pdf', 'psd', 'eps', 'svg']

/**
 * InDesign's Links panel: every placed graphic with its state (embedded, linked, modified,
 * missing) and effective resolution. A row's link button selects the frames showing it; Relink
 * (missing or not) places a file the person picks through the host into each of them. Place
 * puts a picked image into the selected empty frame, or on the page. Unembed (an embedded PNG or
 * JPEG) writes the graphic beside the document through the host and links it there. Each row shows
 * the graphic small (plan stage 8); Update reloads a link whose file changed (all of them from the
 * footer), and Embed keeps only the copy in the document.
 */
export function LinksSection({ info }: { info: DocumentInfo }) {
  const { run, t, host, session } = useKitEditor()
  const words = useErrorText()
  const readOnly = useReadOnly()
  const { can: canPick, pick } = useHostPick()
  // Placing and relinking edit the layout: a viewer only goes to links.
  const can = canPick && !readOnly
  const links = usePanelQuery<Link[]>('linkedFile.list', {}) ?? []
  const place = async (frame?: number) => {
    const file = await pick(t('placePurpose'), PLACE_TYPES)
    if (!file) return
    await run('file.place', { base64: base64(file.bytes), name: file.name, ...(frame != null ? { frame } : {}) })
  }
  const relink = async (link: Link) => {
    const file = await pick(t('relinkPurpose', { name: link.name }), PLACE_TYPES)
    if (!file) return
    for (const use of link.uses) await run('file.place', { base64: base64(file.bytes), name: file.name, frame: use.id })
  }
  const unembed = async (link: Link) => {
    const doc = session.doc
    if (!doc) return
    try {
      const bytes = await doc.invoke<Uint8Array>('assetBytes', link.asset)
      if (!bytes?.length) throw new Error(t('unembedFailed'))
      const written = await host.exportFile(unembedPath(host.init.document.path, link.name), bytes)
      await run('linkedFile.unembed', { asset: link.asset, path: written })
      toast('success', t('unembedded'), written.split('/').pop())
    } catch (error) {
      toast('error', t('unembedFailed'), words(error))
    }
  }
  const update = async (asset?: number) => {
    const answer = await run('linkedFile.update', asset != null ? { asset } : {})
    if (answer === null) return
    const updated = Number((JSON.parse(answer) as { updated?: number } | null)?.updated ?? 0)
    toast('info', updated ? t('linksUpdated', { count: updated }) : t('linksCurrent'))
  }
  const target = info.selection.length === 1 ? info.selection[0] : undefined
  const modified = links.some((link) => link.status === 'modified')
  return (
    <div className="layout-panel" data-testid="layout-links">
      {links.length ? null : <p className="layout-empty">{t('noLinks')}</p>}
      {links.map((link) => {
        const ppi = link.uses.map((use) => use.ppi).filter((value): value is number => value != null)
        return (
          <div key={link.asset} className="layout-row layout-link-row" data-status={link.status} data-testid={`layout-link-${link.asset}`}>
            <LinkThumbnail asset={link.asset} name={link.name} pixels={link.pixels} />
            <span className="layout-row-text" title={link.name}>
              {link.name}
            </span>
            <span className="layout-badge" data-status={link.status}>
              {t(`link.${link.status}`)}
            </span>
            {ppi.length ? <span className="kui-muted layout-count">{t('ppi', { ppi: Math.round(Math.min(...ppi)) })}</span> : null}
            <IconButton icon={Link2} label={t('goToLink')} onClick={() => void run('linkedFile.goTo', { asset: link.asset })} data-testid={`layout-link-go-${link.asset}`} />
            {can ? <IconButton icon={RefreshCw} label={t('relink')} onClick={() => void relink(link)} data-testid={`layout-link-relink-${link.asset}`} /> : null}
            {!readOnly && link.status === 'modified' ? (
              <IconButton icon={RotateCw} label={t('updateLink')} onClick={() => void update(link.asset)} data-testid={`layout-link-update-${link.asset}`} />
            ) : null}
            {!readOnly && link.status !== 'embedded' ? (
              <IconButton icon={FileInput} label={t('embedLink')} onClick={() => void run('linkedFile.embed', { asset: link.asset })} data-testid={`layout-link-embed-${link.asset}`} />
            ) : null}
            {!readOnly && link.status === 'embedded' && unembeddable(link.name) ? (
              <IconButton icon={FileOutput} label={t('unembed')} onClick={() => void unembed(link)} data-testid={`layout-link-unembed-${link.asset}`} />
            ) : null}
          </div>
        )
      })}
      <PanelFooter
        end={
          <>
            {!readOnly && modified ? <IconButton icon={RotateCw} label={t('updateAllLinks')} onClick={() => void update()} data-testid="layout-links-update-all" /> : null}
            {can ? <IconButton icon={ImagePlus} label={t('placeImage')} onClick={() => void place(target)} data-testid="layout-place-image" /> : null}
          </>
        }
      />
    </div>
  )
}

/**
 * A link's graphic, small: the bytes the document keeps (`assetBytes`), shown by the browser when
 * it draws the type (PNG, JPEG, GIF, WebP, SVG); otherwise a picture sign.
 */
function LinkThumbnail({ asset, name, pixels }: { asset: number; name: string; pixels?: [number, number] | null }) {
  const { session } = useKitEditor()
  const [url, setUrl] = useState<string | null>(null)
  const doc = session.doc
  const type = thumbnailType(name)
  const size = pixels ? `${pixels[0]}x${pixels[1]}` : ''
  useEffect(() => {
    if (!doc || !type) return
    let made: string | null = null
    let live = true
    void doc
      .invoke<Uint8Array>('assetBytes', asset)
      .then((bytes) => {
        if (!live || !bytes?.length) return
        made = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type }))
        setUrl(made)
      })
      .catch(() => undefined)
    return () => {
      live = false
      if (made) URL.revokeObjectURL(made)
      setUrl(null)
    }
  }, [doc, asset, type, size])
  return (
    <span className="layout-link-thumb" data-testid={`layout-link-thumb-${asset}`}>
      {url ? <img src={url} alt="" draggable={false} /> : <ImageIcon size={14} aria-hidden />}
    </span>
  )
}
