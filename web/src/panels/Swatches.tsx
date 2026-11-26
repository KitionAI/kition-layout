import { useMemo, useState } from 'react'
import { PlusIcon as Plus, Settings2Icon as Settings2, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Button, Choice, Dialog, IconButton, NumberField, PanelFooter, Segmented, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { SquareIcon as Square, SquareDashedIcon as SquareDashed } from '@kition/tessera/icons/react'
import type { DocumentInfo, SwatchInfo } from '../layout'
import { DialogFrame, useRunInDialog } from '../dialogs/DialogFrame'
import { useCommandPreview } from '../dialogs/preview'
import { ColorEditor } from './ColorWell'
import { colorParam, hexToRgb, pickedRgb, rgbToHex, swatchColor, type PickedColor } from './colors'
import { swatchEdit, swatchReplacements, type SwatchDraft } from './swatchRules'

type Target = 'fill' | 'stroke'

/** Swatches that cannot be changed or deleted (InDesign's in brackets), and the ones that have no colour to change. */
const editable = (swatch: SwatchInfo) => swatch.kind === 'color' || swatch.kind === 'tint' || swatch.kind === 'paper'

/**
 * InDesign's Swatches panel: every swatch with its colour (spot inks marked); a click applies it
 * to the selection's fill or stroke (the toggle above the list), at the tint under it; a
 * double-click (or the options button) opens Swatch Options. New Swatch makes a colour (RGB or
 * CMYK, process or spot); Delete asks which swatch takes the place of one that is in use.
 */
export function SwatchesSection({ info }: { info: DocumentInfo }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [target, setTarget] = useState<Target>('fill')
  const [tint, setTint] = useState(100)
  const [chosen, setChosen] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [options, setOptions] = useState<string | null>(null)
  const [deleting, setDeleting] = useState<string | null>(null)
  const apply = (swatch: SwatchInfo) => {
    setChosen(swatch.name)
    if (readOnly || (!info.selection.length && !info.editingText)) return
    if (info.editingText) void run('text.char', { attrs: target === 'fill' ? { fill: swatch.name } : { stroke: swatch.name } })
    else if (target === 'fill') void run('object.fill', { swatch: swatch.name, tint: tint / 100, ids: info.selection })
    else void run('object.stroke', { swatch: swatch.name, tint: tint / 100, ids: info.selection })
  }
  const deletable = info.swatchColors.find((swatch) => swatch.name === chosen && !swatch.locked && swatch.kind !== 'none' && swatch.kind !== 'paper' && swatch.kind !== 'registration')
  const optionable = info.swatchColors.find((swatch) => swatch.name === chosen && editable(swatch) && (!swatch.locked || swatch.kind === 'paper'))
  const remove = (name: string) => {
    // In use: ask which swatch takes its place; else it goes at once.
    if ((info.colors[name]?.uses ?? 0) > 0) setDeleting(name)
    else void run('swatch.delete', { name }).then(() => setChosen(null))
  }
  return (
    <div className="layout-panel" data-testid="layout-swatches">
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
        <button
          key={swatch.name}
          type="button"
          className="layout-list-item layout-swatch-row"
          data-active={swatch.name === chosen}
          onClick={() => apply(swatch)}
          onDoubleClick={() => !readOnly && editable(swatch) && (!swatch.locked || swatch.kind === 'paper') && setOptions(swatch.name)}
          data-testid={`layout-swatch-${swatch.name}`}
        >
          <span className="layout-chip" data-kind={swatch.kind} style={swatch.color ? { background: swatch.color } : undefined} aria-hidden="true" />
          <span className="layout-row-text">{swatch.name}</span>
          {info.colors[swatch.name]?.spot ? (
            <span className="layout-swatch-mark kui-muted" title={t('swatch.spot')}>
              ◉
            </span>
          ) : null}
          {info.colors[swatch.name]?.cmyk ? <span className="layout-swatch-mark kui-muted">CMYK</span> : swatch.kind === 'color' ? <span className="layout-swatch-mark kui-muted">RGB</span> : null}
        </button>
      ))}
      <PanelFooter
        end={
          <>
            <IconButton icon={Settings2} label={t('swatch.options')} disabled={readOnly || !optionable} onClick={() => optionable && setOptions(optionable.name)} data-testid="layout-swatch-options-button" />
            <IconButton icon={Plus} label={t('newSwatch')} disabled={readOnly} onClick={() => setCreating(true)} data-testid="layout-new-swatch" />
            <IconButton icon={Trash} label={t('deleteSwatch')} disabled={readOnly || !deletable} onClick={() => deletable && remove(deletable.name)} data-testid="layout-delete-swatch" />
          </>
        }
      />
      {creating ? <NewSwatchDialog onClose={() => setCreating(false)} /> : null}
      {options ? (
        <SwatchOptionsDialog
          info={info}
          name={options}
          onClose={(renamed) => {
            setOptions(null)
            if (renamed) setChosen(renamed)
          }}
        />
      ) : null}
      {deleting ? (
        <DeleteSwatchDialog
          info={info}
          name={deleting}
          onClose={(deleted) => {
            setDeleting(null)
            if (deleted) setChosen(null)
          }}
        />
      ) : null}
    </div>
  )
}

/**
 * New Swatch: a name and a colour from the colour picker (RGB, HSB and hex, or CMYK parts, kept
 * as CMYK), or typed as hex; process or spot.
 */
function NewSwatchDialog({ onClose }: { onClose: () => void }) {
  const { run, t } = useKitEditor()
  const [name, setName] = useState('')
  const [color, setColor] = useState<PickedColor>({ model: 'rgb', rgb: [0x56, 0x45, 0xd4] })
  const [hex, setHex] = useState('#5645d4')
  const [spot, setSpot] = useState(false)
  const valid = /^#[0-9a-f]{6}$/i.test(hex)
  const create = async () => {
    if (!valid) return
    const params: Record<string, unknown> = name.trim() ? { name: name.trim(), color: colorParam(color) } : { color: colorParam(color) }
    if (spot) params.spot = true
    await run('swatch.add', params)
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
      <Choice label={t('swatch.type')} value={spot ? 'spot' : 'process'} choices={[['process', t('swatch.process')] as const, ['spot', t('swatch.spot')] as const]} onChange={(next) => setSpot(next === 'spot')} testId="layout-new-swatch-type" />
      <ColorEditor
        value={color}
        onChange={(next) => {
          setColor(next)
          setHex(rgbToHex(pickedRgb(next)))
        }}
        testId="layout-new-swatch-picker"
      />
      <label className="layout-field">
        <span>{t('swatchColor')}</span>
        <input
          className="kui-field"
          value={hex}
          onChange={(event) => {
            const text = event.target.value.trim()
            setHex(text)
            const rgb = hexToRgb(text)
            if (rgb && /^#[0-9a-f]{6}$/i.test(text)) setColor({ model: 'rgb', rgb })
          }}
          data-testid="layout-new-swatch-color"
        />
      </label>
    </Dialog>
  )
}

/**
 * Swatch Options (plan 2026-11-25, stage 7): the swatch's name, process or spot, and its colour
 * (RGB or CMYK); a tint swatch's tint. Every object and text using it changes as the values do
 * (the engine's preview of `swatch.set`); Cancel takes it back, OK keeps it as one undo step.
 */
function SwatchOptionsDialog({ info, name, onClose }: { info: DocumentInfo; name: string; onClose: (renamed: string | null) => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const swatch = info.swatchColors.find((entry) => entry.name === name)
  const opened = useMemo<SwatchDraft>(() => {
    const value = info.colors[name]
    return { name, spot: Boolean(value?.spot), color: swatchColor(value), tint: value?.tint != null ? Math.round(value.tint * 1000) / 10 : null }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const [draft, setDraft] = useState<SwatchDraft>(opened)
  const params = swatchEdit(name, opened, draft)
  const preview = useCommandPreview('swatch.set', params)
  const tint = swatch?.kind === 'tint'
  const locked = Boolean(swatch?.locked)
  return (
    <DialogFrame
      command="swatch.set"
      title={t('swatch.optionsTitle', { name })}
      testId="layout-swatch-options"
      preview={preview}
      onOk={async () => {
        if (!params) return
        if (preview && (await preview.keep(params))) return
        await run('swatch.set', params)
      }}
      onClose={(committed) => onClose(committed ? (typeof params?.rename === 'string' ? params.rename : name) : null)}
    >
      <label className="layout-field">
        <span>{t('swatchName')}</span>
        <input className="kui-field" value={draft.name} disabled={locked} onChange={(event) => setDraft({ ...draft, name: event.target.value })} data-testid="layout-swatch-options-name" />
      </label>
      {tint ? (
        <label className="layout-field">
          <span>
            {t('tint')} <span className="kui-muted">(%)</span>
          </span>
          <input className="kui-field" inputMode="decimal" value={draft.tint ?? ''} onChange={(event) => setDraft({ ...draft, tint: event.target.value === '' ? null : Number(event.target.value) })} data-testid="layout-swatch-options-tint" />
        </label>
      ) : draft.color ? (
        <>
          {swatch?.kind === 'color' ? (
            <Choice label={t('swatch.type')} value={draft.spot ? 'spot' : 'process'} choices={[['process', t('swatch.process')] as const, ['spot', t('swatch.spot')] as const]} onChange={(next) => setDraft({ ...draft, spot: next === 'spot' })} testId="layout-swatch-options-type" />
          ) : null}
          <ColorEditor value={draft.color} onChange={(color) => setDraft({ ...draft, color })} testId="layout-swatch-options-color" />
        </>
      ) : null}
      <p className="kui-muted">{t('swatch.usesNote', { count: info.colors[name]?.uses ?? 0 })}</p>
    </DialogFrame>
  )
}

/** Delete Swatch when it is in use: which swatch takes its place. */
function DeleteSwatchDialog({ info, name, onClose }: { info: DocumentInfo; name: string; onClose: (deleted: boolean) => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const choices = swatchReplacements(info.swatchColors, name)
  const [replaceWith, setReplaceWith] = useState(choices[0] ?? '[Black]')
  return (
    <DialogFrame
      command="swatch.delete"
      title={t('swatch.deleteTitle', { name })}
      okLabel={t('style.deleteOk')}
      testId="layout-delete-swatch-dialog"
      onOk={async () => {
        await run('swatch.delete', { name, replaceWith })
      }}
      onClose={onClose}
    >
      <p className="kui-muted">{t('swatch.deleteNote', { count: info.colors[name]?.uses ?? 0 })}</p>
      <Choice label={t('style.replaceWith')} value={replaceWith} choices={choices.map((choice) => [choice, choice] as const)} onChange={setReplaceWith} testId="layout-delete-swatch-replace" />
    </DialogFrame>
  )
}
