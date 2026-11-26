import { useState } from 'react'
import { ArrowLeftRightIcon as ArrowLeftRight, ArrowUpDownIcon as ArrowUpDown, LinkIcon as Link, UnlinkIcon as Unlink } from '@kition/tessera/icons/react'
import { IconButton, NumberField, useKitEditor } from '@kition/sdk/editor'
import { usePanelQuery } from '../query'
import type { DocumentInfo, Item } from '../../layout'
import { useEditorView } from '../../viewContext'
import { LengthField, PropertyGroup } from '../fields'
import { REF_POINTS, keptProportion, shownPosition } from '../panelLogic'

type Measured = { x: number; y: number; width: number; height: number; rotation: number; shear?: number; scaleX?: number; scaleY?: number }

/** A number with at most two decimals, as fields show it. */
export const round = (value: number) => Math.round(value * 100) / 100

/** The reference point the panel measures from, kept while the editor is open (top left until chosen). */
let lastRef = 0

/**
 * Transform (InDesign's Control and Transform panels): the reference point, X and Y of it, width
 * and height (kept in proportion when linked), scale, rotation and shear about it, and the flips.
 * Lengths are in the ruler unit and take any unit typed. With a frame's graphic selected (plan
 * stage 6) they are the graphic's: its place, size and scale inside the frame.
 */
export function TransformGroup({ info, item }: { info: DocumentInfo; item: Item }) {
  const { run, t } = useKitEditor()
  const { unit } = useEditorView().view
  const [ref, setRefState] = useState(lastRef)
  const [linked, setLinked] = useState(false)
  const setRef = (next: number) => {
    lastRef = next
    setRefState(next)
  }
  const ids = info.selection
  const content = info.selectionContent
  const measured = usePanelQuery<Measured>('transform.info', content ? { ids, content: true } : { ids }, `${ids.join(',')}:${content}`)
  if (!measured) return null
  const at = shownPosition(measured, ref)
  const set = (params: Record<string, unknown>) => void run('transform.set', { ...params, ref, ids, ...(content ? { content: true } : {}) })
  const size = (key: 'width' | 'height') => (points: number) => {
    if (!(points > 0)) return
    const other = linked ? keptProportion(measured, key, points) : null
    set(other ? { [key]: points, [key === 'width' ? 'height' : 'width']: other } : { [key]: points })
  }
  const scale = (key: 'scaleX' | 'scaleY') => (value: number, final: boolean) => {
    if (!final || !Number.isFinite(value) || value === 0) return
    set(linked ? { scaleX: value, scaleY: value } : { [key]: value })
  }
  return (
    <PropertyGroup title={t(content ? 'props.content' : 'props.transform')} testId="layout-transform">
      <div className="layout-transform">
        <div className="layout-ref-grid" role="radiogroup" aria-label={t('props.referencePoint')} data-testid="layout-ref-point">
          {REF_POINTS.map((point) => (
            <button key={point} type="button" role="radio" aria-checked={point === ref} aria-label={t(`props.ref.${point}`)} title={t(`props.ref.${point}`)} className="layout-ref-dot" onClick={() => setRef(point)} data-testid={`layout-ref-${point}`} />
          ))}
        </div>
        <div className="layout-grid layout-grid-tight" data-testid="layout-geometry" data-item={item.id} data-content={content || undefined}>
          <LengthField label="X" name={t('positionX')} value={at.x} unit={unit} onCommit={(x) => set({ x })} testId="layout-x" />
          <LengthField label="W" name={t('width')} value={measured.width} unit={unit} min={0.01} onCommit={size('width')} testId="layout-w" />
          <LengthField label="Y" name={t('positionY')} value={at.y} unit={unit} onCommit={(y) => set({ y })} testId="layout-y" />
          <LengthField label="H" name={t('height')} value={measured.height} unit={unit} min={0.01} onCommit={size('height')} testId="layout-h" />
        </div>
      </div>
      <div className="layout-grid layout-grid-tight">
        <NumberField label={t('props.scaleX')} value={round(measured.scaleX ?? 100)} min={-100000} max={100000} step={1} unit="%" onChange={scale('scaleX')} testId="layout-scale-x" />
        <NumberField label={t('props.scaleY')} value={round(measured.scaleY ?? 100)} min={-100000} max={100000} step={1} unit="%" onChange={scale('scaleY')} testId="layout-scale-y" />
        {/* A graphic turns and slants with its frame. */}
        {content ? null : (
          <>
            <NumberField label="∠" name={t('rotation')} value={round(measured.rotation)} min={-360} max={360} step={1} unit="°" onChange={(rotation, final) => final && set({ rotation })} testId="layout-rotation" />
            <NumberField label="⧄" name={t('props.shear')} value={round(measured.shear ?? 0)} min={-89} max={89} step={1} unit="°" onChange={(shear, final) => final && set({ shear })} testId="layout-shear" />
          </>
        )}
      </div>
      <span className="layout-buttons layout-row-pad">
        <IconButton icon={linked ? Link : Unlink} label={t('props.constrain')} pressed={linked} onClick={() => setLinked(!linked)} data-testid="layout-constrain" />
        {content ? null : <IconButton icon={ArrowLeftRight} label={t('flipHorizontal')} onClick={() => void run('transform.flip', { axis: 'horizontal', ids })} data-testid="layout-flip-h" />}
        {content ? null : <IconButton icon={ArrowUpDown} label={t('flipVertical')} onClick={() => void run('transform.flip', { axis: 'vertical', ids })} data-testid="layout-flip-v" />}
      </span>
    </PropertyGroup>
  )
}
