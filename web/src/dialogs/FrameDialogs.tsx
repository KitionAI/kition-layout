import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import { findItem, type DocumentInfo } from '../layout'
import { DialogFrame, useRunInDialog } from './DialogFrame'
import { CheckRow, ChoiceRow, LengthRow, NumberRow } from './controls'
import { useCommandPreview } from './preview'
import { useEditorView } from '../viewContext'

/**
 * The frame dialogs (plan 2026-11-25, stage 1): Text Frame Options, the size a click with a frame
 * or shape tool asks for, and the polygon tools' settings.
 */

const JUSTIFICATION = ['top', 'center', 'bottom', 'justify'] as const
type Inset = [number | null, number | null, number | null, number | null]

/**
 * Object › Text Frame Options…: columns, gutter, the four insets, vertical justification and
 * auto-size of the selected text frames. The engine reports a frame's columns; what it does not
 * report starts empty ("as it is") and is sent only once set, so OK never changes what the person
 * left alone.
 */
export function TextFrameOptionsDialog({ info, onClose }: { info: DocumentInfo; onClose: () => void }) {
  const { t, phrase } = useKitEditor()
  const run = useRunInDialog()
  const { view } = useEditorView()
  // The frame's columns when the dialog opened (the preview changes what the document reports).
  const [start] = useState(() => (info.selection.map((id) => findItem(info, id)?.item).find((item) => item?.story != null) as { columns?: number } | undefined)?.columns ?? 1)
  const [columns, setColumns] = useState<number>(start)
  const [gutter, setGutter] = useState<number | null>(null)
  const [inset, setInset] = useState<Inset>([null, null, null, null])
  const [justification, setJustification] = useState<'' | (typeof JUSTIFICATION)[number]>('')
  const [flags, setFlags] = useState<{ autoSize?: boolean; ignoreWrap?: boolean; balanceColumns?: boolean }>({})
  const params = {
    ...(columns !== start ? { columns } : {}),
    ...(gutter !== null ? { gutter } : {}),
    ...(inset.some((side) => side !== null) ? { inset } : {}),
    ...(justification ? { verticalJustification: justification } : {}),
    ...flags,
  }
  const preview = useCommandPreview('object.textFrameOptions', params)
  const side = (index: number, key: string) => (
    <LengthRow
      key={key}
      label={t(`side.${key}`)}
      value={inset[index]}
      unit={view.unit}
      placeholder={t('unchanged')}
      onChange={(next) => setInset((now) => now.map((value, at) => (at === index ? (next !== null && next >= 0 ? next : null) : value)) as Inset)}
      testId={`layout-tfo-inset-${key}`}
    />
  )
  return (
    <DialogFrame command="object.textFrameOptions" title={phrase('Text Frame Options')} preview={preview} onOk={async () => (Object.keys(params).length && !(await preview?.keep(params)) ? run('object.textFrameOptions', params) : undefined)} onClose={onClose}>
      <div className="layout-dialog-fields">
        <NumberRow label={t('columns')} value={columns} min={1} max={40} decimals={0} onChange={(next) => next !== null && next >= 1 && setColumns(Math.round(next))} testId="layout-tfo-columns" />
        <LengthRow label={t('gutter')} value={gutter} unit={view.unit} placeholder={t('unchanged')} onChange={(next) => setGutter(next !== null && next >= 0 ? next : null)} testId="layout-tfo-gutter" />
        <fieldset className="layout-dialog-group">
          <legend>{t('dialog.inset')}</legend>
          {side(0, 'top')}
          {side(1, 'left')}
          {side(2, 'bottom')}
          {side(3, 'right')}
        </fieldset>
        <ChoiceRow label={t('param.verticalJustification')} value={justification} choices={[['', t('unchanged')] as const, ...JUSTIFICATION.map((value) => [value, t(`option.${value}`)] as const)]} onChange={setJustification} testId="layout-tfo-justification" />
        {(['autoSize', 'balanceColumns', 'ignoreWrap'] as const).map((key) => (
          <CheckRow key={key} label={t(`param.${key}`)} checked={flags[key] === true} onChange={(next) => setFlags((now) => ({ ...now, [key]: next }))} testId={`layout-tfo-${key}`} />
        ))}
      </div>
    </DialogFrame>
  )
}

/** What a click with a frame or shape tool asks (the engine's `frameSize` request). */
export type FrameSizeRequest = { spread: unknown; shape?: string; content?: string; x: number; y: number; width: number; height: number }

/** A click with a frame or shape tool: the new frame's width and height, its top left at the click. */
export function FrameSizeDialog({ request, onClose }: { request: FrameSizeRequest; onClose: () => void }) {
  const { t } = useKitEditor()
  const { view } = useEditorView()
  const run = useRunInDialog()
  const [width, setWidth] = useState(request.width)
  const [height, setHeight] = useState(request.height)
  const valid = width > 0 && height > 0
  return (
    <DialogFrame
      command="frame.add"
      title={t(`dialog.frameSize.${request.shape === 'ellipse' ? 'ellipse' : request.shape === 'polygon' ? 'polygon' : 'rectangle'}`)}
      okDisabled={!valid}
      onOk={() => run('frame.add', { spread: request.spread, rect: [request.x, request.y, request.x + width, request.y + height], ...(request.shape ? { shape: request.shape } : {}), ...(request.content ? { content: request.content } : {}) })}
      onClose={onClose}
    >
      <div className="layout-dialog-fields">
        <LengthRow label={t('width')} value={width} unit={view.unit} onChange={(next) => next !== null && setWidth(next)} testId="layout-frame-width" />
        <LengthRow label={t('height')} value={height} unit={view.unit} onChange={(next) => next !== null && setHeight(next)} testId="layout-frame-height" />
      </div>
    </DialogFrame>
  )
}

/** The polygon tools' settings (a double click on their button): the number of sides and the star inset. */
export function PolygonSettingsDialog({ onClose }: { onClose: () => void }) {
  const { t, session } = useKitEditor()
  const run = useRunInDialog()
  const [sides, setSides] = useState(6)
  const [inset, setInset] = useState(0)
  useEffect(() => {
    // The preferences as they are: `prefs.set` with nothing to set answers them and changes nothing.
    void session.run('prefs.set', {}).then((answer) => {
      const prefs = answer ? (JSON.parse(answer) as { polygonSides?: number; starInset?: number }) : null
      if (typeof prefs?.polygonSides === 'number') setSides(prefs.polygonSides)
      if (typeof prefs?.starInset === 'number') setInset(prefs.starInset)
    })
  }, [session])
  return (
    <DialogFrame command="prefs.set" title={t('dialog.polygonSettings')} okDisabled={!(sides >= 3 && sides <= 100 && inset >= 0 && inset <= 100)} onOk={() => run('prefs.set', { polygonSides: sides, starInset: inset })} onClose={onClose}>
      <div className="layout-dialog-fields">
        <NumberRow label={t('param.polygonSides')} value={sides} min={3} max={100} decimals={0} onChange={(next) => next !== null && setSides(Math.round(next))} testId="layout-polygon-sides" />
        <NumberRow label={t('param.starInset')} value={inset} min={0} max={100} decimals={0} unit="%" onChange={(next) => next !== null && setInset(next)} testId="layout-polygon-inset" />
      </div>
    </DialogFrame>
  )
}
