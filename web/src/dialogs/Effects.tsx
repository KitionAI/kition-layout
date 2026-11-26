import { useMemo, useState } from 'react'
import { Checkbox, Choice, useKitEditor } from '@kition/sdk/editor'
import { findItem, type DocumentInfo } from '../layout'
import { DialogFrame } from './DialogFrame'
import { useEditorView } from '../viewContext'
import { useStepsPreview } from './preview'
import { BLEND_MODES, EFFECT_PAGES, effectOn, effectsDraft, effectsSteps, type EffectField, type EffectsDraft, type EffectsItem } from '../panels/effects'

/**
 * Effects (plan 2026-11-25, stage 7): InDesign's one dialog for the selected objects, a list of
 * its pages on the left (Transparency, then each effect with its check box: Drop Shadow, Inner
 * Shadow, Outer Glow, Inner Glow, Bevel and Emboss, Satin, Basic, Directional and Gradient
 * Feather) and the chosen page's settings. It opens with the first selected object's values; a
 * change shows on the page at once (the engine's preview of all the commands together), Cancel
 * takes it back and OK keeps it as one undo step.
 */
export function EffectsDialog({ info, onClose }: { info: DocumentInfo; onClose: () => void }) {
  const { t, phrase: p, session } = useKitEditor()
  const { settle } = useEditorView()
  const ids = info.selection
  const opened = useMemo<EffectsDraft>(() => {
    const first = ids.length ? (findItem(info, ids[0])?.item as EffectsItem | undefined) : undefined
    return effectsDraft(first)
    // The values it opens with stay fixed while the preview changes the objects.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const [draft, setDraft] = useState<EffectsDraft>(opened)
  const [page, setPage] = useState('transparency')
  const label = p('Effects')
  const steps = effectsSteps(ids, opened, draft)
  const preview = useStepsPreview(label, steps)
  const setEffect = (id: string, key: string, value: string) => setDraft((now) => ({ ...now, effects: { ...now.effects, [id]: { ...now.effects[id], [key]: value, ...(key !== 'on' ? { on: 'true' } : {}) } } }))
  const swatches = info.swatchColors.filter((swatch) => swatch.kind !== 'gradient' && swatch.kind !== 'none').map((swatch) => [swatch.name, swatch.name] as const)
  const field = (id: string, item: EffectField) => {
    const value = draft.effects[id]?.[item.key] ?? ''
    const testId = `layout-effects-${id}-${item.key}`
    const label = t(`effects.field.${item.key}`)
    if (item.kind === 'check') return <Checkbox key={item.key} label={label} checked={value === 'true'} onChange={(on) => setEffect(id, item.key, on ? 'true' : 'false')} testId={testId} />
    if (item.kind === 'swatch') {
      const list = swatches.some(([name]) => name === value) || !value ? swatches : [[value, value] as const, ...swatches]
      return <Choice key={item.key} label={label} value={value} choices={list} onChange={(next) => setEffect(id, item.key, next)} testId={testId} />
    }
    return (
      <label key={item.key} className="layout-field">
        <span>
          {label}
          {item.unit ? <span className="kui-muted"> ({item.unit})</span> : null}
        </span>
        <input className="kui-field" inputMode="decimal" value={value} onChange={(event) => setEffect(id, item.key, event.target.value)} data-testid={testId} />
      </label>
    )
  }
  const effect = EFFECT_PAGES.find((entry) => entry.id === page)
  return (
    <DialogFrame
      command="object.opacity"
      title={label}
      testId="layout-effects"
      preview={preview}
      okDisabled={steps === null}
      onOk={async () => {
        if (!steps || !steps.length || !session.doc) return
        if (preview && (await preview.keep({ steps }))) return
        await session.doc.invoke('runSteps', label, JSON.stringify(steps))
        await settle()
      }}
      onClose={onClose}
    >
      <div className="layout-options" data-testid="layout-effects-dialog">
        <nav className="layout-options-nav" aria-label={label}>
          <button type="button" className="layout-list-item" data-active={page === 'transparency'} onClick={() => setPage('transparency')} data-testid="layout-effects-page-transparency">
            {t('effects.transparency')}
          </button>
          {EFFECT_PAGES.map((entry) => (
            <span key={entry.id} className="layout-effects-entry">
              <input
                type="checkbox"
                checked={effectOn(draft, entry.id)}
                aria-label={t(`effects.${entry.id}`)}
                onChange={(event) => setEffect(entry.id, 'on', event.target.checked ? 'true' : 'false')}
                data-testid={`layout-effects-on-${entry.id}`}
              />
              <button type="button" className="layout-list-item" data-active={page === entry.id} onClick={() => setPage(entry.id)} data-testid={`layout-effects-page-${entry.id}`}>
                {t(`effects.${entry.id}`)}
              </button>
            </span>
          ))}
        </nav>
        <div className="layout-options-page">
          {page === 'transparency' ? (
            <section className="layout-stack">
              <h3 className="layout-options-heading">{t('effects.transparency')}</h3>
              <Choice label={t('effects.blend')} value={draft.blend} choices={BLEND_MODES.map((mode) => [mode, t(`blend.${mode}`)] as const)} onChange={(blend) => setDraft({ ...draft, blend })} testId="layout-effects-blend" />
              <label className="layout-field">
                <span>
                  {t('effects.field.opacity')} <span className="kui-muted">(%)</span>
                </span>
                <input className="kui-field" inputMode="decimal" value={draft.opacity} onChange={(event) => setDraft({ ...draft, opacity: event.target.value })} data-testid="layout-effects-opacity" />
              </label>
            </section>
          ) : effect ? (
            <section className="layout-stack">
              <h3 className="layout-options-heading">{t(`effects.${effect.id}`)}</h3>
              <Checkbox label={t('effects.on')} checked={effectOn(draft, effect.id)} onChange={(on) => setEffect(effect.id, 'on', on ? 'true' : 'false')} testId={`layout-effects-${effect.id}-on`} />
              <div className="layout-grid">{effect.fields.map((item) => field(effect.id, item))}</div>
            </section>
          ) : null}
        </div>
      </div>
    </DialogFrame>
  )
}
