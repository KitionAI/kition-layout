import { FileOutputIcon as FileOutput, ImagePlusIcon as ImagePlus, Link2Icon as Link2, RefreshCwIcon as RefreshCw } from '@kition/tessera/icons/react'
import { IconButton, PanelFooter, PICTURE_TYPES, base64, toast, useEngineQuery, useHostPick, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { unembedPath, unembeddable } from '../images'

type Link = { asset: number; name: string; path: string | null; status: 'ok' | 'modified' | 'missing' | 'embedded'; pixels?: [number, number] | null; uses: Array<{ id: number; page: number | null; ppi: number | null }> }

/** What Place and Relink accept from the host's picker. */
export const PLACE_TYPES = [...PICTURE_TYPES, 'pdf', 'psd', 'eps', 'svg']

/**
 * InDesign's Links panel: every placed graphic with its state (embedded, linked, modified,
 * missing) and effective resolution. A row's link button selects the frames showing it; Relink
 * (missing or not) places a file the person picks through the host into each of them. Place
 * puts a picked image into the selected empty frame, or on the page. Unembed (an embedded PNG or
 * JPEG) writes the graphic beside the document through the host and links it there.
 */
export function LinksSection({ info }: { info: DocumentInfo }) {
  const { run, t, host, session } = useKitEditor()
  const readOnly = useReadOnly()
  const { can: canPick, pick } = useHostPick()
  // Placing and relinking edit the layout: a viewer only goes to links.
  const can = canPick && !readOnly
  const links = useEngineQuery<Link[]>('linkedFile.list', {}) ?? []
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
      toast('error', t('unembedFailed'), error instanceof Error ? error.message : String(error))
    }
  }
  const target = info.selection.length === 1 ? info.selection[0] : undefined
  return (
    <div className="layout-panel" data-testid="layout-links">
      {links.length ? null : <p className="layout-empty">{t('noLinks')}</p>}
      {links.map((link) => {
        const ppi = link.uses.map((use) => use.ppi).filter((value): value is number => value != null)
        return (
          <div key={link.asset} className="layout-row layout-link-row" data-status={link.status} data-testid={`layout-link-${link.asset}`}>
            <span className="layout-row-text" title={link.name}>
              {link.name}
            </span>
            <span className="layout-badge" data-status={link.status}>
              {t(`link.${link.status}`)}
            </span>
            {ppi.length ? <span className="kui-muted layout-count">{t('ppi', { ppi: Math.round(Math.min(...ppi)) })}</span> : null}
            <IconButton icon={Link2} label={t('goToLink')} onClick={() => void run('linkedFile.goTo', { asset: link.asset })} data-testid={`layout-link-go-${link.asset}`} />
            {can ? <IconButton icon={RefreshCw} label={t('relink')} onClick={() => void relink(link)} data-testid={`layout-link-relink-${link.asset}`} /> : null}
            {!readOnly && link.status === 'embedded' && unembeddable(link.name) ? (
              <IconButton icon={FileOutput} label={t('unembed')} onClick={() => void unembed(link)} data-testid={`layout-link-unembed-${link.asset}`} />
            ) : null}
          </div>
        )
      })}
      <PanelFooter end={can ? <IconButton icon={ImagePlus} label={t('placeImage')} onClick={() => void place(target)} data-testid="layout-place-image" /> : null} />
    </div>
  )
}
