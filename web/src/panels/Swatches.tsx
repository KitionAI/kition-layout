import { useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { BlendIcon as Blend, PlusIcon as Plus, Settings2Icon as Settings2, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Button, Choice, Dialog, IconButton, NumberField, PanelFooter, Segmented, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { SquareIcon as Square, SquareDashedIcon as SquareDashed } from '@kition/tessera/icons/react'
import type { DocumentInfo, SwatchInfo } from '../layout'
import { DialogFrame, useRunInDialog } from '../dialogs/DialogFrame'
import { useCommandPreview } from '../dialogs/preview'
import { ColorEditor } from './ColorWell'
import { colorParam, hexToRgb, pickedRgb, rgbToHex, swatchColor, type PickedColor } from './colors'
import { swatchEdit, swatchReplacements, type SwatchDraft } from './swatchRules'
import { addStop, cssGradient, draftCss, gradientDraft, gradientEdit, gradientParam, moveStop, newGradientDraft, removeStop, type GradientDraft } from './gradientRules'

type Target = 'fill' | 'stroke'

/** Swatches that cannot be changed or deleted (InDesign's in brackets), and the ones that have no colour to change. */
const editable = (swatch: SwatchInfo) => swatch.kind === 'color' || swatch.kind === 'tint' || swatch.kind === 'paper' || swatch.kind === 'gradient'

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
  // A gradient swatch's dialog (phase 11): a new one (''), or the one being edited.
  const [gradient, setGradient] = useState<string | null>(null)
  const openOptions = (swatch: SwatchInfo) => (swatch.kind === 'gradient' ? setGradient(swatch.name) : setOptions(swatch.name))
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
          onDoubleClick={() => !readOnly && editable(swatch) && (!swatch.locked || swatch.kind === 'paper') && openOptions(swatch)}
          data-testid={`layout-swatch-${swatch.name}`}
        >
          <span
            className="layout-chip"
            data-kind={swatch.kind}
            style={swatch.kind === 'gradient' ? { background: cssGradient(info.colors[swatch.name]?.gradient, { asOnPage: true }) ?? swatch.color ?? undefined } : swatch.color ? { background: swatch.color } : undefined}
            aria-hidden="true"
          />
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
            <IconButton icon={Settings2} label={t('swatch.options')} disabled={readOnly || !optionable} onClick={() => optionable && openOptions(optionable)} data-testid="layout-swatch-options-button" />
            <IconButton icon={Plus} label={t('newSwatch')} disabled={readOnly} onClick={() => setCreating(true)} data-testid="layout-new-swatch" />
            <IconButton icon={Blend} label={t('gradient.new')} disabled={readOnly} onClick={() => setGradient('')} data-testid="layout-new-gradient" />
            <IconButton icon={Trash} label={t('deleteSwatch')} disabled={readOnly || !deletable} onClick={() => deletable && remove(deletable.name)} data-testid="layout-delete-swatch" />
          </>
        }
      />
      {creating ? <NewSwatchDialog onClose={() => setCreating(false)} /> : null}
      {gradient != null ? (
        <GradientSwatchDialog
          info={info}
          name={gradient || null}
          onClose={(made) => {
            setGradient(null)
            if (made) setChosen(made)
          }}
        />
      ) : null}
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

/**
 * New Gradient Swatch and a gradient swatch's options (plan 2026-11-25, phase 11): its name, linear
 * or radial, and the stops on a bar under the gradient (a click on the bar adds a stop with the
 * colour there; a stop is dragged along it, or off it to remove it). The chosen stop's colour comes
 * from a colour swatch or the colour picker, with its location and the midpoint to the next stop.
 * Editing one, everything using it changes as the values do (the engine's preview of
 * `swatch.set`); Cancel takes it back, OK keeps it as one undo step.
 */
function GradientSwatchDialog({ info, name, onClose }: { info: DocumentInfo; name: string | null; onClose: (made: string | null) => void }) {
  const { t, phrase: p } = useKitEditor()
  const run = useRunInDialog()
  const opened = useMemo<GradientDraft>(() => (name ? gradientDraft(name, info.colors[name]?.gradient) : newGradientDraft('')), [])
  const [draft, setDraft] = useState<GradientDraft>(opened)
  const [chosen, setChosen] = useState(0)
  const params = name ? gradientEdit(name, opened, draft) : null
  const preview = useCommandPreview('swatch.set', params)
  const stop = draft.stops[chosen] ?? draft.stops[0]
  const colorSwatches = info.swatchColors.filter((swatch) => ['color', 'tint', 'paper'].includes(swatch.kind) && info.colors[swatch.name])
  const setStop = (patch: Partial<GradientDraft['stops'][number]>) => setDraft({ ...draft, stops: draft.stops.map((one, index) => (index === chosen ? { ...one, ...patch } : one)) })
  return (
    <DialogFrame
      command={name ? 'swatch.set' : 'swatch.add'}
      title={name ? t('gradient.optionsTitle', { name }) : t('gradient.new')}
      testId="layout-gradient-dialog"
      preview={name ? preview : undefined}
      onOk={async () => {
        if (!name) {
          const given = draft.name.trim()
          const answer = await run('swatch.add', given ? { name: given, gradient: gradientParam(draft) } : { gradient: gradientParam(draft) })
          onClose((JSON.parse(answer) as { name?: string } | null)?.name ?? null)
          return
        }
        const all = gradientEdit(name, opened, draft, true)
        if (!all) return
        if (preview && !all.rename && (await preview.keep(all))) return
        await run('swatch.set', all)
      }}
      onClose={(committed) => {
        if (name) onClose(committed ? draft.name.trim() || name : null)
        else if (!committed) onClose(null)
      }}
    >
      <label className="layout-field">
        <span>{t('swatchName')}</span>
        <input className="kui-field" value={draft.name} placeholder={t('gradient.namePlaceholder')} onChange={(event) => setDraft({ ...draft, name: event.target.value })} data-testid="layout-gradient-name" />
      </label>
      <Choice label={t('gradient.kind')} value={draft.kind} choices={[['linear', t('gradient.linear')] as const, ['radial', t('gradient.radial')] as const]} onChange={(kind) => setDraft({ ...draft, kind })} testId="layout-gradient-kind" />
      <GradientStopBar draft={draft} chosen={chosen} onChange={(stops, index) => { setDraft({ ...draft, stops }); setChosen(index) }} onChoose={setChosen} />
      <p className="kui-muted">{t('gradient.barNote')}</p>
      {stop ? (
        <div className="layout-stack" data-testid="layout-gradient-stop">
          <Choice
            label={t('gradient.stopColor')}
            value=""
            choices={[['', t('gradient.fromSwatch')] as const, ...colorSwatches.map((swatch) => [swatch.name, swatch.name === '[Paper]' || swatch.name === '[Black]' ? p(swatch.name) : swatch.name] as const)]}
            onChange={(swatchName) => {
              const color = swatchColor(info.colors[swatchName])
              if (color) setStop({ color })
            }}
            testId="layout-gradient-stop-swatch"
          />
          <ColorEditor value={stop.color} onChange={(color) => setStop({ color })} testId="layout-gradient-stop-color" />
          <div className="layout-grid">
            <label className="layout-field">
              <span>
                {t('gradient.location')} <span className="kui-muted">(%)</span>
              </span>
              <input
                className="kui-field"
                inputMode="decimal"
                value={stop.location}
                onChange={(event) => {
                  const value = Number(event.target.value)
                  if (Number.isFinite(value)) {
                    const moved = moveStop(draft.stops, chosen, value)
                    setDraft({ ...draft, stops: moved.stops })
                    setChosen(moved.index)
                  }
                }}
                data-testid="layout-gradient-location"
              />
            </label>
            <label className="layout-field">
              <span>
                {t('gradient.midpoint')} <span className="kui-muted">(%)</span>
              </span>
              <input
                className="kui-field"
                inputMode="decimal"
                value={stop.midpoint}
                disabled={chosen >= draft.stops.length - 1}
                onChange={(event) => {
                  const value = Number(event.target.value)
                  if (Number.isFinite(value)) setStop({ midpoint: Math.max(13, Math.min(87, value)) })
                }}
                data-testid="layout-gradient-midpoint"
              />
            </label>
          </div>
        </div>
      ) : null}
      {name ? <p className="kui-muted">{t('swatch.usesNote', { count: info.colors[name]?.uses ?? 0 })}</p> : null}
    </DialogFrame>
  )
}

/** How far below or above the bar a dragged stop goes off it (and is removed when let go). */
const OFF_BAR = 28

/**
 * The stop bar: the gradient, with a marker under it for each stop (the chosen one marked). A
 * click on the bar adds a stop there; a marker dragged along the bar moves its stop, dragged off it
 * removes it (two stops always stay).
 */
function GradientStopBar({ draft, chosen, onChange, onChoose }: { draft: GradientDraft; chosen: number; onChange: (stops: GradientDraft['stops'], index: number) => void; onChoose: (index: number) => void }) {
  const { t } = useKitEditor()
  const bar = useRef<HTMLDivElement | null>(null)
  const [off, setOff] = useState<number | null>(null)
  const at = (clientX: number) => {
    const box = bar.current?.getBoundingClientRect()
    return box && box.width > 0 ? ((clientX - box.left) / box.width) * 100 : 0
  }
  const add = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return
    const next = addStop(draft.stops, at(event.clientX))
    onChange(next.stops, next.index)
  }
  const grab = (index: number) => (event: ReactPointerEvent<HTMLButtonElement>) => {
    event.preventDefault()
    event.stopPropagation()
    onChoose(index)
    let stops = draft.stops
    let now = index
    let gone = false
    const box = bar.current?.getBoundingClientRect()
    const move = (moved: PointerEvent) => {
      gone = box ? moved.clientY > box.bottom + OFF_BAR || moved.clientY < box.top - OFF_BAR : false
      setOff(gone && stops.length > 2 ? now : null)
      if (gone) return
      const next = moveStop(stops, now, at(moved.clientX))
      stops = next.stops
      now = next.index
      onChange(stops, now)
    }
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      setOff(null)
      if (gone && stops.length > 2) onChange(removeStop(stops, now), Math.max(0, now - 1))
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }
  return (
    <div className="layout-gradient-editor">
      <div ref={bar} className="layout-gradient-bar" style={{ background: draftCss(draft) }} onPointerDown={add} title={t('gradient.barNote')} data-testid="layout-gradient-bar" />
      <div className="layout-gradient-stops">
        {draft.stops.map((stop, index) => (
          <button
            key={index}
            type="button"
            className="layout-gradient-marker"
            style={{ left: `${stop.location}%`, background: rgbToHex(pickedRgb(stop.color)) }}
            data-active={index === chosen ? 'true' : undefined}
            data-off={off === index ? 'true' : undefined}
            aria-label={t('gradient.stopAt', { location: stop.location })}
            onPointerDown={grab(index)}
            data-testid={`layout-gradient-marker-${index}`}
          />
        ))}
      </div>
    </div>
  )
}
