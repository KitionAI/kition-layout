import { RectangleHorizontalIcon, RectangleVerticalIcon } from '@kition/tessera/icons/react'
import { Checkbox, Choice, NumberField, Segmented, useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../../layout'
import { useEditorView } from '../../viewContext'
import { ColorWell } from '../ColorWell'
import { rgbToHex, pickedRgb, swatchColor } from '../colors'
import { LengthField, PropertyGroup } from '../fields'

type Margins = { top: number; bottom: number; inside: number; outside: number }

/**
 * With nothing selected, the document (InDesign's Properties panel with no selection): page width
 * and height, orientation, facing pages, the margins and columns of every page (the first page's
 * shown), and the pages' background.
 */
export function DocumentGroup({ info }: { info: DocumentInfo }) {
  const { run, t } = useKitEditor()
  const { unit } = useEditorView().view
  const { pageWidth: width, pageHeight: height, facingPages } = info.settings
  const first = info.spreads[0]?.pages[0]
  const margins: Margins = first?.margins ?? { top: 36, bottom: 36, inside: 36, outside: 36 }
  const setup = (params: Record<string, unknown>) => void run('layout.documentSetup', params)
  const setMargin = (side: keyof Margins) => (points: number) => void run('layout.marginsAndColumns', { margins: { ...margins, [side]: points } })
  return (
    <PropertyGroup title={t('props.document')} testId="layout-document">
      <div className="layout-grid layout-grid-tight">
        <LengthField label="W" name={t('props.pageWidth')} value={width} unit={unit} min={1} onCommit={(next) => setup({ width: next })} testId="layout-doc-width" />
        <LengthField label="H" name={t('props.pageHeight')} value={height} unit={unit} min={1} onCommit={(next) => setup({ height: next })} testId="layout-doc-height" />
      </div>
      <div className="layout-stack">
        <Segmented
          label={t('props.orientation')}
          value={width > height ? 'landscape' : 'portrait'}
          segments={[
            { value: 'portrait', icon: RectangleVerticalIcon, label: t('props.portrait') },
            { value: 'landscape', icon: RectangleHorizontalIcon, label: t('props.landscape') },
          ]}
          onChange={(next) => {
            if ((next === 'landscape') !== (width > height)) setup({ width: height, height: width })
          }}
          testId="layout-orientation"
        />
        <Checkbox label={t('props.facingPages')} checked={facingPages} onChange={(next) => setup({ facingPages: next })} testId="layout-facing" />
      </div>
      <h4 className="layout-property-subtitle">{t('props.margins')}</h4>
      <div className="layout-grid layout-grid-tight">
        {(['top', 'bottom', 'inside', 'outside'] as const).map((side) => (
          <LengthField key={side} label={t(`side.${side}`)} value={margins[side]} unit={unit} min={0} onCommit={setMargin(side)} testId={`layout-doc-margin-${side}`} />
        ))}
        <NumberField label={t('props.columns')} value={first?.columns ?? 1} min={1} max={40} step={1} onChange={(columns, final) => final && void run('layout.marginsAndColumns', { columns: Math.round(columns) })} testId="layout-doc-columns" />
      </div>
      <PageBackground info={info} />
    </PropertyGroup>
  )
}

/** The pages' background, a locked rectangle on their parents (none = transparent in PNG export). */
function PageBackground({ info }: { info: DocumentInfo }) {
  const { run, t } = useKitEditor()
  const current = info.spreads[0]?.pages[0]?.background ?? ''
  const swatches = info.swatchColors.filter((swatch) => swatch.kind === 'color' || swatch.kind === 'tint' || swatch.kind === 'paper')
  const choices = [['', t('noBackground')] as const, ...swatches.map((swatch) => [swatch.name, swatch.name] as const)]
  if (current && !swatches.some((swatch) => swatch.name === current)) choices.push([current, info.colors[current] ? t('props.unnamedColor') : current])
  return (
    <div className="layout-stack" data-testid="layout-page-background">
      <span className="layout-color">
        <Choice label={t('pageBackground')} value={current} choices={choices} onChange={(color) => void run('page.background', { color: color || null })} testId="layout-background" />
        <ColorWell label={t('backgroundColor')} color={current ? swatchColor(info.colors[current]) : null} onChange={(color, final) => final && void run('page.background', { color: rgbToHex(pickedRgb(color)) })} testId="layout-background-color" />
      </span>
    </div>
  )
}
