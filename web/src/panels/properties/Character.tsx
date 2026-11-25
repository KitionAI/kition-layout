import { CaseUpperIcon, CaseSensitiveIcon, StrikethroughIcon, SubscriptIcon, SuperscriptIcon, UnderlineIcon } from '@kition/tessera/icons/react'
import { Choice, IconButton, NumberField, toast, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../../layout'
import { ColorWell } from '../ColorWell'
import { colorParam, swatchColor, type PickedColor } from '../colors'
import { PropertyGroup } from '../fields'
import { FontMenu } from '../FontMenu'
import { LANGUAGES } from '../panelLogic'
import { round } from './Transform'

/** The resolved character attributes at the text selection (`text.selectionAttrs`'s `chars`). */
export type CharAttrs = {
  fontFamily?: string
  fontStyle?: string
  size?: number
  leading?: { kind: string; value?: number }
  kerning?: { kind: string; value?: number }
  tracking?: number
  hScale?: number
  vScale?: number
  baselineShift?: number
  skew?: number
  fill?: string
  stroke?: string
  strokeWeight?: number
  capitalization?: string
  position?: string
  underline?: boolean
  strikethrough?: boolean
  language?: string
}

/** A family's styles (weights) to choose from, with the current one even when the family lacks it. */
function styleChoices(styles: readonly string[], current: string | undefined) {
  const list = styles.map((name) => [name, name] as const)
  return current && !styles.includes(current) ? [[current, current] as const, ...list] : list
}

/**
 * Character (InDesign's Character panel and the Control panel's character row): font (with the
 * searchable font menu), style, size, leading, kerning, tracking, vertical and horizontal scale,
 * baseline shift, skew, All Caps, Small Caps, superscript, subscript, underline, strikethrough,
 * language, and the text's fill and stroke. Each change sets the selected text (or the selected
 * frames' stories).
 */
export function CharacterGroup({ info, chars }: { info: DocumentInfo; chars: CharAttrs }) {
  const { run, t, session } = useKitEditor()
  const families = useEngineQuery<string[]>('font.families', {}, session.doc ? 'families' : null) ?? []
  const family = chars.fontFamily ?? ''
  const styles = useEngineQuery<{ styles: string[] }>('font.families', { family }, family)
  const char = (next: Record<string, unknown>) => void run('text.char', { attrs: next })
  const familyChoices = families.map((name) => [name, name] as const)
  if (family && !familyChoices.some(([name]) => name === family)) familyChoices.unshift([family, family])
  const leading = chars.leading?.kind === 'points' ? (chars.leading.value ?? 0) : 0
  const kerning = chars.kerning?.kind ?? 'metrics'
  const toggles = [
    ['allCaps', CaseUpperIcon, chars.capitalization === 'allCaps', { capitalization: chars.capitalization === 'allCaps' ? 'normal' : 'allCaps' }],
    ['smallCaps', CaseSensitiveIcon, chars.capitalization === 'smallCaps', { capitalization: chars.capitalization === 'smallCaps' ? 'normal' : 'smallCaps' }],
    ['superscript', SuperscriptIcon, chars.position === 'superscript', { position: chars.position === 'superscript' ? 'normal' : 'superscript' }],
    ['subscript', SubscriptIcon, chars.position === 'subscript', { position: chars.position === 'subscript' ? 'normal' : 'subscript' }],
    ['underline', UnderlineIcon, Boolean(chars.underline), { underline: !chars.underline }],
    ['strikethrough', StrikethroughIcon, Boolean(chars.strikethrough), { strikethrough: !chars.strikethrough }],
  ] as const
  const languages = LANGUAGES.map((name) => [name, t(`lang.${name}`)] as const)
  const language = chars.language ?? 'English: USA'
  const languageChoices = languages.some(([name]) => name === language) ? languages : [[language, language] as const, ...languages]
  /** The text's colour from the picker: a swatch of that colour (made if none has it), then set. */
  const textColor = (key: 'fill' | 'stroke') => async (color: PickedColor, final: boolean) => {
    if (!final) return
    const value = JSON.stringify(colorParam(color))
    const existing = info.swatchColors.find((swatch) => {
      const known = swatchColor(info.colors[swatch.name])
      return known && JSON.stringify(colorParam(known)) === value
    })
    const name = existing?.name ?? (JSON.parse((await run('swatch.add', { color: colorParam(color) })) ?? 'null') as { name?: string } | null)?.name
    if (name) char({ [key]: name })
    else toast('info', t('props.colorFailed'))
  }
  return (
    <PropertyGroup title={t('props.character')} testId="layout-character">
      <div className="layout-stack">
        <span className="layout-font-field">
          <Choice label={t('props.font')} value={family} choices={familyChoices} onChange={(fontFamily) => char({ fontFamily })} testId="layout-font" />
          <FontMenu families={families} current={family} onPick={(fontFamily) => char({ fontFamily })} testId="layout-font" />
        </span>
        <Choice label={t('fontStyle')} value={chars.fontStyle ?? 'Regular'} choices={styleChoices(styles?.styles ?? [], chars.fontStyle)} onChange={(fontStyle) => char({ fontStyle })} testId="layout-font-style" />
        <div className="layout-grid layout-grid-tight">
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
          <Choice label={t('props.kerning')} value={kerning} choices={(['metrics', 'optical', 'none'] as const).map((value) => [value, t(`kerning.${value}`)] as const)} onChange={(kind) => char({ kerning: { kind } })} testId="layout-kerning" />
          <NumberField label={t('tracking')} value={round(chars.tracking ?? 0)} min={-1000} max={10000} step={5} onChange={(tracking, final) => final && char({ tracking })} testId="layout-tracking" />
          <NumberField label={t('props.vScale')} value={round((chars.vScale ?? 1) * 100)} min={1} max={1000} step={1} unit="%" onChange={(value, final) => final && char({ vScale: value / 100 })} testId="layout-v-scale" />
          <NumberField label={t('props.hScale')} value={round((chars.hScale ?? 1) * 100)} min={1} max={1000} step={1} unit="%" onChange={(value, final) => final && char({ hScale: value / 100 })} testId="layout-h-scale" />
          <NumberField label={t('props.baselineShift')} value={round(chars.baselineShift ?? 0)} min={-5000} max={5000} step={0.5} unit="pt" onChange={(baselineShift, final) => final && char({ baselineShift })} testId="layout-baseline-shift" />
          <NumberField label={t('props.skew')} value={round(chars.skew ?? 0)} min={-85} max={85} step={1} unit="°" onChange={(skew, final) => final && char({ skew })} testId="layout-skew" />
        </div>
        <div className="layout-variant-row" role="group" aria-label={t('props.characterStyle')} data-testid="layout-char-toggles">
          {toggles.map(([key, icon, on, attrs]) => (
            <IconButton key={key} icon={icon} label={t(`char.${key}`)} pressed={on} onClick={() => char(attrs)} data-testid={`layout-char-${key}`} />
          ))}
        </div>
        <Choice label={t('props.language')} value={language} choices={languageChoices} onChange={(name) => char({ language: name })} testId="layout-language" />
        <span className="layout-color">
          <Choice label={t('props.textFill')} value={chars.fill ?? '[Black]'} choices={textSwatches(info, chars.fill ?? '[Black]')} onChange={(fill) => char({ fill })} testId="layout-text-fill" />
          <ColorWell label={t('props.textFillColor')} color={swatchColor(info.colors[chars.fill ?? '[Black]'])} onChange={(color, final) => void textColor('fill')(color, final)} testId="layout-text-fill-well" />
        </span>
        <span className="layout-color">
          <Choice label={t('props.textStroke')} value={chars.stroke ?? '[None]'} choices={textSwatches(info, chars.stroke ?? '[None]')} onChange={(stroke) => char({ stroke })} testId="layout-text-stroke" />
          <ColorWell label={t('props.textStrokeColor')} color={swatchColor(info.colors[chars.stroke ?? '[None]'])} onChange={(color, final) => void textColor('stroke')(color, final)} testId="layout-text-stroke-well" />
        </span>
      </div>
    </PropertyGroup>
  )
}

function textSwatches(info: DocumentInfo, current: string) {
  const list = info.swatchColors.filter((swatch) => swatch.kind !== 'registration' && swatch.kind !== 'gradient').map((swatch) => [swatch.name, swatch.name] as const)
  return list.some(([name]) => name === current) ? list : [[current, current] as const, ...list]
}
