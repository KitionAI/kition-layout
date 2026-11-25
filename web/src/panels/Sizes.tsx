import { useState } from 'react'
import { PlusIcon as Plus, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Button, Dialog, IconButton, NumberField, PanelFooter, PanelSection, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import type { DocumentInfo, LayoutInfo, Spread } from '../layout'
import { PRESETS, presetOf } from '../dialogs/DocumentSetup'

/** The screen sizes a document is usually made in again (the print sizes stay in Document Setup). */
const SIZE_PRESETS = PRESETS.filter((preset) => preset.unit === 'px')

const round = (value: number) => Math.round(value * 100) / 100

/**
 * Sizes: the document's alternate layouts (InDesign's Layout › Create Alternate Layout), each a
 * copy of the original at another size whose objects follow the liquid rules. A click shows the
 * size; New makes one from a preset or a custom size; Delete removes an alternate size.
 */
export function SizesSection({ info, onShow }: { info: DocumentInfo; onShow: (spread: Spread) => void }) {
  const { run, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [chosen, setChosen] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const spreadOf = (layout: LayoutInfo) => info.spreads.find((spread) => spread.pages.some((page) => page.index === layout.firstPage - 1))
  const selected = info.layouts.find((layout) => layout.name != null && layout.name === chosen) ?? null
  return (
    <PanelSection title={t('sizes')} actions={<IconButton icon={Plus} label={t('newSize')} disabled={readOnly} onClick={() => setAdding(true)} data-testid="layout-new-size" />} testId="layout-sizes">
      <div data-testid="layout-size-list" data-count={info.layouts.length}>
        {info.layouts.map((layout) => {
          const name = layout.name ?? t('originalSize')
          return (
            <button
              key={layout.name ?? ''}
              type="button"
              className="layout-list-item layout-size-row"
              data-active={layout.name != null && layout.name === chosen}
              onClick={() => {
                setChosen(layout.name)
                const spread = spreadOf(layout)
                if (spread) onShow(spread)
              }}
              data-testid={`layout-size-${layout.name ?? 'original'}`}
            >
              <span className="layout-row-text">{name}</span>
              <span className="kui-muted layout-count">{t('sizeOf', { width: round(layout.width), height: round(layout.height) })}</span>
            </button>
          )
        })}
      </div>
      <PanelFooter
        end={
          <IconButton
            icon={Trash}
            label={t('deleteSize')}
            disabled={readOnly || !selected}
            onClick={() => selected?.name && void run('layout.deleteAlternate', { name: selected.name }).then(() => setChosen(null))}
            data-testid="layout-delete-size"
          />
        }
      />
      {adding ? <NewSizeDialog info={info} onClose={() => setAdding(false)} onMade={setChosen} /> : null}
    </PanelSection>
  )
}

/** New size: a preset or a custom size (in pixels, which are points here) and the size's name. */
function NewSizeDialog({ info, onClose, onMade }: { info: DocumentInfo; onClose: () => void; onMade: (name: string) => void }) {
  const { run, t } = useKitEditor()
  const taken = new Set(info.layouts.map((layout) => layout.name).filter((name): name is string => name != null))
  const original = info.layouts[0]
  const first = SIZE_PRESETS.find((preset) => !original || presetOf(original.width, original.height) !== preset.id) ?? SIZE_PRESETS[0]
  const [width, setWidth] = useState<number>(first.width)
  const [height, setHeight] = useState<number>(first.height)
  const preset = presetOf(width, height)
  const suggested = (id: string) => {
    const base = id === 'custom' ? t('sizeOf', { width: round(width), height: round(height) }) : t(`preset.${id}`)
    let name = base
    for (let n = 2; taken.has(name); n += 1) name = `${base} ${n}`
    return name
  }
  const [name, setName] = useState<string | null>(null)
  const shown = name ?? suggested(preset)
  const valid = shown.trim() !== '' && !taken.has(shown.trim())
  const make = async () => {
    if (!valid) return
    await run('layout.addAlternate', { name: shown.trim(), width, height })
    onMade(shown.trim())
    onClose()
  }
  return (
    <Dialog
      title={t('newSize')}
      onClose={onClose}
      testId="layout-size-dialog"
      actions={
        <>
          <Button onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" disabled={!valid} onClick={() => void make()} data-testid="layout-size-ok">
            {t('ok')}
          </Button>
        </>
      }
    >
      <div className="layout-presets" role="radiogroup" aria-label={t('presets')}>
        {SIZE_PRESETS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="radio"
            aria-checked={preset === item.id}
            className="layout-preset"
            data-active={preset === item.id}
            onClick={() => {
              setWidth(item.width)
              setHeight(item.height)
            }}
            data-testid={`layout-size-preset-${item.id}`}
          >
            <span className="layout-preset-shape" style={{ aspectRatio: `${item.width} / ${item.height}` }} aria-hidden="true" />
            <span>{t(`preset.${item.id}`)}</span>
            <span className="kui-muted">{`${item.width} × ${item.height} px`}</span>
          </button>
        ))}
      </div>
      <div className="layout-grid">
        <NumberField label={t('width')} value={width} min={1} max={15552} step={1} unit="px" onChange={(value, final) => final && setWidth(value)} testId="layout-size-width" />
        <NumberField label={t('height')} value={height} min={1} max={15552} step={1} unit="px" onChange={(value, final) => final && setHeight(value)} testId="layout-size-height" />
      </div>
      <label className="layout-field">
        <span>{t('sizeName')}</span>
        <input className="kui-field" value={shown} onChange={(event) => setName(event.target.value)} data-testid="layout-size-name" />
      </label>
      {taken.has(shown.trim()) ? <p className="kui-muted layout-note">{t('sizeNameTaken')}</p> : <p className="kui-muted layout-note">{t('sizeNote')}</p>}
    </Dialog>
  )
}
