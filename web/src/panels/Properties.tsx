import { ArrowLeftRightIcon as ArrowLeftRight, ArrowUpDownIcon as ArrowUpDown, TextAlignCenterIcon as TextAlignCenter, TextAlignEndIcon as TextAlignEnd, TextAlignJustifyIcon as TextAlignJustify, TextAlignStartIcon as TextAlignStart } from '@kition/tessera/icons/react'
import { Button, Choice, NumberField, PanelSection, Segmented, useEngineQuery, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { findItem, type DocumentInfo, type Item } from '../layout'

type Measured = { x: number; y: number; width: number; height: number; rotation: number }
type TextAttrs = {
  paragraphStyle?: string
  characterStyle?: string
  para?: { align?: string }
  chars?: { fontFamily?: string; fontStyle?: string; size?: number; leading?: { kind: string; value?: number }; tracking?: number; fill?: string }
}
type InspectedItem = Item & { fillTint?: number; opacity?: number; corner?: { shape: string; size: number } }

const ALIGNS = [
  ['left', TextAlignStart],
  ['center', TextAlignCenter],
  ['right', TextAlignEnd],
  ['justify', TextAlignJustify],
] as const

/** A family's styles (weights) to choose from, with the current one even when the family lacks it. */
function styleChoices(styles: readonly string[], current: string | undefined) {
  const list = styles.map((name) => [name, name] as const)
  return current && !styles.includes(current) ? [[current, current] as const, ...list] : list
}

/** A number with at most two decimals, as fields show it. */
const round = (value: number) => Math.round(value * 100) / 100

/**
 * The Properties panel (InDesign's Control and Properties panels): position, size, rotation and
 * flip, fill, stroke, opacity and corners of the selection; the font, size, leading, tracking,
 * alignment and styles of the text being edited or of the selected text frames, and Fit Text to
 * Frame; with nothing selected, the pages' background. Read-only (a viewer), it shows them and
 * changes nothing.
 */
export function PropertiesSection({ info }: { info: DocumentInfo }) {
  const { t, phrase: p } = useKitEditor()
  const readOnly = useReadOnly()
  const first = info.selection.length ? findItem(info, info.selection[0]) : null
  const item = first?.item as InspectedItem | undefined
  const text = info.editingText || (item?.story != null)
  if (!item && !info.editingText) {
    return (
      <PanelSection title={p('Properties')} testId="layout-properties">
        <p className="layout-empty">{t('nothingSelected')}</p>
        <fieldset className="layout-fieldset" disabled={readOnly}>
          <PageBackground info={info} />
        </fieldset>
      </PanelSection>
    )
  }
  return (
    <PanelSection title={p('Properties')} testId="layout-properties">
      <fieldset className="layout-fieldset" disabled={readOnly}>
        {item && !info.editingText ? <Geometry info={info} item={item} /> : null}
        {item && !info.editingText ? <Appearance info={info} item={item} /> : null}
        {text ? <Typography info={info} /> : null}
      </fieldset>
    </PanelSection>
  )
}

/** With nothing selected: the pages' background, a locked rectangle on their parents (none = transparent in PNG export). */
function PageBackground({ info }: { info: DocumentInfo }) {
  const { run, t } = useKitEditor()
  const current = info.spreads[0]?.pages[0]?.background ?? ''
  const swatches = info.swatchColors.filter((swatch) => swatch.kind === 'color' || swatch.kind === 'tint' || swatch.kind === 'paper')
  const hex = swatches.find((swatch) => swatch.name === current)?.color ?? '#ffffff'
  const choices = [['', t('noBackground')] as const, ...swatches.map((swatch) => [swatch.name, swatch.name] as const)]
  if (current && !swatches.some((swatch) => swatch.name === current)) choices.push([current, current])
  return (
    <div className="layout-stack" data-testid="layout-page-background">
      <span className="layout-color">
        <Choice label={t('pageBackground')} value={current} choices={choices} onChange={(color) => void run('page.background', { color: color || null })} testId="layout-background" />
        <input type="color" aria-label={t('backgroundColor')} value={hex} onChange={(event) => void run('page.background', { color: event.target.value })} data-testid="layout-background-color" />
      </span>
    </div>
  )
}

function Geometry({ info, item }: { info: DocumentInfo; item: InspectedItem }) {
  const { run, t } = useKitEditor()
  const measured = useEngineQuery<Measured>('transform.info', { ids: info.selection }, info.selection.join(','))
  if (!measured) return null
  const set = (key: keyof Measured) => (value: number, final: boolean) => {
    if (final && Number.isFinite(value)) void run('transform.set', { [key]: value, ids: info.selection, ...(key === 'rotation' ? {} : { ref: 0 }) })
  }
  return (
    <div className="layout-grid" data-testid="layout-geometry" data-item={item.id}>
      <NumberField label="X" name={t('positionX')} value={round(measured.x)} min={-100000} max={100000} step={1} unit="pt" onChange={set('x')} testId="layout-x" />
      <NumberField label="Y" name={t('positionY')} value={round(measured.y)} min={-100000} max={100000} step={1} unit="pt" onChange={set('y')} testId="layout-y" />
      <NumberField label="W" name={t('width')} value={round(measured.width)} min={0.1} max={100000} step={1} unit="pt" onChange={set('width')} testId="layout-w" />
      <NumberField label="H" name={t('height')} value={round(measured.height)} min={0.1} max={100000} step={1} unit="pt" onChange={set('height')} testId="layout-h" />
      <NumberField label="∠" name={t('rotation')} value={round(measured.rotation)} min={-360} max={360} step={1} unit="°" onChange={set('rotation')} testId="layout-rotation" />
      <span className="layout-buttons">
        <button type="button" className="kui-icon-button" aria-label={t('flipHorizontal')} title={t('flipHorizontal')} onClick={() => void run('transform.flip', { axis: 'horizontal', ids: info.selection })} data-testid="layout-flip-h">
          <ArrowLeftRight size={16} aria-hidden="true" />
        </button>
        <button type="button" className="kui-icon-button" aria-label={t('flipVertical')} title={t('flipVertical')} onClick={() => void run('transform.flip', { axis: 'vertical', ids: info.selection })} data-testid="layout-flip-v">
          <ArrowUpDown size={16} aria-hidden="true" />
        </button>
      </span>
    </div>
  )
}

function Appearance({ info, item }: { info: DocumentInfo; item: InspectedItem }) {
  const { run, t, phrase: p } = useKitEditor()
  const ids = info.selection
  const swatches = info.swatchColors.filter((swatch) => swatch.kind !== 'registration').map((swatch) => [swatch.name, swatch.name] as const)
  const known = (name: string) => (swatches.some(([value]) => value === name) ? swatches : [[name, name] as const, ...swatches])
  return (
    <div className="layout-stack" data-testid="layout-appearance">
      <Choice label={p('Fill')} value={item.fill} choices={known(item.fill)} onChange={(swatch) => void run('object.fill', { swatch, ids })} testId="layout-fill" />
      <NumberField label={t('tint')} value={Math.round((item.fillTint ?? 1) * 100)} min={0} max={100} step={1} unit="%" track onChange={(value, final) => final && void run('object.fill', { swatch: item.fill, tint: value / 100, ids })} testId="layout-tint" />
      <Choice label={p('Stroke')} value={item.stroke.swatch} choices={known(item.stroke.swatch)} onChange={(swatch) => void run('object.stroke', { swatch, ids })} testId="layout-stroke" />
      <NumberField label={t('strokeWeight')} value={round(item.stroke.weight)} min={0} max={1000} step={0.5} unit="pt" onChange={(weight, final) => final && void run('object.stroke', { weight, ids })} testId="layout-stroke-weight" />
      <NumberField label={p('Opacity')} value={Math.round((item.opacity ?? 1) * 100)} min={0} max={100} step={1} unit="%" track onChange={(value, final) => final && void run('object.opacity', { opacity: value / 100, ids })} testId="layout-opacity" />
      <NumberField
        label={t('cornerRadius')}
        value={round(item.corner && item.corner.shape !== 'none' ? item.corner.size : 0)}
        min={0}
        max={10000}
        step={1}
        unit="pt"
        onChange={(size, final) => final && void run('object.cornerOptions', { shape: size > 0 ? 'rounded' : 'none', size, ids })}
        testId="layout-corner"
      />
    </div>
  )
}

function Typography({ info }: { info: DocumentInfo }) {
  const { run, t, phrase: p, session } = useKitEditor()
  const attrs = useEngineQuery<TextAttrs>('type.selectionAttrs', {}, `${info.selection.join(',')}:${info.editingText}`)
  const families = useEngineQuery<string[]>('font.families', {}, session.doc ? 'families' : null)
  const family = attrs?.chars?.fontFamily ?? ''
  const styles = useEngineQuery<{ styles: string[] }>('font.families', { family }, family)
  if (!attrs) return null
  const chars = attrs.chars ?? {}
  const familyChoices = (families ?? []).map((name) => [name, name] as const)
  if (family && !familyChoices.some(([name]) => name === family)) familyChoices.unshift([family, family])
  const char = (next: Record<string, unknown>) => void run('type.char', { attrs: next })
  const leading = chars.leading?.kind === 'points' ? (chars.leading.value ?? 0) : 0
  return (
    <div className="layout-stack" data-testid="layout-typography">
      <Choice label={p('Font')} value={family} choices={familyChoices} onChange={(fontFamily) => char({ fontFamily })} testId="layout-font" />
      <Choice label={t('fontStyle')} value={chars.fontStyle ?? 'Regular'} choices={styleChoices(styles?.styles ?? [], chars.fontStyle)} onChange={(fontStyle) => char({ fontStyle })} testId="layout-font-style" />
      <div className="layout-grid">
        <NumberField label={t('fontSize')} value={round(chars.size ?? 12)} min={0.1} max={1296} step={0.5} unit="pt" onChange={(size, final) => final && char({ size })} testId="layout-font-size" />
        <NumberField
          label={t('leading')}
          value={round(leading)}
          min={0}
          max={5000}
          step={0.5}
          unit={leading ? 'pt' : t('auto')}
          onChange={(value, final) => final && char({ leading: value > 0 ? { kind: 'points', value } : { kind: 'auto' } })}
          testId="layout-leading"
        />
        <NumberField label={t('tracking')} value={round(chars.tracking ?? 0)} min={-1000} max={10000} step={5} onChange={(tracking, final) => final && char({ tracking })} testId="layout-tracking" />
      </div>
      <Segmented
        label={t('alignment')}
        value={attrs.para?.align ?? 'left'}
        segments={ALIGNS.map(([value, icon]) => ({ value, icon, label: t(`align.${value}`) }))}
        onChange={(align) => void run('type.para', { attrs: { align } })}
        testId="layout-align"
      />
      <Button onClick={() => void run('frame.fitText', info.editingText ? {} : { ids: info.selection })} data-testid="layout-fit-text">
        {p('Fit Text to Frame')}
      </Button>
      <Choice
        label={p('Paragraph Style')}
        value={attrs.paragraphStyle ?? ''}
        choices={info.paragraphStyles.map((name) => [name, name] as const)}
        onChange={(name) => void run('style.paragraph.apply', { name })}
        testId="layout-paragraph-style-choice"
      />
      <Choice
        label={p('Character Style')}
        value={attrs.characterStyle ?? '[None]'}
        choices={info.characterStyles.map((name) => [name, name] as const)}
        onChange={(name) => void run('style.character.apply', { name })}
        testId="layout-character-style-choice"
      />
    </div>
  )
}
