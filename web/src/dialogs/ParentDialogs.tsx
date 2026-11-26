import { useMemo, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { basedOnChoices, pagesUsing, parentDraft, parentEdit, parentReplacements, prefixProblem, type ParentDraft } from '../panels/parentRules'
import { ChoiceRow, NumberRow, TextRow } from './controls'
import { DialogFrame, useRunInDialog } from './DialogFrame'
import { useCommandPreview } from './preview'

/**
 * Parent Page Options (plan 2026-11-25, phase 11): the parent's prefix, name, the parent it is
 * based on (whose objects then show under its own) and its number of pages. The pages using it
 * change as the values do (the engine's preview of `layout.parents.set`); Cancel takes it back, OK
 * keeps it as one undo step.
 */
export function ParentOptionsDialog({ info, parent, onClose }: { info: DocumentInfo; parent: string; onClose: () => void }) {
  const { t, phrase: p } = useKitEditor()
  const run = useRunInDialog()
  // The parents as the dialog opened: the preview renames and rebases them as the person types.
  const opened = useMemo(() => {
    const entry = info.parents.find((one) => one.label === parent || one.prefix === parent)
    return { entry, parents: info.parents, draft: entry ? parentDraft(entry) : null }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const [draft, setDraft] = useState<ParentDraft | null>(opened.draft)
  const label = opened.entry?.label ?? parent
  const params = draft && opened.draft ? parentEdit(label, opened.draft, draft, opened.parents) : null
  const preview = useCommandPreview('layout.parents.set', params)
  const problem = draft ? prefixProblem(draft.prefix, opened.parents, label) : ''
  const choices = basedOnChoices(opened.parents, label)
  return (
    <DialogFrame
      command="layout.parents.set"
      title={t('parent.optionsTitle', { name: label })}
      testId="layout-parent-options"
      preview={preview}
      okDisabled={!draft || Boolean(problem) || !draft.name.trim()}
      onOk={async () => {
        if (!params) return
        if (preview && (await preview.keep(params))) return
        await run('layout.parents.set', params)
      }}
      onClose={onClose}
    >
      {draft ? (
        <div className="layout-dialog-fields">
          <TextRow label={t('parent.prefix')} value={draft.prefix} onChange={(prefix) => setDraft({ ...draft, prefix })} testId="layout-parent-prefix" />
          {problem ? <div className="layout-dialog-warning">{t(`parent.prefixProblem.${problem}`)}</div> : null}
          <TextRow label={t('parent.name')} value={draft.name} onChange={(name) => setDraft({ ...draft, name })} testId="layout-parent-name" />
          <ChoiceRow
            label={t('parent.basedOn')}
            value={draft.basedOn}
            choices={[['', p('[None]')] as const, ...choices.map((choice) => [choice, choice] as const)]}
            onChange={(basedOn) => setDraft({ ...draft, basedOn })}
            testId="layout-parent-based-on"
          />
          <NumberRow label={t('parent.pages')} value={draft.pages} min={1} max={10} decimals={0} onChange={(pages) => pages !== null && setDraft({ ...draft, pages: Math.round(pages) })} testId="layout-parent-pages" />
          {opened.draft && draft.pages < opened.draft.pages ? <p className="layout-note">{t('parent.fewerPages')}</p> : null}
        </div>
      ) : (
        <p className="layout-note">{t('parent.gone')}</p>
      )}
    </DialogFrame>
  )
}

/**
 * Delete Parent Spread (phase 11): its objects go; the pages using it, and the parents based on
 * it, take the parent chosen here ([None] at first).
 */
export function DeleteParentDialog({ info, parent, onClose }: { info: DocumentInfo; parent: string; onClose: () => void }) {
  const { t, phrase: p } = useKitEditor()
  const run = useRunInDialog()
  const entry = info.parents.find((one) => one.label === parent || one.prefix === parent)
  const label = entry?.label ?? parent
  const choices = parentReplacements(info.parents, label)
  const [replaceWith, setReplaceWith] = useState('')
  const pages = info.editingParents ? null : pagesUsing(info.spreads, label)
  return (
    <DialogFrame
      command="layout.parents.delete"
      title={t('parent.deleteTitle', { name: label })}
      okLabel={t('style.deleteOk')}
      testId="layout-delete-parent"
      onOk={() => run('layout.parents.delete', { parent: label, replaceWith: replaceWith || null })}
      onClose={onClose}
    >
      <div className="layout-dialog-fields">
        <p className="layout-note">{pages == null ? t('parent.deleteNoteParents') : t('parent.deleteNote', { count: pages, items: entry?.items ?? 0 })}</p>
        <ChoiceRow label={t('parent.replaceWith')} value={replaceWith} choices={choices.map((choice) => [choice, choice || p('[None]')] as const)} onChange={setReplaceWith} testId="layout-delete-parent-replace" />
      </div>
    </DialogFrame>
  )
}
