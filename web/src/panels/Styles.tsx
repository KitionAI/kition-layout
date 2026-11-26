import { useState } from 'react'
import { CopyIcon as Copy, EraserIcon as Eraser, PencilIcon as Pencil, PlusIcon as Plus, RefreshCwIcon as Redefine, Settings2Icon as Settings2, TrashIcon as Trash } from '@kition/tessera/icons/react'
import { Choice, IconButton, PanelFooter, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { usePanelQuery } from './query'
import { PanelHeading } from './fields'
import type { DocumentInfo } from '../layout'
import { DialogFrame, useRunInDialog } from '../dialogs/DialogFrame'
import { StyleOptionsDialog, type NewStyle } from './StyleOptions'
import { BUILT_IN_STYLES, duplicateParams, replacementChoices, type StyleKind } from './styleRules'

type Selected = {
  paragraphStyle?: string
  characterStyle?: string
  para?: Record<string, unknown>
  chars?: Record<string, unknown>
  paraOverrides?: number
  charOverrides?: number
}

/**
 * InDesign's Paragraph and Character Styles panels: the styles, the one at the text selection
 * marked (with "+" when the text overrides it); a click applies, a double-click opens the style's
 * options. Under the list: Clear Overrides (paragraph styles), Redefine Style (the style takes the
 * selected text's look), New (Alt-click or the options button: its options first), Style Options,
 * Duplicate, and Delete (when text uses the style, it asks which style that text takes).
 */
export function StylesSection({ info, kind }: { info: DocumentInfo; kind: StyleKind }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const names = kind === 'paragraph' ? info.paragraphStyles : info.characterStyles
  const defs = kind === 'paragraph' ? info.styleDefs.paragraph : info.styleDefs.character
  const at = usePanelQuery<Selected>('text.selectionAttrs', {}, `${info.selection.join(',')}:${info.editingText}`)
  const current = kind === 'paragraph' ? at?.paragraphStyle : at?.characterStyle
  const overrides = (kind === 'paragraph' ? (at?.paraOverrides ?? 0) + (at?.charOverrides ?? 0) : (at?.charOverrides ?? 0)) > 0
  const [chosen, setChosen] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  // Alt+click on New opens the style options first (InDesign's); a plain click makes the style at once.
  const [making, setMaking] = useState(false)
  const [deleting, setDeleting] = useState<string | null>(null)
  const testId = kind === 'paragraph' ? 'layout-paragraph-style' : 'layout-character-style'
  const hasText = info.editingText || info.selection.length > 0
  const apply = (name: string) => {
    setChosen(name)
    if (!readOnly) void run(kind === 'paragraph' ? 'style.paragraph.apply' : 'style.character.apply', { name })
  }
  const freeName = () => {
    const base = kind === 'paragraph' ? t('newParagraphStyleName') : t('newCharacterStyleName')
    let name = base
    for (let n = 2; names.includes(name); n += 1) name = `${base} ${n}`
    return name
  }
  const create = async () => {
    const name = freeName()
    if (kind === 'paragraph') {
      await run('style.paragraph.add', { name, fromSelection: hasText })
    } else {
      const chars = at?.chars ?? {}
      const picked = Object.fromEntries(['fontFamily', 'fontStyle', 'size', 'fill', 'tracking'].filter((key) => chars[key] != null).map((key) => [key, chars[key]]))
      await run('style.character.add', { name, chars: picked })
    }
    setChosen(name)
  }
  const selected = chosen && names.includes(chosen) ? chosen : null
  const remove = (name: string) => {
    const uses = defs.find((def) => def.name === name)?.uses ?? 0
    // Text uses it: ask which style that text takes; else it goes at once.
    if (uses > 0) setDeleting(name)
    else void run(kind === 'paragraph' ? 'style.paragraph.delete' : 'style.character.delete', { name }).then(() => setChosen(null))
  }
  const duplicate = (name: string) => {
    const def = defs.find((style) => style.name === name)
    if (!def) return
    const params = duplicateParams(kind, def, names)
    void run(kind === 'paragraph' ? 'style.paragraph.add' : 'style.character.add', params).then((answer) => setChosen((JSON.parse(answer ?? 'null') as { name?: string } | null)?.name ?? (params.name as string)))
  }
  const redefinable = !readOnly && hasText && current != null && !BUILT_IN_STYLES.has(current) && overrides
  const make: NewStyle = { fromSelection: hasText, para: at?.para, chars: at?.chars }
  return (
    <div className="layout-panel" data-testid={`${testId}s`}>
      <PanelHeading title={p(kind === 'paragraph' ? 'Paragraph Styles' : 'Character Styles')} />
      {names.map((name) => (
        <button
          key={name}
          type="button"
          className="layout-list-item"
          data-active={name === selected}
          data-current={name === current}
          data-overrides={name === current && overrides ? 'true' : undefined}
          onClick={() => apply(name)}
          onDoubleClick={() => !readOnly && setEditing(name)}
          data-testid={`${testId}-${name}`}
        >
          {name}
          {name === current && overrides ? <span className="layout-override-mark" title={t('style.overridden')}>+</span> : null}
        </button>
      ))}
      <PanelFooter
        start={
          <>
            {kind === 'paragraph' ? (
              <IconButton
                icon={Eraser}
                label={t('style.clearOverrides')}
                disabled={readOnly || !current || !overrides}
                onClick={() => current && void run('style.paragraph.apply', { name: current, clearOverrides: true })}
                data-testid={`${testId}-clear-overrides`}
              />
            ) : null}
            <IconButton
              icon={Redefine}
              label={t('style.redefine')}
              disabled={!redefinable}
              onClick={() => current && void run(kind === 'paragraph' ? 'style.paragraph.edit' : 'style.character.edit', { name: current, fromSelection: true })}
              data-testid={`${testId}-redefine`}
            />
          </>
        }
        end={
          <>
            <IconButton
              icon={Plus}
              label={t(kind === 'paragraph' ? 'newParagraphStyleHint' : 'newCharacterStyleHint')}
              disabled={readOnly}
              onClick={(event) => (event.altKey ? setMaking(true) : void create())}
              data-testid={`${testId}-new`}
            />
            <IconButton icon={Settings2} label={t(kind === 'paragraph' ? 'newParagraphStyleOptions' : 'newCharacterStyleOptions')} disabled={readOnly} onClick={() => setMaking(true)} data-testid={`${testId}-new-options`} />
            <IconButton icon={Pencil} label={t('editStyle')} disabled={readOnly || !selected} onClick={() => selected && setEditing(selected)} data-testid={`${testId}-edit`} />
            <IconButton icon={Copy} label={t('style.duplicate')} disabled={readOnly || !selected || selected === '[No Paragraph Style]' || selected === '[None]'} onClick={() => selected && duplicate(selected)} data-testid={`${testId}-duplicate`} />
            <IconButton icon={Trash} label={t('deleteStyle')} disabled={readOnly || !selected || BUILT_IN_STYLES.has(selected)} onClick={() => selected && remove(selected)} data-testid={`${testId}-delete`} />
          </>
        }
      />
      {editing ? <StyleOptionsDialog info={info} kind={kind} name={editing} onClose={() => setEditing(null)} /> : null}
      {making ? <StyleOptionsDialog info={info} kind={kind} name={freeName()} make={make} onClose={() => setMaking(false)} onMade={setChosen} /> : null}
      {deleting ? (
        <DeleteStyleDialog
          kind={kind}
          name={deleting}
          names={names}
          onClose={(deleted) => {
            setDeleting(null)
            if (deleted) setChosen(null)
          }}
        />
      ) : null}
    </div>
  )
}

/** Delete Style when text uses it: which style that text takes instead. */
function DeleteStyleDialog({ kind, name, names, onClose }: { kind: StyleKind; name: string; names: readonly string[]; onClose: (deleted: boolean) => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const choices = replacementChoices(kind, names, name)
  const [replaceWith, setReplaceWith] = useState(choices[0] ?? '')
  return (
    <DialogFrame
      command={kind === 'paragraph' ? 'style.paragraph.delete' : 'style.character.delete'}
      title={t('style.deleteTitle', { name })}
      okLabel={t('style.deleteOk')}
      testId="layout-delete-style"
      onOk={async () => {
        await run(kind === 'paragraph' ? 'style.paragraph.delete' : 'style.character.delete', { name, replaceWith })
      }}
      onClose={onClose}
    >
      <p className="kui-muted">{t('style.deleteNote')}</p>
      <Choice label={t('style.replaceWith')} value={replaceWith} choices={choices.map((choice) => [choice, choice] as const)} onChange={setReplaceWith} testId="layout-delete-style-replace" />
    </DialogFrame>
  )
}
