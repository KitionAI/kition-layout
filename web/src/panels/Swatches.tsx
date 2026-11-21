import { useState } from 'react'
import { PlusIcon as Plus, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Button, Dialog, IconButton, NumberField, PanelFooter, PanelSection, Segmented, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { SquareIcon as Square, SquareDashedIcon as SquareDashed } from '@kition/tessera/icons/react'
import type { DocumentInfo, SwatchInfo } from '../layout'

type Target = 'fill' | 'stroke'

/**
 * InDesign's Swatches panel: every swatch with its colour; a click applies it to the selection's
 * fill or stroke (the toggle above the list), at the tint under it. New Swatch makes a colour from
 * a hex value; a swatch that is not locked can be deleted.
 */
export function SwatchesSection({ info }: { info: DocumentInfo }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [target, setTarget] = useState<Target>('fill')
  const [tint, setTint] = useState(100)
  const [chosen, setChosen] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const apply = (swatch: SwatchInfo) => {
    setChosen(swatch.name)
    if (readOnly || (!info.selection.length && !info.editingText)) return
    if (info.editingText) void run('type.char', { attrs: target === 'fill' ? { fill: swatch.name } : { stroke: swatch.name } })
    else if (target === 'fill') void run('object.fill', { swatch: swatch.name, tint: tint / 100, ids: info.selection })
    else void run('object.stroke', { swatch: swatch.name, tint: tint / 100, ids: info.selection })
  }
  const deletable = info.swatchColors.find((swatch) => swatch.name === chosen && !swatch.locked && swatch.kind !== 'none' && swatch.kind !== 'paper' && swatch.kind !== 'registration')
  return (
    <PanelSection title={p('Swatches')} defaultOpen={false} testId="layout-swatches">
      <div className="layout-bar">
        <Segmented
          label={t('swatchTarget')}
          value={target}
          segments={[
            { value: 'fill', icon: Square, label: p('Fill') },
            { value: 'stroke', icon: SquareDashed, label: p('Stroke') },
          ]}
          onChange={setTarget}
          testId="layout-swatch-target"
        />
        <NumberField label={t('tint')} value={tint} min={0} max={100} step={1} unit="%" width={64} onChange={(value) => setTint(value)} testId="layout-swatch-tint" />
      </div>
      {info.swatchColors.map((swatch) => (
        <button key={swatch.name} type="button" className="layout-list-item layout-swatch-row" data-active={swatch.name === chosen} onClick={() => apply(swatch)} data-testid={`layout-swatch-${swatch.name}`}>
          <span className="layout-chip" data-kind={swatch.kind} style={swatch.color ? { background: swatch.color } : undefined} aria-hidden="true" />
          <span className="layout-row-text">{swatch.name}</span>
        </button>
      ))}
      <PanelFooter
        end={
          <>
            <IconButton icon={Plus} label={t('newSwatch')} disabled={readOnly} onClick={() => setCreating(true)} data-testid="layout-new-swatch" />
            <IconButton icon={Trash} label={t('deleteSwatch')} disabled={readOnly || !deletable} onClick={() => deletable && void run('swatch.delete', { name: deletable.name }).then(() => setChosen(null))} data-testid="layout-delete-swatch" />
          </>
        }
      />
      {creating ? <NewSwatchDialog onClose={() => setCreating(false)} /> : null}
    </PanelSection>
  )
}

function NewSwatchDialog({ onClose }: { onClose: () => void }) {
  const { run, t } = useKitEditor()
  const [name, setName] = useState('')
  const [color, setColor] = useState('#5645d4')
  const valid = /^#[0-9a-f]{6}$/i.test(color)
  const create = async () => {
    if (!valid) return
    await run('swatch.create', name.trim() ? { name: name.trim(), color } : { color })
    onClose()
  }
  return (
    <Dialog
      title={t('newSwatch')}
      onClose={onClose}
      testId="layout-new-swatch-dialog"
      actions={
        <>
          <Button onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" disabled={!valid} onClick={() => void create()} data-testid="layout-new-swatch-ok">
            {t('ok')}
          </Button>
        </>
      }
    >
      <label className="layout-field">
        <span>{t('swatchName')}</span>
        <input className="kui-field" value={name} placeholder={t('swatchNameHint')} onChange={(event) => setName(event.target.value)} data-testid="layout-new-swatch-name" />
      </label>
      <label className="layout-field">
        <span>{t('swatchColor')}</span>
        <span className="layout-color">
          <input type="color" value={valid ? color : '#000000'} onChange={(event) => setColor(event.target.value)} aria-label={t('swatchColor')} />
          <input className="kui-field" value={color} onChange={(event) => setColor(event.target.value.trim())} data-testid="layout-new-swatch-color" />
        </span>
      </label>
    </Dialog>
  )
}
