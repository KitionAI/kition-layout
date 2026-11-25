import { useEffect, useMemo, useState } from 'react'
import { FileUpIcon as FileUp, MonitorIcon as Monitor, SearchIcon as Search } from '@kition/tessera/icons/react'
import { Button, IconButton, Popover, PopoverContent, PopoverTrigger, toast, useHostPick, useKitEditor } from '@kition/sdk/editor'
import { canListLocalFonts, fontFamilies, listLocalFonts, loadPreviews, localFamilyFiles, previewFamily, registerPreview, type LocalFamily } from '../fonts'
import { useEditorView } from '../viewContext'

/** Font files a person may load (the engine reads TrueType, OpenType and collections). */
const FONT_TYPES = ['ttf', 'otf', 'ttc']

/**
 * The font menu (plan 2026-11-25, stage 4): a search over the families text can be set in, each
 * drawn in its own face, then the computer's fonts once the person asks for them (the browser's
 * Local Font Access API, where it has one) and "Load Font File…" (a file the host picks). A family
 * from the computer or a file is given to the engine (`addFontFile`) before the text takes it.
 */
export function FontMenu({ families, current, onPick, testId }: { families: readonly string[]; current: string; onPick: (family: string) => void; testId: string }) {
  const { t, session } = useKitEditor()
  const { repaint } = useEditorView()
  const { can, pick } = useHostPick()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [local, setLocal] = useState<LocalFamily[] | null>(null)
  const [busy, setBusy] = useState(false)
  const doc = session.doc
  const match = (family: string) => family.toLowerCase().includes(query.trim().toLowerCase())
  const own = useMemo(() => families.filter(match), [families, query]) // eslint-disable-line react-hooks/exhaustive-deps
  const others = useMemo(() => (local ?? []).filter((family) => !families.includes(family.family) && match(family.family)).slice(0, 300), [local, families, query]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (open) loadPreviews(families)
  }, [open, families])
  /** Gives the engine a font's files; true when it took at least one face. */
  const give = async (files: Uint8Array[], family: string) => {
    if (!doc) return false
    let added = 0
    for (const bytes of files) added += Number(await doc.invoke<number>('addFontFile', bytes).catch(() => 0))
    if (files[0]) registerPreview(family, files[0])
    await doc.refreshInfo()
    repaint()
    return added > 0 || families.includes(family)
  }
  const choose = (family: string) => {
    setOpen(false)
    onPick(family)
  }
  const chooseLocal = async (family: LocalFamily) => {
    setBusy(true)
    try {
      const taken = await give(await localFamilyFiles(family), family.family)
      if (taken) choose(family.family)
      else toast('info', t('font.unreadable', { family: family.family }))
    } finally {
      setBusy(false)
    }
  }
  const askLocal = async () => {
    try {
      setLocal(await listLocalFonts())
    } catch {
      toast('info', t('font.localRefused'))
    }
  }
  const loadFile = async () => {
    const file = await pick(t('font.pickPurpose'), FONT_TYPES)
    if (!file) return
    const bytes = new Uint8Array(file.bytes)
    const names = fontFamilies(bytes)
    const family = names[0]
    if (!family || !(await give([bytes], family))) {
      toast('info', t('font.unreadable', { family: file.name }))
      return
    }
    choose(family)
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <IconButton icon={Search} label={t('font.browse')} data-testid={`${testId}-browse`} />
      </PopoverTrigger>
      <PopoverContent align="end" className="layout-font-menu" data-testid={`${testId}-menu`}>
        <input className="kui-field" type="search" autoFocus placeholder={t('font.search')} aria-label={t('font.search')} value={query} onChange={(event) => setQuery(event.target.value)} data-testid={`${testId}-search`} />
        <div className="layout-font-list" role="listbox" aria-label={t('font.families')}>
          {own.map((family) => (
            <button key={family} type="button" role="option" aria-selected={family === current} className="layout-font-row" style={{ fontFamily: previewFamily(family) }} onClick={() => choose(family)} data-testid={`${testId}-family-${family}`}>
              {family}
            </button>
          ))}
          {others.length ? <div className="layout-font-group">{t('font.computer')}</div> : null}
          {others.map((family) => (
            <button key={family.family} type="button" role="option" aria-selected={false} className="layout-font-row" disabled={busy} style={{ fontFamily: previewFamily(family.family) }} onClick={() => void chooseLocal(family)} data-testid={`${testId}-local-${family.family}`}>
              {family.family}
            </button>
          ))}
          {!own.length && !others.length ? <p className="layout-empty">{t('font.none')}</p> : null}
        </div>
        <div className="layout-font-actions">
          {canListLocalFonts() && local === null ? (
            <Button onClick={() => void askLocal()} data-testid={`${testId}-computer`}>
              <Monitor size={14} aria-hidden="true" /> {t('font.showComputer')}
            </Button>
          ) : null}
          {!canListLocalFonts() ? <p className="layout-note kui-muted">{t('font.noLocalAccess')}</p> : null}
          {can ? (
            <Button onClick={() => void loadFile()} data-testid={`${testId}-file`}>
              <FileUp size={14} aria-hidden="true" /> {t('font.loadFile')}
            </Button>
          ) : null}
        </div>
      </PopoverContent>
    </Popover>
  )
}
