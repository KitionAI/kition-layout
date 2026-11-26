import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button, Dialog, useEngineQuery, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { useEditorView } from '../viewContext'
import { changeAllSteps, indexAfterRemoving, issueText, occurrenceKeys, shownWords, spellingMemory, type Issue, type Misspelling, type Preflight } from '../output'
import { useDialogPlace } from './DialogFrame'
import { TextRow } from './controls'

/** Where a dialog sends the person: an object (it is selected and its spread shown) or a page. */
export type GoTo = {
  /** Shows the object's spread and selects the object (`keep`: leaves the selection as it is, a word's text selection). */
  item: (id: number, keep?: boolean) => void
  /** Shows a page (counted from 0). */
  page: (page: number) => void
}

/**
 * Window › Output › Preflight (plan 2026-11-25, stage 8): the document's problems, live as it
 * changes (overset text, missing fonts, glyphs and links, low resolution, empty frames, RGB
 * swatches in print), each in the person's language with its page; a click on a row goes to the
 * object and selects it (or to its page). It does not block the page.
 */
export function PreflightDialog({ goTo, onClose }: { goTo: GoTo; onClose: () => void }) {
  const { t } = useKitEditor()
  const place = useDialogPlace()
  const result = useEngineQuery<Preflight>('preflight.run', {})
  const issues = result?.issues ?? []
  const open = (issue: Issue) => {
    if (issue.item != null) goTo.item(issue.item)
    else if (issue.page != null) goTo.page(issue.page)
  }
  return (
    <Dialog
      title={t('dialog.preflight')}
      modal={false}
      position={place.position}
      onMove={place.onMove}
      onClose={onClose}
      testId="layout-preflight"
      actions={
        <Button variant="primary" onClick={onClose} data-testid="layout-preflight-done">
          {t('find.done')}
        </Button>
      }
    >
      <div className="layout-dialog layout-output">
        <p className="layout-note" role="status" data-testid="layout-preflight-summary">
          {result == null ? t('preflight.checking') : issues.length ? t('preflight.summary', { errors: result.errors, warnings: result.warnings }) : t('preflight.none')}
        </p>
        <div className="layout-output-list" data-testid="layout-preflight-list">
          {issues.map((issue, index) => {
            const goes = issue.item != null || issue.page != null
            return (
              <button
                key={`${issue.kind}-${issue.item ?? ''}-${index}`}
                type="button"
                className="layout-output-row"
                data-severity={issue.severity}
                data-item={issue.item ?? ''}
                disabled={!goes}
                title={goes ? t('preflight.goTo') : undefined}
                onClick={() => open(issue)}
                data-testid={`layout-preflight-row-${index}`}
              >
                <span className="layout-badge" data-status={issue.severity === 'error' ? 'missing' : 'modified'}>
                  {t(`preflight.${issue.severity}`)}
                </span>
                <span className="layout-row-text">{issueText(issue, t)}</span>
                {issue.page != null ? <span className="kui-muted layout-count">{t('preflight.page', { page: issue.page + 1 })}</span> : null}
              </button>
            )
          })}
        </div>
      </div>
    </Dialog>
  )
}

/**
 * Edit › Spelling › Check Spelling… (plan stage 8): walks the words not in the dictionary, story
 * by story; each is selected in its frame and shown. Change replaces it (with a suggestion or
 * what the person types), Change All every one spelled so (one undo step), Skip passes this one,
 * Ignore All every one spelled so, Add puts it in the document's user dictionary. What Skip and
 * Ignore All set aside stays aside while the document is open, the dialog closed and opened again
 * too (plan 2026-11-25, phase 10). It does not block the page.
 */
export function SpellingDialog({ goTo, onClose }: { goTo: GoTo; onClose: () => void }) {
  const { t, session, run } = useKitEditor()
  const { settle } = useEditorView()
  const readOnly = useReadOnly()
  const place = useDialogPlace()
  const [words, setWords] = useState<Misspelling[] | null>(null)
  const [index, setIndex] = useState(0)
  const memory = session.doc ? spellingMemory(session.doc) : null
  const [ignored, setIgnored] = useState<ReadonlySet<string>>(() => new Set(memory?.ignored))
  const [skipped, setSkipped] = useState<ReadonlySet<string>>(() => new Set(memory?.skipped))
  const [to, setTo] = useState('')
  const [busy, setBusy] = useState(false)
  const shown = useMemo(() => (words ? shownWords(words, ignored, skipped) : []), [words, ignored, skipped])
  const current = shown[index] ?? null

  const check = useCallback(async () => {
    const open = session.doc
    if (!open) return
    try {
      setWords(JSON.parse(await open.execute('spelling.check', {})) as Misspelling[])
    } catch {
      setWords([])
    }
  }, [session.doc])
  useEffect(() => {
    void check()
  }, [check])

  // Each word, as it comes up: selected in its frame and shown, with its first suggestion.
  const latest = useRef({ goTo, run })
  latest.current = { goTo, run }
  const key = current ? `${current.story}:${current.start}:${current.word}` : ''
  useEffect(() => {
    if (!current) return
    setTo(current.suggestions[0] ?? current.word)
    void latest.current.run('text.select', { story: current.story, anchor: current.start, focus: current.end })
    if (current.frame != null) latest.current.goTo.item(current.frame, true)
    // The word's place is what changes; its object stays the same.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])

  const act = async (work: () => Promise<void>) => {
    setBusy(true)
    try {
      await work()
    } finally {
      setBusy(false)
    }
  }
  const change = () =>
    act(async () => {
      if (!current) return
      // The words after it move with the text: the check runs again, and the next one is at this place.
      await run('spelling.change', { story: current.story, start: current.start, end: current.end, to })
      await check()
    })
  const changeAll = () =>
    act(async () => {
      const open = session.doc
      if (!current || !open || !words) return
      const steps = changeAllSteps(words, current.word, to)
      await open.invoke('runSteps', t('spelling.changeAllStep'), JSON.stringify(steps))
      await settle()
      setIndex(indexAfterRemoving(shown, index, current.word))
      await check()
    })
  const ignore = () => {
    if (!current) return
    setIndex(indexAfterRemoving(shown, index, current.word))
    memory?.ignored.add(current.word.toLowerCase())
    setIgnored((now) => new Set([...now, current.word.toLowerCase()]))
  }
  // This occurrence set aside: the next word takes its place in the list.
  const skip = () => {
    if (!current || !words) return
    const key = occurrenceKeys(words)[words.indexOf(current)]
    if (key === undefined) return
    memory?.skipped.add(key)
    setSkipped((now) => new Set([...now, key]))
  }
  const add = () =>
    act(async () => {
      if (!current) return
      await run('spelling.addWord', { word: current.word })
      setIndex(indexAfterRemoving(shown, index, current.word))
      await check()
    })
  const ready = current != null && !busy
  return (
    <Dialog
      title={t('dialog.spelling')}
      modal={false}
      position={place.position}
      onMove={place.onMove}
      onClose={onClose}
      testId="layout-spelling"
      actions={
        <>
          <Button onClick={onClose} data-testid="layout-spelling-close">
            {t('find.done')}
          </Button>
          <Button disabled={!ready || readOnly} onClick={() => void add()} data-testid="layout-spelling-add">
            {t('spelling.add')}
          </Button>
          <Button disabled={!ready} onClick={ignore} data-testid="layout-spelling-ignore">
            {t('spelling.ignoreAll')}
          </Button>
          <Button disabled={!ready} onClick={skip} data-testid="layout-spelling-skip">
            {t('spelling.skip')}
          </Button>
          <Button disabled={!ready || readOnly || !to} onClick={() => void changeAll()} data-testid="layout-spelling-change-all">
            {t('find.changeAll')}
          </Button>
          <Button variant="primary" disabled={!ready || readOnly || !to} onClick={() => void change()} data-testid="layout-spelling-change">
            {t('spelling.change')}
          </Button>
        </>
      }
    >
      <div className="layout-dialog layout-dialog-fields layout-output">
        <p className="layout-note" role="status" data-testid="layout-spelling-status">
          {words == null ? t('spelling.checking') : current ? t('spelling.notInDictionary') : t('spelling.done')}
        </p>
        {current ? (
          <>
            <p className="layout-spelling-word" data-testid="layout-spelling-word">
              {current.word}
            </p>
            <TextRow label={t('spelling.changeTo')} value={to} onChange={setTo} testId="layout-spelling-to" />
            <div className="layout-output-list" data-testid="layout-spelling-suggestions">
              {current.suggestions.length ? (
                current.suggestions.map((suggestion, at) => (
                  <button key={suggestion} type="button" className="layout-output-row" data-selected={suggestion === to} onClick={() => setTo(suggestion)} data-testid={`layout-spelling-suggestion-${at}`}>
                    <span className="layout-row-text">{suggestion}</span>
                  </button>
                ))
              ) : (
                <p className="layout-empty">{t('spelling.noSuggestions')}</p>
              )}
            </div>
          </>
        ) : null}
      </div>
    </Dialog>
  )
}
