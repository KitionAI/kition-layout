import { useEffect, useRef, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { useEditorView } from '../viewContext'
import { RULER_UNITS } from '../menus/contextRows'
import type { Unit } from '../units'
import { DialogFrame, useRunInDialog } from './DialogFrame'
import { CheckRow, ChoiceRow, LengthRow, NumberRow } from './controls'

/**
 * Edit › Preferences… (plan 2026-11-25, stage 3): the three pages with something to set. Units &
 * Increments: the ruler unit (the editor's), the arrow keys' step (the document's keyboard
 * increment) and whether sizes include the stroke weight; Guides & Grids: the baseline grid's
 * start, step and origin, the settings grid's spacing and subdivisions; Type: typographer's
 * quotes and the hidden characters. OK sets only what changed: the view in the editor, the
 * application preferences in the engine (`prefs.set`), the document's settings as one history
 * step (`file.preferences`).
 */

type Page = 'units' | 'grids' | 'type'
type DocumentPreferences = {
  keyboardIncrement: number
  baselineGrid: { start: number; increment: number; relativeTo: 'topOfPage' | 'topMargin' }
  grid: { horizontal: number; vertical: number; subdivisions: number }
}
type AppPreferences = { typographersQuotes: boolean; dimensionsIncludeStroke: boolean }

export function PreferencesDialog({ onClose }: { onClose: () => void }) {
  const { t, session } = useKitEditor()
  const { view, setView } = useEditorView()
  const run = useRunInDialog()
  const [page, setPage] = useState<Page>('units')
  const [unit, setUnit] = useState<Unit>(view.unit)
  const [hidden, setHidden] = useState(view.hiddenCharacters)
  const [started, setStarted] = useState<{ settings: DocumentPreferences; app: AppPreferences } | null>(null)
  const [settings, setSettings] = useState<DocumentPreferences | null>(null)
  const [app, setApp] = useState<AppPreferences | null>(null)
  const asked = useRef(false)
  useEffect(() => {
    // Both commands answer the preferences as they are when given nothing to set; asked once, so
    // what the person types is not read over again.
    if (asked.current) return
    asked.current = true
    void Promise.all([session.run('file.preferences', {}), session.run('prefs.set', {})]).then(([doc, prefs]) => {
      if (!doc || !prefs) return
      const d = JSON.parse(doc) as DocumentPreferences
      const p = JSON.parse(prefs) as AppPreferences
      const now = {
        settings: { keyboardIncrement: d.keyboardIncrement, baselineGrid: { start: d.baselineGrid.start, increment: d.baselineGrid.increment, relativeTo: d.baselineGrid.relativeTo }, grid: { horizontal: d.grid.horizontal, vertical: d.grid.vertical, subdivisions: d.grid.subdivisions } },
        app: { typographersQuotes: p.typographersQuotes, dimensionsIncludeStroke: p.dimensionsIncludeStroke },
      }
      setStarted(now)
      setSettings(now.settings)
      setApp(now.app)
    })
  }, [session])
  const valid = Boolean(settings && settings.keyboardIncrement > 0 && settings.baselineGrid.increment >= 1 && settings.grid.horizontal > 0 && settings.grid.subdivisions >= 1)
  const ok = async () => {
    setView((now) => ({ ...now, unit, hiddenCharacters: hidden }))
    if (!started || !settings || !app) return
    const appChanges = Object.fromEntries(Object.entries(app).filter(([key, value]) => started.app[key as keyof AppPreferences] !== value))
    if (Object.keys(appChanges).length) await run('prefs.set', appChanges)
    const docChanges: Record<string, unknown> = {}
    if (settings.keyboardIncrement !== started.settings.keyboardIncrement) docChanges.keyboardIncrement = settings.keyboardIncrement
    if (JSON.stringify(settings.baselineGrid) !== JSON.stringify(started.settings.baselineGrid)) docChanges.baselineGrid = settings.baselineGrid
    if (JSON.stringify(settings.grid) !== JSON.stringify(started.settings.grid)) docChanges.grid = settings.grid
    if (Object.keys(docChanges).length) await run('file.preferences', docChanges)
  }
  const setGrid = (change: Partial<DocumentPreferences['grid']>) => setSettings((now) => (now ? { ...now, grid: { ...now.grid, ...change } } : now))
  const setBaseline = (change: Partial<DocumentPreferences['baselineGrid']>) => setSettings((now) => (now ? { ...now, baselineGrid: { ...now.baselineGrid, ...change } } : now))
  return (
    <DialogFrame command="kit:edit.preferences" title={t('dialog.preferences')} okDisabled={!valid} onOk={ok} onClose={onClose}>
      <div className="layout-dialog-fields" data-testid="layout-preferences">
        <div className="layout-tabs" role="tablist" aria-label={t('dialog.preferences')}>
          {(['units', 'grids', 'type'] as const).map((item) => (
            <button key={item} type="button" role="tab" className="kui-button" data-variant={page === item ? 'primary' : undefined} aria-selected={page === item} onClick={() => setPage(item)} data-testid={`layout-preferences-${item}`}>
              {t(`preferences.${item}`)}
            </button>
          ))}
        </div>
        {page === 'units' ? (
          <>
            <ChoiceRow label={t('preferences.rulerUnits')} value={unit} choices={RULER_UNITS.map((item) => [item, t(`unit.${item}`)] as const)} onChange={setUnit} testId="layout-preferences-unit" />
            {settings ? <LengthRow label={t('preferences.keyboardIncrement')} value={settings.keyboardIncrement} unit={unit} onChange={(next) => next !== null && next > 0 && setSettings({ ...settings, keyboardIncrement: next })} testId="layout-preferences-increment" /> : null}
            {app ? <CheckRow label={t('preferences.dimensionsIncludeStroke')} checked={app.dimensionsIncludeStroke} onChange={(next) => setApp({ ...app, dimensionsIncludeStroke: next })} testId="layout-preferences-stroke" /> : null}
          </>
        ) : null}
        {page === 'grids' && settings ? (
          <>
            <fieldset className="layout-dialog-group">
              <legend>{t('preferences.baselineGrid')}</legend>
              <LengthRow label={t('preferences.start')} value={settings.baselineGrid.start} unit={unit} onChange={(next) => next !== null && setBaseline({ start: next })} testId="layout-preferences-baseline-start" />
              <LengthRow label={t('preferences.every')} value={settings.baselineGrid.increment} unit={unit} onChange={(next) => next !== null && next > 0 && setBaseline({ increment: next })} testId="layout-preferences-baseline-every" />
              <ChoiceRow label={t('preferences.relativeTo')} value={settings.baselineGrid.relativeTo} choices={[['topOfPage', t('preferences.topOfPage')], ['topMargin', t('preferences.topMargin')]] as const} onChange={(relativeTo) => setBaseline({ relativeTo })} testId="layout-preferences-baseline-relative" />
            </fieldset>
            <fieldset className="layout-dialog-group">
              <legend>{t('preferences.documentGrid')}</legend>
              <LengthRow label={t('preferences.gridEvery')} value={settings.grid.horizontal} unit={unit} onChange={(next) => next !== null && next > 0 && setGrid({ horizontal: next, vertical: next })} testId="layout-preferences-grid-every" />
              <NumberRow label={t('preferences.subdivisions')} value={settings.grid.subdivisions} min={1} max={100} decimals={0} onChange={(next) => next !== null && next >= 1 && setGrid({ subdivisions: Math.round(next) })} testId="layout-preferences-grid-subdivisions" />
            </fieldset>
          </>
        ) : null}
        {page === 'type' ? (
          <>
            {app ? <CheckRow label={t('preferences.typographersQuotes')} checked={app.typographersQuotes} onChange={(next) => setApp({ ...app, typographersQuotes: next })} testId="layout-preferences-quotes" /> : null}
            <CheckRow label={t('preferences.hiddenCharacters')} checked={hidden} onChange={setHidden} testId="layout-preferences-hidden" />
          </>
        ) : null}
      </div>
    </DialogFrame>
  )
}
