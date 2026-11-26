import { useState } from 'react'
import { KitEngine, toast, useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { engineManifest } from '../engine'
import { UNIT_NAMES, toPoints, type Unit } from '../units'
import { useEditorView } from '../viewContext'
import { useCommandPreview } from './preview'
import { DialogFrame, useRunInDialog } from './DialogFrame'
import { CheckRow, ChoiceRow, LengthRow, NumberRow, SidesRows, TextRow, type Sides } from './controls'
import { useErrorText } from '../errorWords'
import { addPrimaryTextFrames, insertPlan, movePlan, pagesFromText, type Where } from './pages'

/**
 * The page dialogs (plan 2026-11-25, stage 1): New Document, Margins and Columns, Insert Pages,
 * Move Pages, Delete Pages and Go to Page. Each runs its commands on OK; page numbers are counted
 * from 1, as the Pages panel names them.
 */

const WHERE: readonly Where[] = ['after', 'before', 'start', 'end']

/** The words for four sides of a page (inside/outside with facing pages, left/right without). */
function sideLabels(t: (key: string) => string, facing: boolean): Record<keyof Sides, string> {
  return { top: t('side.top'), bottom: t('side.bottom'), inside: t(facing ? 'side.inside' : 'side.left'), outside: t(facing ? 'side.outside' : 'side.right') }
}

const unitChoices = (t: (key: string) => string) => UNIT_NAMES.map((unit) => [unit, t(`unit.${unit}`)] as const)

/** Document sizes New Document offers, in points (portrait). */
export const PAGE_PRESETS = {
  a4: { width: 595.276, height: 841.89, unit: 'mm' },
  a5: { width: 419.528, height: 595.276, unit: 'mm' },
  a3: { width: 841.89, height: 1190.551, unit: 'mm' },
  b5: { width: 498.898, height: 708.661, unit: 'mm' },
  letter: { width: 612, height: 792, unit: 'in' },
  legal: { width: 612, height: 1008, unit: 'in' },
  tabloid: { width: 792, height: 1224, unit: 'in' },
} as const satisfies Record<string, { width: number; height: number; unit: Unit }>
type PagePreset = keyof typeof PAGE_PRESETS | 'custom'

/** A file name without characters a file system refuses, never empty, ending in `.klayout`. */
export function documentFileName(name: string, fallback: string) {
  const clean = name.replace(/[\\/:*?"<>|]+/g, ' ').trim() || fallback
  return /\.klayout$/i.test(clean) ? clean : `${clean}.klayout`
}

/**
 * File › New… (New Document): a name, a size preset or a size in a unit, orientation, pages,
 * facing pages, columns and gutter, four margins (linked or each its own) and bleed. The editor
 * shows the one document its host opened, so OK makes the new layout beside it through the host
 * (`host/write`, as an export) and says where; the person opens it from the workspace.
 */
export function NewDocumentDialog({ onClose }: { onClose: () => void }) {
  const { t, host } = useKitEditor()
  const { setView } = useEditorView()
  const words = useErrorText()
  const [primary, setPrimary] = useState(false)
  const [name, setName] = useState(() => t('dialog.untitled'))
  const [preset, setPreset] = useState<PagePreset>('a4')
  const [unit, setUnit] = useState<Unit>('mm')
  const [width, setWidth] = useState<number>(PAGE_PRESETS.a4.width)
  const [height, setHeight] = useState<number>(PAGE_PRESETS.a4.height)
  const [pages, setPages] = useState(1)
  const [facing, setFacing] = useState(true)
  const [columns, setColumns] = useState(1)
  const [gutter, setGutter] = useState(toPoints(5, 'mm'))
  const [margins, setMargins] = useState<Sides>(() => {
    const side = toPoints(12.7, 'mm')
    return { top: side, bottom: side, inside: side, outside: side }
  })
  const [linked, setLinked] = useState(true)
  const [bleed, setBleed] = useState(toPoints(3, 'mm'))
  const landscape = width > height
  const valid = width > 0 && height > 0 && width <= 15552 && height <= 15552 && pages >= 1 && pages <= 9999 && columns >= 1
  const choose = (next: PagePreset) => {
    setPreset(next)
    if (next === 'custom') return
    const size = PAGE_PRESETS[next]
    setWidth(landscape ? size.height : size.width)
    setHeight(landscape ? size.width : size.height)
    setUnit(size.unit)
  }
  const create = async () => {
    const file = documentFileName(name, t('dialog.untitled'))
    const engine = await KitEngine.create(engineManifest(), file, width, height)
    try {
      await engine.execute('layout.documentSetup', { width, height, pages, facingPages: facing, bleed })
      await engine.execute('layout.marginsAndColumns', { margins, columns, gutter })
      // A-Parent takes them too (phase 9: it kept the engine's half inch and one column): Margins
      // and Columns sets the parents' pages while the parents are edited.
      await engine.execute('layout.parents.edit', { on: true })
      await engine.execute('layout.marginsAndColumns', { margins, columns, gutter })
      await engine.execute('layout.parents.edit', { on: false })
      if (primary) await addPrimaryTextFrames((command, params) => engine.execute(command, params), columns, gutter)
      const path = await host.exportFile(file, await engine.save())
      // The rulers and fields go on in the unit the layout was made in (they fell back to points).
      setView((view) => ({ ...view, unit }))
      toast('success', t('dialog.created', { path }))
    } catch (error) {
      toast('error', t('dialog.createFailed'), words(error))
      throw error
    } finally {
      engine.dispose()
    }
  }
  const size = (label: string, value: number, set: (points: number) => void, testId: string) => (
    <LengthRow
      label={label}
      value={value}
      unit={unit}
      onChange={(next) => {
        if (next === null || !(next > 0)) return
        setPreset('custom')
        set(next)
      }}
      testId={testId}
    />
  )
  return (
    <DialogFrame command="kit:file.new" title={t('dialog.newDocument')} okLabel={t('dialog.create')} okDisabled={!valid} onOk={create} onClose={onClose}>
      <div className="layout-dialog-fields">
        <TextRow label={t('dialog.name')} value={name} onChange={setName} testId="layout-new-name" />
        <ChoiceRow label={t('presets')} value={preset} choices={[...(Object.keys(PAGE_PRESETS) as (keyof typeof PAGE_PRESETS)[]).map((key) => [key, t(`page.${key}`)] as const), ['custom', t('page.custom')] as const]} onChange={choose} testId="layout-new-preset" />
        <ChoiceRow label={t('units')} value={unit} choices={unitChoices(t)} onChange={setUnit} testId="layout-new-unit" />
        {size(t('width'), width, setWidth, 'layout-new-width')}
        {size(t('height'), height, setHeight, 'layout-new-height')}
        <ChoiceRow
          label={t('dialog.orientation')}
          value={landscape ? 'landscape' : 'portrait'}
          choices={[['portrait', t('dialog.portrait')], ['landscape', t('dialog.landscape')]] as const}
          onChange={(next) => {
            if ((next === 'landscape') === landscape) return
            setWidth(height)
            setHeight(width)
          }}
          testId="layout-new-orientation"
        />
        <NumberRow label={t('pages')} value={pages} min={1} max={9999} decimals={0} onChange={(next) => next !== null && next >= 1 && setPages(Math.round(next))} testId="layout-new-pages" />
        <CheckRow label={t('facingPages')} checked={facing} onChange={setFacing} testId="layout-new-facing" />
        <NumberRow label={t('columns')} value={columns} min={1} max={216} decimals={0} onChange={(next) => next !== null && next >= 1 && setColumns(Math.round(next))} testId="layout-new-columns" />
        <LengthRow label={t('gutter')} value={gutter} unit={unit} onChange={(next) => next !== null && next >= 0 && setGutter(next)} testId="layout-new-gutter" />
        <SidesRows label={t('margins')} sides={margins} linked={linked} labels={sideLabels(t, facing)} linkLabel={t('dialog.linkSides')} unit={unit} onChange={setMargins} onLink={setLinked} testId="layout-new-margins" />
        <LengthRow label={t('bleed')} value={bleed} unit={unit} onChange={(next) => next !== null && next >= 0 && setBleed(next)} testId="layout-new-bleed" />
        <CheckRow label={t('dialog.primaryTextFrame')} checked={primary} onChange={setPrimary} testId="layout-new-primary" />
        {primary ? <p className="kui-muted layout-note">{t('dialog.primaryTextFrameNote')}</p> : null}
        {valid ? null : <div className="layout-dialog-warning">{t('dialog.badSize')}</div>}
        <p className="kui-muted layout-note">{t('dialog.newDocumentNote')}</p>
      </div>
    </DialogFrame>
  )
}

/** The page in view's margins and columns, as the dialogs start from them. */
function pageOf(info: DocumentInfo, index: number) {
  return info.spreads.flatMap((spread) => spread.pages).find((page) => page.index === index) ?? info.spreads[0]?.pages[0] ?? null
}

/**
 * Layout › Margins and Columns…: four margins (linked or each its own), columns and gutter, for
 * the page in view or every page, in a unit.
 */
export function MarginsColumnsDialog({ info, page, onClose }: { info: DocumentInfo; page: number; onClose: () => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const shown = pageOf(info, page)
  const start = shown?.margins ?? { top: 36, bottom: 36, inside: 36, outside: 36 }
  const [margins, setMargins] = useState<Sides>({ top: start.top, bottom: start.bottom, inside: start.inside, outside: start.outside })
  const [linked, setLinked] = useState(start.top === start.bottom && start.top === start.inside && start.top === start.outside)
  const [columns, setColumns] = useState(shown?.columns ?? 1)
  const [gutter, setGutter] = useState(12)
  const [unit, setUnit] = useState<Unit>(useEditorView().view.unit)
  const [scope, setScope] = useState<'page' | 'all'>('all')
  const params = { margins, columns, gutter, ...(scope === 'page' ? { pages: [page] } : {}) }
  const preview = useCommandPreview('layout.marginsAndColumns', params)
  return (
    <DialogFrame command="layout.marginsAndColumns" title={t('dialog.marginsAndColumns')} preview={preview} onOk={async () => ((await preview?.keep(params)) ? undefined : run('layout.marginsAndColumns', params))} onClose={onClose}>
      <div className="layout-dialog-fields">
        <ChoiceRow label={t('units')} value={unit} choices={unitChoices(t)} onChange={setUnit} testId="layout-margins-unit" />
        <SidesRows label={t('margins')} sides={margins} linked={linked} labels={sideLabels(t, info.settings.facingPages)} linkLabel={t('dialog.linkSides')} unit={unit} onChange={setMargins} onLink={setLinked} testId="layout-margins" />
        <NumberRow label={t('columns')} value={columns} min={1} max={216} decimals={0} onChange={(next) => next !== null && next >= 1 && setColumns(Math.round(next))} testId="layout-margins-columns" />
        <LengthRow label={t('gutter')} value={gutter} unit={unit} onChange={(next) => next !== null && next >= 0 && setGutter(next)} testId="layout-margins-gutter" />
        <ChoiceRow label={t('dialog.applyTo')} value={scope} choices={[['all', t('dialog.allPages')], ['page', t('currentPage', { page: shown?.name ?? String(page + 1) })]] as const} onChange={setScope} testId="layout-margins-scope" />
      </div>
    </DialogFrame>
  )
}

/** The where-choice of Insert and Move Pages. */
function WhereRows({ where, page, onWhere, onPage, count }: { where: Where; page: number; onWhere: (where: Where) => void; onPage: (page: number) => void; count: number }) {
  const { t } = useKitEditor()
  return (
    <>
      <ChoiceRow label={t('dialog.where')} value={where} choices={WHERE.map((item) => [item, t(`where.${item}`)] as const)} onChange={onWhere} testId="layout-pages-where" />
      {where === 'after' || where === 'before' ? <NumberRow label={t('dialog.page')} value={page} min={1} max={count} decimals={0} onChange={(next) => next !== null && onPage(Math.round(next))} testId="layout-pages-at" /> : null}
    </>
  )
}

/** Layout › Pages › Insert Pages…: how many, where (after or before a page, at the start or end) and with which parent. */
export function InsertPagesDialog({ info, page, onClose }: { info: DocumentInfo; page: number; onClose: () => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const [count, setCount] = useState(1)
  const [where, setWhere] = useState<Where>('after')
  const [at, setAt] = useState(page + 1)
  const parents = info.parents.map((parent) => parent.label ?? '').filter(Boolean)
  const [parent, setParent] = useState(parents[0] ?? '')
  const valid = count >= 1 && count <= 9999 && (where === 'start' || where === 'end' || (at >= 1 && at <= info.pageCount))
  return (
    <DialogFrame
      command="layout.pages.insert"
      title={t('dialog.insertPages')}
      okDisabled={!valid}
      onOk={async () => {
        for (const [command, params] of insertPlan({ count, where, page: at, pages: info.pageCount, parent: parent || null })) await run(command, params)
      }}
      onClose={onClose}
    >
      <div className="layout-dialog-fields">
        <NumberRow label={t('pages')} value={count} min={1} max={9999} decimals={0} onChange={(next) => next !== null && setCount(Math.round(next))} testId="layout-insert-count" />
        <WhereRows where={where} page={at} onWhere={setWhere} onPage={setAt} count={info.pageCount} />
        <ChoiceRow label={t('pageParent')} value={parent} choices={[...parents.map((label) => [label, label] as const), ['', t('noParent')] as const]} onChange={setParent} testId="layout-insert-parent" />
      </div>
    </DialogFrame>
  )
}

/** Layout › Pages › Move Pages…: one page or a range, to after or before a page, or to the start or end. */
export function MovePagesDialog({ info, page, pages: chosen, onClose }: { info: DocumentInfo; page: number; pages?: string; onClose: () => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  // The pages chosen in the Pages panel (phase 11), else the page in view.
  const [text, setText] = useState(chosen ?? String(page + 1))
  const [where, setWhere] = useState<Where>('end')
  const [at, setAt] = useState(1)
  const moving = pagesFromText(text, info.pageCount)
  const plan = moving ? movePlan({ pages: moving, where, page: at, count: info.pageCount }) : null
  return (
    <DialogFrame
      command="layout.pages.move"
      title={t('dialog.movePages')}
      okDisabled={!plan}
      onOk={async () => {
        for (const params of plan ?? []) await run('layout.pages.move', params)
      }}
      onClose={onClose}
    >
      <div className="layout-dialog-fields">
        <TextRow label={t('dialog.movePagesWhich')} value={text} placeholder="3, 5-7" onChange={setText} testId="layout-move-pages" />
        <WhereRows where={where} page={at} onWhere={setWhere} onPage={setAt} count={info.pageCount} />
        {moving ? null : <div className="layout-dialog-warning">{t('dialog.badPages', { count: info.pageCount })}</div>}
      </div>
    </DialogFrame>
  )
}

/** Layout › Pages › Delete Pages…: which pages (the one in view at first), asked before they go. */
export function DeletePagesDialog({ info, page, pages: chosen, onClose }: { info: DocumentInfo; page: number; pages?: string; onClose: () => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  // The pages chosen in the Pages panel (phase 11), else the page in view.
  const [text, setText] = useState(chosen ?? String(page + 1))
  const pages = pagesFromText(text, info.pageCount)
  // A layout keeps one page at least.
  const valid = pages !== null && pages.length < info.pageCount
  return (
    <DialogFrame command="layout.pages.delete" title={t('dialog.deletePages')} okLabel={t('dialog.delete')} okDisabled={!valid} onOk={() => run('layout.pages.delete', { pages: (pages ?? []).map((number) => number - 1) })} onClose={onClose}>
      <div className="layout-dialog-fields">
        <TextRow label={t('pages')} value={text} placeholder="3, 5-7" onChange={setText} testId="layout-delete-pages" autoFocus />
        <p className="layout-note">{valid ? t('dialog.deletePagesNote', { count: pages?.length ?? 0 }) : t('dialog.badPages', { count: info.pageCount })}</p>
      </div>
    </DialogFrame>
  )
}

/** Layout › Go to Page…: shows the page (its spread) in the view. */
export function GoToPageDialog({ info, page, onShow, onClose }: { info: DocumentInfo; page: number; onShow: (page: number) => void; onClose: () => void }) {
  const { t } = useKitEditor()
  const [number, setNumber] = useState(page + 1)
  const valid = number >= 1 && number <= info.pageCount
  return (
    <DialogFrame command="kit:layout.goToPage" title={t('dialog.goToPage')} okDisabled={!valid} onOk={() => onShow(number - 1)} onClose={onClose}>
      <div className="layout-dialog-fields">
        <NumberRow label={t('dialog.page')} value={number} min={1} max={info.pageCount} decimals={0} onChange={(next) => next !== null && setNumber(Math.round(next))} testId="layout-go-to-page" />
      </div>
    </DialogFrame>
  )
}
