import { Button, Choice, NumberField, Segmented, toast, useKitEditor } from '@kition/sdk/editor'
import { useDialogs } from '../dialogsContext'
import { BLEND_MODES } from '../effects'
import type { DocumentInfo, Item } from '../../layout'
import { useEditorView } from '../../viewContext'
import { ColorWell } from '../ColorWell'
import { colorParam, swatchColor, type PickedColor } from '../colors'
import { LengthField, PropertyGroup } from '../fields'
import { useLiveRun } from '../live'
import { ARROWHEADS, CORNER_SHAPES, STROKE_TYPES, strokeType } from '../panelLogic'
import { round } from './Transform'

/** What `inspect()` reports of an object beyond `Item` (the read-backs of plan stage 4). */
export type InspectedItem = Item & {
  fillTint?: number
  opacity?: number
  corner?: { shape: string; size: number }
  corners?: Array<{ shape: string; size: number }>
  strokeDetail?: { swatch: string; tint: number; weight: number; type?: { kind: string }; align?: string; cap?: string; join?: string; miterLimit?: number; start?: string; end?: string; gapSwatch?: string }
  textFrame?: { columns: number; gutter: number; inset: [number, number, number, number]; verticalJustification: string; autoSize?: string }
  wrap?: { mode: string; offsets?: [number, number, number, number] }
  /** The blend mode and every effect (plan stage 7, the Effects dialog). */
  blend?: string
  effects?: Record<string, unknown>
}

/** The swatches to choose from, with `name` even when it is an unnamed colour. */
export function swatchChoices(info: DocumentInfo, name: string, unnamed: string) {
  const list = info.swatchColors.filter((swatch) => swatch.kind !== 'registration').map((swatch) => [swatch.name, swatch.name] as const)
  return list.some(([value]) => value === name) ? list : [[name, info.colors[name] ? unnamed : name] as const, ...list]
}

/**
 * Adds a colour as a swatch: the object's unnamed colour becomes a named swatch
 * (`swatch.addToSwatches`); a colour already named makes a new swatch of it (`swatch.add`).
 */
export function useAddSwatch() {
  const { run, t } = useKitEditor()
  return async (info: DocumentInfo, current: string, color: PickedColor, target: 'fill' | 'stroke') => {
    const named = info.swatchColors.some((swatch) => swatch.name === current)
    const answer = named ? await run('swatch.add', { color: colorParam(color) }) : await run('swatch.addToSwatches', { swatch: current, target })
    if (answer) toast('info', t('props.swatchAdded'))
  }
}

/**
 * Appearance (InDesign's Swatches, Stroke and Effects basics, the Control panel's corners): fill and
 * stroke from a swatch or the colour picker (an unnamed colour, Add to Swatches), tint, weight, the
 * stroke's type, caps, joins, alignment and arrowheads, opacity, and the corners' shape and size.
 */
export function AppearanceGroup({ info, item }: { info: DocumentInfo; item: InspectedItem }) {
  const { run, t, phrase: p } = useKitEditor()
  const dialogs = useDialogs()
  const { unit } = useEditorView().view
  const live = useLiveRun()
  const addSwatch = useAddSwatch()
  const ids = info.selection
  const stroke = item.strokeDetail
  const weight = stroke?.weight ?? item.stroke.weight
  const corner = item.corner ?? { shape: 'none', size: 0 }
  const strokeParams = (params: Record<string, unknown>) => void run('object.stroke', { ...params, ids })
  const well = (target: 'fill' | 'stroke', name: string) => (
    <ColorWell
      label={t(target === 'fill' ? 'props.fillColor' : 'props.strokeColor')}
      color={swatchColor(info.colors[name])}
      onChange={(color, final) => void live('object.color', { color: colorParam(color), target, ids }, final)}
      onAddSwatch={(color) => void addSwatch(info, name, color, target)}
      testId={`layout-${target}-well`}
    />
  )
  return (
    <PropertyGroup title={t('props.appearance')} testId="layout-appearance">
      <div className="layout-stack">
        <span className="layout-color">
          <Choice label={p('Fill')} value={item.fill} choices={swatchChoices(info, item.fill, t('props.unnamedColor'))} onChange={(swatch) => void run('object.fill', { swatch, ids })} testId="layout-fill" />
          {well('fill', item.fill)}
        </span>
        <NumberField label={t('tint')} value={Math.round((item.fillTint ?? 1) * 100)} min={0} max={100} step={1} unit="%" track onChange={(value, final) => final && void run('object.fill', { swatch: item.fill, tint: value / 100, ids })} testId="layout-tint" />
        <span className="layout-color">
          <Choice label={p('Stroke')} value={item.stroke.swatch} choices={swatchChoices(info, item.stroke.swatch, t('props.unnamedColor'))} onChange={(swatch) => strokeParams({ swatch })} testId="layout-stroke" />
          {well('stroke', item.stroke.swatch)}
        </span>
        <div className="layout-grid layout-grid-tight">
          <NumberField label={t('strokeWeight')} value={round(weight)} min={0} max={1000} step={0.5} unit="pt" onChange={(value, final) => final && strokeParams({ weight: value })} testId="layout-stroke-weight" />
          <Choice label={t('props.strokeType')} value={stroke?.type?.kind ?? 'solid'} choices={STROKE_TYPES.map((kind) => [kind, t(`stroke.${kind}`)] as const)} onChange={(kind) => strokeParams({ type: strokeType(kind, weight) })} testId="layout-stroke-type" />
        </div>
        <Segmented label={t('props.strokeAlign')} value={stroke?.align ?? 'center'} segments={(['center', 'inside', 'outside'] as const).map((value) => ({ value, label: t(`strokeAlign.${value}`) }))} onChange={(align) => strokeParams({ align })} testId="layout-stroke-align" />
        <Segmented label={t('props.cap')} value={stroke?.cap ?? 'butt'} segments={(['butt', 'round', 'projecting'] as const).map((value) => ({ value, label: t(`cap.${value}`) }))} onChange={(cap) => strokeParams({ cap })} testId="layout-stroke-cap" />
        <Segmented label={t('props.join')} value={stroke?.join ?? 'miter'} segments={(['miter', 'round', 'bevel'] as const).map((value) => ({ value, label: t(`join.${value}`) }))} onChange={(join) => strokeParams({ join })} testId="layout-stroke-join" />
        <div className="layout-grid layout-grid-tight">
          <Choice label={t('props.arrowStart')} value={stroke?.start ?? 'none'} choices={ARROWHEADS.map((value) => [value, t(`arrow.${value}`)] as const)} onChange={(start) => strokeParams({ start })} testId="layout-arrow-start" />
          <Choice label={t('props.arrowEnd')} value={stroke?.end ?? 'none'} choices={ARROWHEADS.map((value) => [value, t(`arrow.${value}`)] as const)} onChange={(end) => strokeParams({ end })} testId="layout-arrow-end" />
        </div>
        <NumberField label={p('Opacity')} value={Math.round((item.opacity ?? 1) * 100)} min={0} max={100} step={1} unit="%" track onChange={(value, final) => final && void run('object.opacity', { opacity: value / 100, ids })} testId="layout-opacity" />
        <span className="layout-color">
          <Choice label={t('effects.blend')} value={item.blend ?? 'Normal'} choices={BLEND_MODES.map((mode) => [mode, t(`blend.${mode}`)] as const)} onChange={(blend) => void run('object.opacity', { opacity: item.opacity ?? 1, blend, ids })} testId="layout-blend" />
          <Button onClick={() => dialogs?.current?.show('effects')} data-testid="layout-effects-button">
            {p('Effects…')}
          </Button>
        </span>
        <div className="layout-grid layout-grid-tight">
          <Choice
            label={t('props.cornerShape')}
            value={corner.shape}
            choices={CORNER_SHAPES.map((value) => [value, t(`corner.${value}`)] as const)}
            onChange={(shape) => void run('object.cornerOptions', { shape, size: shape === 'none' ? 0 : corner.size > 0 ? corner.size : 12, ids })}
            testId="layout-corner-shape"
          />
          <LengthField
            label={t('cornerRadius')}
            value={corner.shape !== 'none' ? corner.size : 0}
            unit={unit}
            min={0}
            onCommit={(size) => void run('object.cornerOptions', { shape: size > 0 ? (corner.shape !== 'none' ? corner.shape : 'rounded') : 'none', size, ids })}
            testId="layout-corner"
          />
        </div>
      </div>
    </PropertyGroup>
  )
}
