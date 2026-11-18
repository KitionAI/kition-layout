import { useState } from 'react'
import { PencilIcon as Pencil, PlusIcon as Plus, TrashIcon as Trash } from '@kition/tessera-icons/react'
import { Button, Choice, Dialog, IconButton, PanelFooter, PanelSection, useEngineQuery, useKitEditor, useReadOnly } from '@kition/kit-editor'
import type { DocumentInfo } from '../layout'

type Kind = 'paragraph' | 'character'
type Selected = { paragraphStyle?: string; characterStyle?: string; chars?: Record<string, unknown> }

/** Styles the engine makes and keeps: they can be applied and edited but not deleted. */
const BUILT_IN = new Set(['[No Paragraph Style]', '[Basic Paragraph]', '[None]'])

/**
 * InDesign's Paragraph and Character Styles panels: the styles (the one at the text selection
 * marked), a click applies, New makes one from the selected text's look, Edit opens the style's
 * font, size, leading, tracking, colour (and, for paragraphs, alignment), Delete removes the
 * style (its text takes [Basic Paragraph] or [None]).
 */
export function StylesSection({ info, kind }: { info: DocumentInfo; kind: Kind }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const names = kind === 'paragraph' ? info.paragraphStyles : info.characterStyles
  const at = useEngineQuery<Selected>('type.selectionAttrs', {}, `${info.selection.join(',')}:${info.editingText}`)
  const current = kind === 'paragraph' ? at?.paragraphStyle : at?.characterStyle
  const [chosen, setChosen] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const testId = kind === 'paragraph' ? 'layout-paragraph-style' : 'layout-character-style'
  const apply = (name: string) => {
    setChosen(name)
    if (!readOnly) void run(kind === 'paragraph' ? 'style.paragraph.apply' : 'style.character.apply', { name })
  }
  const create = async () => {
    const base = kind === 'paragraph' ? t('newParagraphStyleName') : t('newCharacterStyleName')
    let name = base
    for (let n = 2; names.includes(name); n += 1) name = `${base} ${n}`
    if (kind === 'paragraph') {
      await run('style.paragraph.create', { name, fromSelection: info.editingText || info.selection.length > 0 })
    } else {
      const chars = at?.chars ?? {}
      const picked = Object.fromEntries(['fontFamily', 'fontStyle', 'size', 'fill', 'tracking'].filter((key) => chars[key] != null).map((key) => [key, chars[key]]))
      await run('style.character.create', { name, chars: picked })
    }
    setChosen(name)
  }
  const selected = chosen && names.includes(chosen) ? chosen : null
  return (
    <PanelSection title={p(kind === 'paragraph' ? 'Paragraph Styles' : 'Character Styles')} defaultOpen={false} testId={`${testId}s`}>
      {names.map((name) => (
        <button key={name} type="button" className="layout-list-item" data-active={name === selected} data-current={name === current} onClick={() => apply(name)} onDoubleClick={() => setEditing(name)} data-testid={`${testId}-${name}`}>
          {name}
        </button>
      ))}
      <PanelFooter
        end={
          <>
            <IconButton icon={Plus} label={t(kind === 'paragraph' ? 'newParagraphStyle' : 'newCharacterStyle')} disabled={readOnly} onClick={() => void create()} data-testid={`${testId}-new`} />
            <IconButton icon={Pencil} label={t('editStyle')} disabled={readOnly || !selected} onClick={() => selected && setEditing(selected)} data-testid={`${testId}-edit`} />
            <IconButton
              icon={Trash}
              label={t('deleteStyle')}
              disabled={!selected || BUILT_IN.has(selected)}
              onClick={() => selected && void run(kind === 'paragraph' ? 'style.paragraph.delete' : 'style.character.delete', { name: selected }).then(() => setChosen(null))}
              data-testid={`${testId}-delete`}
            />
          </>
        }
      />
      {editing ? <StyleDialog info={info} kind={kind} name={editing} onClose={() => setEditing(null)} /> : null}
    </PanelSection>
  )
}

const ALIGN_CHOICES = ['', 'left', 'center', 'right', 'justify'] as const

/** A style's text settings; empty fields stay as the style has them. */
function StyleDialog({ info, kind, name, onClose }: { info: DocumentInfo; kind: Kind; name: string; onClose: () => void }) {
  const { run, t, phrase: p, session } = useKitEditor()
  const families = useEngineQuery<string[]>('font.families', {}, session.doc ? 'families' : null) ?? []
  const [rename, setRename] = useState(name)
  const [family, setFamily] = useState('')
  const [size, setSize] = useState('')
  const [leading, setLeading] = useState('')
  const [tracking, setTracking] = useState('')
  const [fill, setFill] = useState('')
  const [align, setAlign] = useState<(typeof ALIGN_CHOICES)[number]>('')
  const number = (text: string) => (text.trim() === '' || !Number.isFinite(Number(text)) ? undefined : Number(text))
  const save = async () => {
    const chars: Record<string, unknown> = {}
    if (family) chars.fontFamily = family
    if (number(size) != null) chars.size = number(size)
    if (number(leading) != null) chars.leading = { kind: 'points', value: number(leading) }
    if (number(tracking) != null) chars.tracking = number(tracking)
    if (fill) chars.fill = fill
    const params: Record<string, unknown> = { name, chars }
    if (rename.trim() && rename.trim() !== name && !BUILT_IN.has(name)) params.rename = rename.trim()
    if (kind === 'paragraph' && align) params.para = { align }
    await run(kind === 'paragraph' ? 'style.paragraph.edit' : 'style.character.edit', params)
    onClose()
  }
  const field = (label: string, value: string, set: (value: string) => void, testId: string) => (
    <label className="layout-field">
      <span>{label}</span>
      <input className="kui-field" inputMode="decimal" value={value} placeholder={t('unchanged')} onChange={(event) => set(event.target.value)} data-testid={testId} />
    </label>
  )
  return (
    <Dialog
      title={t('styleOptions', { name })}
      onClose={onClose}
      testId="layout-style-dialog"
      actions={
        <>
          <Button onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" onClick={() => void save()} data-testid="layout-style-ok">
            {t('ok')}
          </Button>
        </>
      }
    >
      <label className="layout-field">
        <span>{t('styleName')}</span>
        <input className="kui-field" value={rename} disabled={BUILT_IN.has(name)} onChange={(event) => setRename(event.target.value)} data-testid="layout-style-name" />
      </label>
      <Choice label={p('Font')} value={family} choices={[['', t('unchanged')] as const, ...families.map((name) => [name, name] as const)]} onChange={setFamily} testId="layout-style-font" />
      {field(t('fontSize'), size, setSize, 'layout-style-size')}
      {field(t('leading'), leading, setLeading, 'layout-style-leading')}
      {field(t('tracking'), tracking, setTracking, 'layout-style-tracking')}
      <Choice label={p('Fill')} value={fill} choices={[['', t('unchanged')] as const, ...info.swatches.map((name) => [name, name] as const)]} onChange={setFill} testId="layout-style-fill" />
      {kind === 'paragraph' ? (
        <Choice label={t('alignment')} value={align} choices={ALIGN_CHOICES.map((value) => [value, value ? t(`align.${value}`) : t('unchanged')] as const)} onChange={setAlign} testId="layout-style-align" />
      ) : null}
    </Dialog>
  )
}
