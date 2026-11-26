import { useMemo, useState } from 'react'
import { EraserIcon as Eraser, PencilIcon as Pencil, PlusIcon as Plus, RefreshCwIcon as Redefine, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Checkbox, Choice, IconButton, PanelFooter, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { findItem, type DocumentInfo } from '../layout'
import { DialogFrame, useRunInDialog } from '../dialogs/DialogFrame'
import { useCommandPreview } from '../dialogs/preview'
import { PanelHeading } from './fields'
import { BUILT_IN_OBJECT_STYLES, VERTICAL_JUSTIFICATIONS, freeObjectStyleName, objectBasedOnChoices, objectStyleDraft, objectStyleEdit, objectStyleReplacements, type ObjectStyleDraft } from './objectStyleRules'

/**
 * InDesign's Object Styles panel (plan 2026-11-25, phase 11): the object styles, the one of the
 * selected object marked (with "+" when the object overrides it); a click applies the style to the
 * selection, a double-click opens its options. Under the list: Clear Overrides (the object takes
 * the style's parts again), Redefine Style (the style takes the selected object's look), New (from
 * the selected object), Style Options, and Delete (when objects use the style, it asks which style
 * they take).
 */
export function ObjectStylesSection({ info }: { info: DocumentInfo }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const defs = info.styleDefs.object ?? []
  const first = info.selection.length ? findItem(info, info.selection[0])?.item : undefined
  const current = first?.objectStyle ?? null
  const overrides = Boolean(first?.styleOverride)
  const [chosen, setChosen] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [deleting, setDeleting] = useState<string | null>(null)
  const selected = chosen && defs.some((def) => def.name === chosen) ? chosen : null
  const apply = (name: string) => {
    setChosen(name)
    if (!readOnly && info.selection.length) void run('style.object.apply', { name, ids: info.selection })
  }
  const create = async () => {
    const name = freeObjectStyleName(defs, t('objectStyle.newName'))
    const answer = await run('style.object.add', { name, fromSelection: info.selection.length > 0 })
    if (answer == null) return
    const made = (JSON.parse(answer) as { name?: string } | null)?.name ?? name
    setChosen(made)
    setEditing(made)
  }
  const remove = (name: string) => {
    if ((defs.find((def) => def.name === name)?.uses ?? 0) > 0) setDeleting(name)
    else void run('style.object.delete', { name }).then(() => setChosen(null))
  }
  return (
    <div className="layout-panel" data-testid="layout-object-styles">
      <PanelHeading title={p('Object Styles')} />
      {defs.map(({ name }) => (
        <button
          key={name}
          type="button"
          className="layout-list-item"
          data-active={name === selected}
          data-current={name === current}
          data-overrides={name === current && overrides ? 'true' : undefined}
          onClick={() => apply(name)}
          onDoubleClick={() => !readOnly && name !== '[None]' && setEditing(name)}
          data-testid={`layout-object-style-${name}`}
        >
          {name === '[None]' ? p('[None]') : name}
          {name === current && overrides ? <span className="layout-override-mark" title={t('style.overridden')}>+</span> : null}
        </button>
      ))}
      <PanelFooter
        start={
          <>
            <IconButton
              icon={Eraser}
              label={t('style.clearOverrides')}
              disabled={readOnly || !current || !overrides}
              onClick={() => current && void run('style.object.apply', { name: current, ids: info.selection })}
              data-testid="layout-object-style-clear-overrides"
            />
            <IconButton
              icon={Redefine}
              label={t('style.redefine')}
              disabled={readOnly || !current || current === '[None]' || !overrides}
              onClick={() => current && void run('style.object.edit', { name: current, fromSelection: true })}
              data-testid="layout-object-style-redefine"
            />
          </>
        }
        end={
          <>
            <IconButton icon={Plus} label={t('objectStyle.new')} disabled={readOnly} onClick={() => void create()} data-testid="layout-object-style-new" />
            <IconButton icon={Pencil} label={t('editStyle')} disabled={readOnly || !selected || selected === '[None]'} onClick={() => selected && setEditing(selected)} data-testid="layout-object-style-edit" />
            <IconButton icon={Trash} label={t('deleteStyle')} disabled={readOnly || !selected || BUILT_IN_OBJECT_STYLES.has(selected)} onClick={() => selected && remove(selected)} data-testid="layout-object-style-delete" />
          </>
        }
      />
      {editing ? <ObjectStyleOptionsDialog info={info} name={editing} onClose={(renamed) => { setEditing(null); if (renamed) setChosen(renamed) }} /> : null}
      {deleting ? (
        <DeleteObjectStyleDialog
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
 * Object Style Options (phase 11): General (name, Based On), Fill, Stroke, Paragraph Style and
 * Text Frame Options, each with its "include in the style" box. It opens with the style's own
 * values; the objects using the style change as the values do (the engine's preview of
 * `style.object.edit`), Cancel takes it back, OK keeps it as one undo step (a rename at OK).
 */
function ObjectStyleOptionsDialog({ info, name, onClose }: { info: DocumentInfo; name: string; onClose: (renamed: string | null) => void }) {
  const { t, phrase: p } = useKitEditor()
  const run = useRunInDialog()
  const defs = info.styleDefs.object ?? []
  const opened = useMemo(() => objectStyleDraft(defs.find((def) => def.name === name), name), [])
  const [draft, setDraft] = useState<ObjectStyleDraft>(opened)
  const builtIn = BUILT_IN_OBJECT_STYLES.has(name)
  const params = objectStyleEdit(name, opened, draft)
  const preview = useCommandPreview('style.object.edit', params)
  const swatches = info.swatchColors.filter((swatch) => swatch.kind !== 'registration').map((swatch) => [swatch.name, swatch.name === '[None]' ? p('[None]') : swatch.name] as const)
  const based = [['', p('[None]')] as const, ...objectBasedOnChoices(defs, name).map((other) => [other, other] as const)]
  const paragraphs = info.paragraphStyles.filter((style) => style !== '[No Paragraph Style]').map((style) => [style, style] as const)
  const field = (label: string, value: string, onChange: (value: string) => void, testId: string, unit: string | undefined, disabled: boolean) => (
    <label className="layout-field">
      <span>
        {label}
        {unit ? <span className="kui-muted"> ({unit})</span> : null}
      </span>
      <input className="kui-field" inputMode="decimal" value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)} data-testid={testId} />
    </label>
  )
  return (
    <DialogFrame
      command="style.object.edit"
      title={t('objectStyle.optionsTitle', { name })}
      testId="layout-object-style-options"
      preview={preview}
      okDisabled={objectStyleEdit(name, opened, draft, true) === null && JSON.stringify(draft) !== JSON.stringify(opened)}
      onOk={async () => {
        const all = objectStyleEdit(name, opened, draft, !builtIn)
        if (!all) return
        if (preview && !all.rename && (await preview.keep(all))) return
        await run('style.object.edit', all)
      }}
      onClose={(committed) => {
        const rename = draft.name.trim()
        onClose(committed && !builtIn && rename && rename !== name ? rename : null)
      }}
    >
      <div className="layout-stack" data-testid="layout-object-style-dialog">
        <section className="layout-stack">
          <h3 className="layout-options-heading">{t('style.section.general')}</h3>
          <label className="layout-field">
            <span>{t('styleName')}</span>
            <input className="kui-field" value={draft.name} disabled={builtIn} onChange={(event) => setDraft({ ...draft, name: event.target.value })} data-testid="layout-object-style-name" />
          </label>
          <Choice label={t('style.basedOn')} value={draft.basedOn} choices={based} onChange={(basedOn) => setDraft({ ...draft, basedOn })} testId="layout-object-style-based-on" />
        </section>
        <section className="layout-stack">
          <Checkbox label={t('objectStyle.fill')} checked={draft.fill.on} onChange={(on) => setDraft({ ...draft, fill: { ...draft.fill, on } })} testId="layout-object-style-fill-on" />
          <div className="layout-grid">
            <Choice label={t('objectStyle.swatch')} value={draft.fill.swatch} choices={swatches} disabled={!draft.fill.on} onChange={(swatch) => setDraft({ ...draft, fill: { ...draft.fill, swatch } })} testId="layout-object-style-fill" />
            {field(t('tint'), draft.fill.tint, (tint) => setDraft({ ...draft, fill: { ...draft.fill, tint } }), 'layout-object-style-fill-tint', '%', !draft.fill.on)}
          </div>
        </section>
        <section className="layout-stack">
          <Checkbox label={t('objectStyle.stroke')} checked={draft.stroke.on} onChange={(on) => setDraft({ ...draft, stroke: { ...draft.stroke, on } })} testId="layout-object-style-stroke-on" />
          <div className="layout-grid">
            <Choice label={t('objectStyle.swatch')} value={draft.stroke.swatch} choices={swatches} disabled={!draft.stroke.on} onChange={(swatch) => setDraft({ ...draft, stroke: { ...draft.stroke, swatch } })} testId="layout-object-style-stroke" />
            {field(t('objectStyle.weight'), draft.stroke.weight, (weight) => setDraft({ ...draft, stroke: { ...draft.stroke, weight } }), 'layout-object-style-stroke-weight', 'pt', !draft.stroke.on)}
            {field(t('tint'), draft.stroke.tint, (tint) => setDraft({ ...draft, stroke: { ...draft.stroke, tint } }), 'layout-object-style-stroke-tint', '%', !draft.stroke.on)}
          </div>
        </section>
        <section className="layout-stack">
          <Checkbox label={t('objectStyle.paragraph')} checked={draft.paragraph.on} onChange={(on) => setDraft({ ...draft, paragraph: { ...draft.paragraph, on } })} testId="layout-object-style-paragraph-on" />
          <Choice label={p('Paragraph Styles')} value={draft.paragraph.style} choices={paragraphs} disabled={!draft.paragraph.on} onChange={(style) => setDraft({ ...draft, paragraph: { ...draft.paragraph, style } })} testId="layout-object-style-paragraph" />
        </section>
        <section className="layout-stack">
          <Checkbox label={t('objectStyle.frame')} checked={draft.frame.on} onChange={(on) => setDraft({ ...draft, frame: { ...draft.frame, on } })} testId="layout-object-style-frame-on" />
          <div className="layout-grid">
            {field(t('objectStyle.columns'), draft.frame.columns, (columns) => setDraft({ ...draft, frame: { ...draft.frame, columns } }), 'layout-object-style-columns', undefined, !draft.frame.on)}
            {field(t('objectStyle.gutter'), draft.frame.gutter, (gutter) => setDraft({ ...draft, frame: { ...draft.frame, gutter } }), 'layout-object-style-gutter', 'pt', !draft.frame.on)}
            {(['top', 'left', 'bottom', 'right'] as const).map((side, index) =>
              field(
                t(`objectStyle.inset.${side}`),
                draft.frame.inset[index],
                (value) => {
                  const inset = [...draft.frame.inset] as ObjectStyleDraft['frame']['inset']
                  inset[index] = value
                  setDraft({ ...draft, frame: { ...draft.frame, inset } })
                },
                `layout-object-style-inset-${side}`,
                'pt',
                !draft.frame.on,
              ),
            )}
            <Choice
              label={t('objectStyle.vj')}
              value={draft.frame.vj}
              choices={VERTICAL_JUSTIFICATIONS.map((vj) => [vj, t(`objectStyle.vj.${vj}`)] as const)}
              disabled={!draft.frame.on}
              onChange={(vj) => setDraft({ ...draft, frame: { ...draft.frame, vj } })}
              testId="layout-object-style-vj"
            />
          </div>
        </section>
        <p className="kui-muted">{t('objectStyle.usesNote', { count: defs.find((def) => def.name === name)?.uses ?? 0 })}</p>
      </div>
    </DialogFrame>
  )
}

/** Delete Object Style when objects use it: which style they take (they keep their look). */
function DeleteObjectStyleDialog({ info, name, onClose }: { info: DocumentInfo; name: string; onClose: (deleted: boolean) => void }) {
  const { t, phrase: p } = useKitEditor()
  const run = useRunInDialog()
  const defs = info.styleDefs.object ?? []
  const choices = objectStyleReplacements(defs, name)
  const [replaceWith, setReplaceWith] = useState(choices[0] ?? '[None]')
  return (
    <DialogFrame
      command="style.object.delete"
      title={t('style.deleteTitle', { name })}
      okLabel={t('style.deleteOk')}
      testId="layout-delete-object-style"
      onOk={() => run('style.object.delete', { name, replaceWith })}
      onClose={onClose}
    >
      <p className="kui-muted">{t('objectStyle.deleteNote', { count: defs.find((def) => def.name === name)?.uses ?? 0 })}</p>
      <Choice label={t('style.replaceWith')} value={replaceWith} choices={choices.map((choice) => [choice, choice === '[None]' ? p('[None]') : choice] as const)} onChange={setReplaceWith} testId="layout-delete-object-style-replace" />
    </DialogFrame>
  )
}
