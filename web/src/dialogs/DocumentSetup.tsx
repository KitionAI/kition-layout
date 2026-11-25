import { Fragment, useState } from 'react'
import { Button, Checkbox, Choice, Dialog, NumberField, useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'

/** Units the dialog measures in; a pixel is a point, as in a web or mobile document. */
const UNITS = { pt: 1, px: 1, mm: 72 / 25.4, in: 72 } as const
type Unit = keyof typeof UNITS

/** Document sizes: print sizes, and the screen sizes Kition's designs start from (in points = px). */
export const PRESETS = [
  { id: 'letter', width: 612, height: 792, intent: 'print', unit: 'in' },
  { id: 'a4', width: 595.276, height: 841.89, intent: 'print', unit: 'mm' },
  { id: 'poster', width: 1080, height: 1440, intent: 'web', unit: 'px' },
  { id: 'square', width: 1080, height: 1080, intent: 'web', unit: 'px' },
  { id: 'story', width: 1080, height: 1920, intent: 'mobile', unit: 'px' },
  { id: 'landscape', width: 1920, height: 1080, intent: 'web', unit: 'px' },
] as const

type Preset = (typeof PRESETS)[number]['id'] | 'custom'

/** The preset a page size is, if any (either orientation counts for print sizes). */
export function presetOf(width: number, height: number): Preset {
  const near = (a: number, b: number) => Math.abs(a - b) < 0.5
  return PRESETS.find((preset) => near(preset.width, width) && near(preset.height, height))?.id ?? 'custom'
}

/**
 * Document Setup with InDesign's New Document choices and Kition's design sizes: a preset or a
 * custom size in points, pixels, millimetres or inches, facing pages, four margins (linked or each
 * its own), columns and gutter, and bleed. Applied in one step each (`layout.documentSetup`, `layout.marginsAndColumns`).
 */
export function DocumentSetupDialog({ info, onClose }: { info: DocumentInfo; onClose: () => void }) {
  const { run, t } = useKitEditor()
  const page = info.spreads[0]?.pages[0]
  const [width, setWidth] = useState(info.settings.pageWidth)
  const [height, setHeight] = useState(info.settings.pageHeight)
  const [unit, setUnit] = useState<Unit>(() => PRESETS.find((preset) => preset.id === presetOf(info.settings.pageWidth, info.settings.pageHeight))?.unit ?? 'pt')
  const [intent, setIntent] = useState(info.settings.intent || 'print')
  const [facing, setFacing] = useState(info.settings.facingPages)
  const start = page?.margins ?? { top: 36, bottom: 36, inside: 36, outside: 36 }
  const [margins, setMargins] = useState({ top: start.top, bottom: start.bottom, inside: start.inside, outside: start.outside })
  const [linked, setLinked] = useState(start.top === start.bottom && start.top === start.inside && start.top === start.outside)
  const [columns, setColumns] = useState(page?.columns ?? 1)
  const [gutter, setGutter] = useState(12)
  const [bleed, setBleed] = useState(0)
  const preset = presetOf(width, height)
  const k = UNITS[unit]
  const shown = (points: number) => Math.round((points / k) * 100) / 100
  const choose = (id: Preset) => {
    const found = PRESETS.find((item) => item.id === id)
    if (!found) return
    setWidth(found.width)
    setHeight(found.height)
    setUnit(found.unit)
    setIntent(found.intent)
    if (found.intent !== 'print') setFacing(false)
  }
  const apply = async () => {
    await run('layout.documentSetup', { width, height, facingPages: facing, intent, bleed })
    await run('layout.marginsAndColumns', { margins, columns, gutter })
    onClose()
  }
  const size = (label: string, value: number, set: (points: number) => void, testId: string, min = 0.01) => (
    <NumberField label={label} value={shown(value)} min={min} max={100000} step={unit === 'in' ? 0.01 : 1} unit={unit} onChange={(next, final) => final && set(next * k)} testId={testId} />
  )
  return (
    <Dialog
      title={t('documentSetup')}
      onClose={onClose}
      testId="layout-setup-dialog"
      actions={
        <>
          <Button onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" onClick={() => void apply()} data-testid="layout-setup-ok">
            {t('ok')}
          </Button>
        </>
      }
    >
      <div className="layout-presets" role="radiogroup" aria-label={t('presets')}>
        {PRESETS.map((item) => (
          <button key={item.id} type="button" role="radio" aria-checked={preset === item.id} className="layout-preset" data-active={preset === item.id} onClick={() => choose(item.id)} data-testid={`layout-preset-${item.id}`}>
            <span className="layout-preset-shape" style={{ aspectRatio: `${item.width} / ${item.height}` }} aria-hidden="true" />
            <span>{t(`preset.${item.id}`)}</span>
            <span className="kui-muted">{item.unit === 'px' ? `${item.width} × ${item.height} px` : t(`presetSize.${item.id}`)}</span>
          </button>
        ))}
      </div>
      <div className="layout-grid">
        <Choice label={t('units')} value={unit} choices={(Object.keys(UNITS) as Unit[]).map((value) => [value, t(`unit.${value}`)] as const)} onChange={setUnit} testId="layout-setup-unit" />
        <Choice label={t('intent')} value={intent} choices={['print', 'web', 'mobile'].map((value) => [value, t(`intent.${value}`)] as const)} onChange={setIntent} testId="layout-setup-intent" />
        {size(t('width'), width, setWidth, 'layout-setup-width')}
        {size(t('height'), height, setHeight, 'layout-setup-height')}
        {(['top', 'bottom', 'inside', 'outside'] as const).map((side) => (
          <Fragment key={side}>{size(`${t('margins')}: ${t(facing ? `side.${side}` : `side.${side === 'inside' ? 'left' : side === 'outside' ? 'right' : side}`)}`, margins[side], (points) => setMargins((now) => (linked ? { top: points, bottom: points, inside: points, outside: points } : { ...now, [side]: points })), `layout-setup-margin-${side}`, 0)}</Fragment>
        ))}
        {size(t('bleed'), bleed, setBleed, 'layout-setup-bleed', 0)}
        <NumberField label={t('columns')} value={columns} min={1} max={216} step={1} onChange={(value, final) => final && setColumns(Math.round(value))} testId="layout-setup-columns" />
        {size(t('gutter'), gutter, setGutter, 'layout-setup-gutter', 0)}
      </div>
      <Checkbox label={t('dialog.linkSides')} checked={linked} onChange={setLinked} testId="layout-setup-margin-link" />
      <Checkbox label={t('facingPages')} checked={facing} onChange={setFacing} testId="layout-setup-facing" />
      <p className="kui-muted layout-note">{t('presetCustom', { size: `${shown(width)} × ${shown(height)} ${unit}` })}</p>
    </Dialog>
  )
}
