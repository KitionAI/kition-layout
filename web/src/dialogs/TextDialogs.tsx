import { useState } from 'react'
import { Button, Dialog, useKitEditor } from '@kition/sdk/editor'
import { refusalReason } from '../menus/context'
import { DialogFrame, useDialogPlace, useRunInDialog } from './DialogFrame'
import { CheckRow, ChoiceRow, NumberRow, TextRow } from './controls'

/**
 * The text dialogs (plan 2026-11-25, stage 1): Insert Table, Convert Text to Table's caret, Find/
 * Change, and the question before the sample document replaces an edited one.
 */

/** Table › Insert Table…: body rows, columns, header and footer rows, at the text insertion point; the caret is then in the first cell. */
export function InsertTableDialog({ onClose }: { onClose: () => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const [rows, setRows] = useState(4)
  const [cols, setCols] = useState(4)
  const [headerRows, setHeaderRows] = useState(0)
  const [footerRows, setFooterRows] = useState(0)
  const count = (label: string, value: number, set: (value: number) => void, min: number, testId: string) => (
    <NumberRow label={label} value={value} min={min} max={1000} decimals={0} onChange={(next) => next !== null && next >= min && set(Math.round(next))} testId={testId} />
  )
  return (
    <DialogFrame command="table.insert" title={t('dialog.insertTable')} okDisabled={!(rows >= 1 && cols >= 1)} onOk={() => run('table.insert', { rows, cols, headerRows, footerRows })} onClose={onClose}>
      <div className="layout-dialog-fields">
        {count(t('param.rows'), rows, setRows, 1, 'layout-table-rows')}
        {count(t('param.cols'), cols, setCols, 1, 'layout-table-cols')}
        {count(t('param.headerRows'), headerRows, setHeaderRows, 0, 'layout-table-header')}
        {count(t('param.footerRows'), footerRows, setFooterRows, 0, 'layout-table-footer')}
      </div>
    </DialogFrame>
  )
}

/** What Find/Change searches with. */
type Search = { find: string; change: string; scope: 'document' | 'story' | 'selection'; caseSensitive: boolean; wholeWord: boolean; grep: boolean }

/** The last search, for the session: Find Next runs it again, and the dialog opens with it. */
let lastSearch: Search = { find: '', change: '', scope: 'document', caseSensitive: false, wholeWord: false, grep: false }

/** The search's values as the engine's find commands take them. */
const findParams = (search: Search) => ({ find: search.find, scope: search.scope, caseSensitive: search.caseSensitive, wholeWord: search.wholeWord, grep: search.grep })

/** Edit › Find Next: the last search again when there is one, else null (the dialog opens). */
export function lastFind() {
  return lastSearch.find ? findParams(lastSearch) : null
}

/**
 * Edit › Find/Change…: the text to find and to change it to, where (the document, the story or
 * the selection), case, whole word and GREP; Find Next selects the next match, Change replaces the
 * match selected, Change/Find does both, Change All replaces every match. It does not block the
 * page: the person edits the layout with it open.
 */
export function FindChangeDialog({ onClose }: { onClose: () => void }) {
  const { t, session, report } = useKitEditor()
  const [search, setSearch] = useState<Search>(lastSearch)
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const place = useDialogPlace()
  const set = (next: Partial<Search>) => {
    lastSearch = { ...search, ...next }
    setSearch(lastSearch)
  }
  const act = async (work: () => Promise<string>) => {
    setBusy(true)
    try {
      setStatus(await work())
    } catch (error) {
      const reason = refusalReason(error)
      if (reason) setStatus(t(`hint.${reason}`))
      else report(t('commandFailed'), error)
    } finally {
      setBusy(false)
    }
  }
  const next = async () => {
    const answer = await session.run('edit.findNext', findParams(search))
    return answer && answer !== 'null' ? t('find.found') : t('find.none')
  }
  const change = async () => {
    // Change replaces the match Find Next selected (the text selection).
    await session.run('text.insert', { text: search.change, raw: true })
    return t('find.changed', { count: 1 })
  }
  const all = async () => {
    const answer = await session.run('edit.replace', { ...findParams(search), change: search.change })
    const count = answer ? Number((JSON.parse(answer) as { count?: number }).count ?? 0) : 0
    return count ? t('find.changed', { count }) : t('find.none')
  }
  const ready = search.find !== '' && !busy
  return (
    <Dialog
      title={t('dialog.findChange')}
      modal={false}
      position={place.position}
      onMove={place.onMove}
      onClose={onClose}
      testId="layout-find-dialog"
      actions={
        <>
          <Button onClick={onClose} data-testid="layout-find-done">
            {t('find.done')}
          </Button>
          <Button disabled={!ready} onClick={() => void act(all)} data-testid="layout-find-change-all">
            {t('find.changeAll')}
          </Button>
          <Button disabled={!ready} onClick={() => void act(async () => `${await change()} ${await next()}`)} data-testid="layout-find-change-find">
            {t('find.changeFind')}
          </Button>
          <Button disabled={!ready} onClick={() => void act(change)} data-testid="layout-find-change">
            {t('find.changeOne')}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={() => void act(next)} data-testid="layout-find-next">
            {t('find.findNext')}
          </Button>
        </>
      }
    >
      <div className="layout-dialog layout-dialog-fields">
        <TextRow label={t('find.what')} value={search.find} onChange={(find) => set({ find })} testId="layout-find-text" autoFocus />
        <TextRow label={t('find.changeTo')} value={search.change} onChange={(change) => set({ change })} testId="layout-find-change-text" />
        <ChoiceRow label={t('find.scope')} value={search.scope} choices={(['document', 'story', 'selection'] as const).map((scope) => [scope, t(`find.scope.${scope}`)] as const)} onChange={(scope) => set({ scope })} testId="layout-find-scope" />
        <CheckRow label={t('param.caseSensitive')} checked={search.caseSensitive} onChange={(caseSensitive) => set({ caseSensitive })} testId="layout-find-case" />
        <CheckRow label={t('param.wholeWord')} checked={search.wholeWord} onChange={(wholeWord) => set({ wholeWord })} testId="layout-find-whole-word" />
        <CheckRow label={t('param.grep')} checked={search.grep} onChange={(grep) => set({ grep })} testId="layout-find-grep" />
        <p className="layout-note" role="status" data-testid="layout-find-status">
          {status ?? ''}
        </p>
      </div>
    </Dialog>
  )
}

/** A question before something that cannot be taken back (Help › Sample Document over unsaved changes). */
export function ConfirmDialog({ title, message, confirm, onConfirm, onClose, testId }: { title: string; message: string; confirm: string; onConfirm: () => void; onClose: () => void; testId: string }) {
  const { t } = useKitEditor()
  return (
    <Dialog
      title={title}
      onClose={onClose}
      testId={testId}
      actions={
        <>
          <Button onClick={onClose} data-testid={`${testId}-cancel`}>
            {t('cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              onClose()
              onConfirm()
            }}
            data-testid={`${testId}-ok`}
          >
            {confirm}
          </Button>
        </>
      }
    >
      <p className="layout-note">{message}</p>
    </Dialog>
  )
}
