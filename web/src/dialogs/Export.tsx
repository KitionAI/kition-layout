import { useState } from 'react'
import { Button, Checkbox, Choice, Dialog, NumberField, toast, useKitEditor, useKitStore } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { FORMATS, type Format, bytesOf, bytesRequest, decodeBase64, exportPath, writeExports } from '../exporting'

type Range = 'all' | 'current' | 'range' | 'sizes'
export type ExportSettings = { format: Format; range: Range; pages: string; ppi: number; transparent: boolean; quality: number; bleed: boolean }

export const DEFAULT_EXPORT: ExportSettings = { format: 'png', range: 'all', pages: '', ppi: 72, transparent: false, quality: 90, bleed: false }

function validSettings(raw: unknown): ExportSettings | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const value = { ...DEFAULT_EXPORT, ...(raw as Partial<ExportSettings>) }
  if (!FORMATS.includes(value.format) || !['all', 'current', 'range', 'sizes'].includes(value.range)) return undefined
  if (!Number.isFinite(value.ppi) || value.ppi < 1 || value.ppi > 2400 || !Number.isFinite(value.quality)) return undefined
  return value
}

/** A layout name as part of a file name. */
const safe = (name: string) => name.replace(/[\\/:*?"<>|]+/g, '-').trim() || 'size'

/**
 * File › Export: PNG or JPEG pages (resolution, transparent background for PNG, JPEG quality),
 * PDF, IDML, EPUB or HTML, of every page, every size (alternate layouts), the page in view, or a
 * range; written next to the
 * document through the host. The choices are remembered for the document.
 */
export function ExportDialog({ info, page, onClose }: { info: DocumentInfo; page: number; onClose: () => void }) {
  const { query, host, t } = useKitEditor()
  const key = `export:${host.init.document.path}`
  const [stored, setStored] = useKitStore<ExportSettings>('kit.layout', key, { initial: DEFAULT_EXPORT, validate: validSettings })
  const [settings, setSettings] = useState<ExportSettings>(stored)
  const [busy, setBusy] = useState(false)
  const set = (next: Partial<ExportSettings>) => setSettings((now) => ({ ...now, ...next }))
  const raster = settings.format === 'png' || settings.format === 'jpeg'
  const paged = raster || settings.format === 'pdf'
  // Every size: the first page of the original and of each alternate layout.
  const sizes = settings.range === 'sizes' && info.layouts.length > 1 && raster
  const range = settings.range === 'sizes' && !sizes ? 'all' : settings.range
  const pages = range === 'all' || sizes ? undefined : range === 'current' ? String(page + 1) : settings.pages.trim() || undefined
  const doc = { name: host.init.document.name || 'Untitled', path: host.init.document.path }

  const run = async () => {
    setBusy(true)
    setStored(settings)
    try {
      const files: Array<{ path: string; bytes: Uint8Array }> = []
      if (raster) {
        const answer = await query<{ files: Array<{ page: number; layout: string | null; base64: string }> }>('file.exportImage', {
          format: settings.format,
          ...(pages ? { pages } : {}),
          ...(sizes ? { sizes: true } : {}),
          ppi: settings.ppi,
          background: settings.format === 'png' && settings.transparent ? 'transparent' : 'paper',
          quality: settings.quality,
          bleed: settings.bleed,
        })
        if (!answer) throw new Error(t('exportFailed'))
        const many = answer.files.length > 1
        // Files of an alternate layout take its name; pages are numbered when a layout gives several.
        const perLayout = (layout: string | null) => answer.files.filter((file) => file.layout === layout).length
        for (const file of answer.files) {
          const named = info.layouts.length > 1 && file.layout != null
          const numbered = named ? perLayout(file.layout) > 1 : many && (info.layouts.length < 2 || perLayout(null) > 1)
          const suffix = `${named ? `-${safe(file.layout ?? '')}` : ''}${numbered ? `-${file.page}` : ''}`
          files.push({ path: exportPath(doc, settings.format, suffix), bytes: decodeBase64(file.base64) })
        }
      } else {
        // PDF, IDML, EPUB and HTML: the engine answers the file's bytes, the host writes it.
        const request = bytesRequest(settings.format, { pages, bleed: settings.bleed })
        const bytes = bytesOf(await query<Record<string, unknown>>(request.command, request.params))
        if (!bytes) throw new Error(t('exportFailed'))
        files.push({ path: exportPath(doc, settings.format), bytes })
      }
      // Success only once the host wrote every file; its refusal is shown as the error.
      const written = await writeExports(files, (path, bytes) => host.exportFile(path, bytes))
      toast('success', t('exported', { count: written.length }), written.map((path) => path.split('/').pop()).join(', '))
      onClose()
    } catch (error) {
      toast('error', t('exportFailed'), error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog
      title={t('export')}
      onClose={onClose}
      testId="layout-export-dialog"
      actions={
        <>
          <Button onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" disabled={busy} onClick={() => void run()} data-testid="layout-export-ok">
            {t('exportAction')}
          </Button>
        </>
      }
    >
      <div className="layout-dialog-stack">
        <Choice label={t('format')} value={settings.format} choices={FORMATS.map((format) => [format, t(`format.${format}`)] as const)} onChange={(format) => set({ format })} testId="layout-export-format" />
        {paged ? (
          <>
            <Choice
              label={t('pages')}
              value={range}
              choices={[
                ['all', t('allPages', { count: info.pageCount })],
                ...(raster && info.layouts.length > 1 ? [['sizes', t('allSizes', { count: info.layouts.length })] as const] : []),
                ['current', t('currentPage', { page: page + 1 })],
                ['range', t('pageRange')],
              ]}
              onChange={(range) => set({ range })}
              testId="layout-export-range"
            />
            {range === 'range' ? (
              <label className="layout-field">
                <span>{t('pageRange')}</span>
                <input className="kui-field" value={settings.pages} placeholder="1-3, 5" onChange={(event) => set({ pages: event.target.value })} data-testid="layout-export-pages" />
              </label>
            ) : null}
            <Checkbox label={t('includeBleed')} checked={settings.bleed} onChange={(bleed) => set({ bleed })} testId="layout-export-bleed" />
          </>
        ) : null}
        {raster ? (
          <>
            <Choice
              label={t('resolution')}
              value={String(settings.ppi)}
              choices={[72, 144, 216, 300].map((ppi) => [String(ppi), t('scale', { scale: ppi / 72, ppi })] as const)}
              onChange={(ppi) => set({ ppi: Number(ppi) })}
              testId="layout-export-ppi"
            />
            {settings.format === 'png' ? <Checkbox label={t('transparentBackground')} checked={settings.transparent} onChange={(transparent) => set({ transparent })} testId="layout-export-transparent" /> : null}
            {settings.format === 'jpeg' ? <NumberField label={t('quality')} value={settings.quality} min={1} max={100} step={1} track onChange={(quality, final) => final && set({ quality })} testId="layout-export-quality" /> : null}
          </>
        ) : null}
      </div>
    </Dialog>
  )
}
