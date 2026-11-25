import { useState } from 'react'
import {
  AlignCenterHorizontalIcon,
  AlignCenterVerticalIcon,
  AlignEndHorizontalIcon,
  AlignEndVerticalIcon,
  AlignHorizontalJustifyCenterIcon,
  AlignStartHorizontalIcon,
  AlignStartVerticalIcon,
  AlignVerticalJustifyCenterIcon,
  ArrowDownToLineIcon,
  CropIcon,
  ExpandIcon,
  Maximize2Icon,
  MoveHorizontalIcon,
  MoveVerticalIcon,
  ShrinkIcon,
  SquareDashedIcon,
  SquareIcon,
  SquaresExcludeIcon,
  SquaresIntersectIcon,
  SquaresSubtractIcon,
  SquaresUniteIcon,
  SquareXIcon,
  CircleDashedIcon,
  Columns2Icon,
  SquareSquareIcon,
  type IconComponent,
} from '@kition/tessera/icons/react'
import { Choice, NumberField, Segmented, useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../../layout'
import { useEditorView } from '../../viewContext'
import { LengthField, PropertyGroup, VariantButtons } from '../fields'
import { ALIGN_ROWS, ALIGN_TO, DISTRIBUTE_ROWS, FIT_ROWS, PATHFINDER_ROWS, WRAP_ROWS, type AlignTo } from '../panelLogic'
import type { InspectedItem } from './Appearance'

const ALIGN_ICONS: Record<string, IconComponent> = {
  left: AlignStartVerticalIcon,
  hcenter: AlignCenterVerticalIcon,
  right: AlignEndVerticalIcon,
  top: AlignStartHorizontalIcon,
  vcenter: AlignCenterHorizontalIcon,
  bottom: AlignEndHorizontalIcon,
  horizontalCenters: AlignHorizontalJustifyCenterIcon,
  verticalCenters: AlignVerticalJustifyCenterIcon,
  horizontalSpace: MoveHorizontalIcon,
  verticalSpace: MoveVerticalIcon,
}
const WRAP_ICONS: Record<string, IconComponent> = { none: SquareXIcon, boundingBox: SquareIcon, contour: CircleDashedIcon, jumpObject: ArrowDownToLineIcon, jumpToNextColumn: Columns2Icon }
const PATHFINDER_ICONS: Record<string, IconComponent> = { add: SquaresUniteIcon, subtract: SquaresSubtractIcon, intersect: SquaresIntersectIcon, exclude: SquaresExcludeIcon, minusBack: SquareSquareIcon }
const FIT_ICONS: Record<string, IconComponent> = { fillProportionally: ExpandIcon, fitProportionally: ShrinkIcon, fitContentToFrame: Maximize2Icon, fitFrameToContent: CropIcon, centerContent: SquareDashedIcon }

/** What Align aligns to, kept while the editor is open. */
let lastAlignTo: AlignTo = 'selection'

/**
 * Text Wrap (its five modes as the Object menu's rows, and the offsets for the four sides), Align
 * (six edges, four distributions, and what they align to), Pathfinder (two or more objects) and
 * Fitting (a frame's content). The buttons are the menus' rows, run through the dialog host.
 */
export function ArrangeGroups({ info, item }: { info: DocumentInfo; item: InspectedItem }) {
  const { run, t } = useKitEditor()
  const { unit } = useEditorView().view
  const [to, setToState] = useState<AlignTo>(lastAlignTo)
  const setTo = (next: AlignTo) => {
    lastAlignTo = next
    setToState(next)
  }
  const ids = info.selection
  const mode = item.wrap?.mode ?? 'none'
  const offsets = item.wrap?.offsets ?? [0, 0, 0, 0]
  const setOffset = (side: number) => (points: number) => {
    const next = [...offsets]
    next[side] = points
    void run('object.textWrap', { mode, offset: next, ids })
  }
  const several = ids.length > 1
  return (
    <>
      <PropertyGroup title={t('props.textWrap')} testId="layout-text-wrap">
        <VariantButtons rows={WRAP_ROWS} icons={WRAP_ICONS} active={mode} testId="layout-wrap" />
        {mode !== 'none' ? (
          <div className="layout-grid layout-grid-tight">
            {(['top', 'left', 'bottom', 'right'] as const).map((side, index) => (
              <LengthField key={side} label={t(`side.${side}`)} value={offsets[index]} unit={unit} onCommit={setOffset(index)} testId={`layout-wrap-${side}`} />
            ))}
          </div>
        ) : null}
      </PropertyGroup>
      <PropertyGroup title={t('props.align')} testId="layout-align-panel">
        <div className="layout-stack">
          <Choice label={t('props.alignTo')} value={to} choices={ALIGN_TO.map((value) => [value, t(`alignTo.${value}`)] as const)} onChange={setTo} testId="layout-align-to" />
        </div>
        <VariantButtons rows={ALIGN_ROWS} icons={ALIGN_ICONS} params={{ to }} disabled={!several && to === 'selection'} testId="layout-align-edge" />
        <VariantButtons rows={DISTRIBUTE_ROWS} icons={ALIGN_ICONS} disabled={ids.length < 3} testId="layout-distribute" />
      </PropertyGroup>
      {several ? (
        <PropertyGroup title={t('props.pathfinder')} testId="layout-pathfinder">
          <VariantButtons rows={PATHFINDER_ROWS} icons={PATHFINDER_ICONS} testId="layout-pathfinder-op" />
        </PropertyGroup>
      ) : null}
      {item.asset != null ? (
        <PropertyGroup title={t('props.fitting')} testId="layout-fitting">
          <VariantButtons rows={FIT_ROWS} icons={FIT_ICONS} testId="layout-fit" />
        </PropertyGroup>
      ) : null}
    </>
  )
}

/** Text Frame Options in the panel: columns, gutter, inset spacing and vertical justification. */
export function TextFrameGroup({ info, item }: { info: DocumentInfo; item: InspectedItem }) {
  const { run, t } = useKitEditor()
  const { unit } = useEditorView().view
  const frame = item.textFrame
  const ids = info.selection
  const set = (params: Record<string, unknown>) => void run('object.textFrameOptions', { ...params, ids })
  const inset = frame?.inset ?? null
  return (
    <PropertyGroup title={t('props.textFrame')} testId="layout-text-frame">
      <div className="layout-grid layout-grid-tight">
        <NumberField label={t('props.columns')} value={frame?.columns ?? item.columns ?? 1} min={1} max={40} step={1} onChange={(columns, final) => final && set({ columns: Math.round(columns) })} testId="layout-frame-columns" />
        <LengthField label={t('props.gutter')} value={frame?.gutter ?? null} unit={unit} min={0} onCommit={(gutter) => set({ gutter })} testId="layout-frame-gutter" />
        {(['top', 'left', 'bottom', 'right'] as const).map((side, index) => (
          <LengthField
            key={side}
            label={t(`side.${side}`)}
            name={t('props.inset', { side: t(`side.${side}`) })}
            value={inset ? inset[index] : null}
            unit={unit}
            min={0}
            onCommit={(points) => set({ inset: [0, 1, 2, 3].map((at) => (at === index ? points : null)) })}
            testId={`layout-frame-inset-${side}`}
          />
        ))}
      </div>
      <Segmented
        label={t('props.verticalJustification')}
        value={frame?.verticalJustification ?? 'top'}
        segments={(['top', 'center', 'bottom', 'justify'] as const).map((value) => ({ value, label: t(`vj.${value}`) }))}
        onChange={(verticalJustification) => set({ verticalJustification })}
        testId="layout-frame-vj"
      />
    </PropertyGroup>
  )
}
