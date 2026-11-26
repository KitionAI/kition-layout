import { Choice, NumberField, useKitEditor } from '@kition/sdk/editor'
import { usePanelQuery } from '../query'
import type { DocumentInfo } from '../../layout'
import { useEditorView } from '../../viewContext'
import { LengthField, PropertyGroup } from '../fields'
import { CELL_ALIGNS, resizeTableSteps } from '../tableLogic'

/** What `table.get` answers about the table at the caret or the selected cells. */
export type TableInfo = {
  story: number
  table: number
  rows: { height: number; mode: 'atLeast' | 'exactly' }[]
  columns: { width: number }[]
  headerRows: number
  cells: { row: number; col: number; insets?: number[]; vj?: string }[]
  range: { r0: number; c0: number; r1: number; c1: number }
}

/**
 * Table (InDesign's Table panel, plan 2026-11-25, stage 5), with the caret in a table or cells
 * selected: the number of rows and columns, the height of the rows and the width of the columns
 * the selection is in, the cells' vertical alignment and inset, and the header rows. Each runs the
 * engine's `table.*` command (one undo step).
 */
export function TableGroup({ info }: { info: DocumentInfo }) {
  const { run, t } = useKitEditor()
  const { unit } = useEditorView().view
  const table = usePanelQuery<TableInfo>('table.get', {}, `${info.textStory}:${info.editingText}`)
  if (!table?.rows?.length || !table.columns?.length) return null
  const range = table.range ?? { r0: 0, c0: 0, r1: 0, c1: 0 }
  const row = table.rows[range.r0] ?? table.rows[0]
  const column = table.columns[range.c0] ?? table.columns[0]
  const cell = table.cells?.find((item) => item.row === range.r0 && item.col === range.c0)
  const at = { story: table.story, table: table.table, rows: [range.r0, range.r1], cols: [range.c0, range.c1] }
  const steps = async (list: [string, Record<string, unknown>][]) => {
    for (const [command, params] of list) await run(command, params)
  }
  return (
    <PropertyGroup title={t('props.table')} testId="layout-table">
      <div className="layout-grid layout-grid-tight">
        <NumberField label={t('props.tableRows')} value={table.rows.length} min={1} max={500} step={1} onChange={(value, final) => final && void steps(resizeTableSteps(table, 'rows', table.rows.length, value))} testId="layout-table-rows" />
        <NumberField label={t('props.tableColumns')} value={table.columns.length} min={1} max={200} step={1} onChange={(value, final) => final && void steps(resizeTableSteps(table, 'columns', table.columns.length, value))} testId="layout-table-columns" />
        <LengthField label={t('props.rowHeight')} value={row?.height ?? null} unit={unit} min={3} max={10000} onCommit={(height) => void run('table.setRowHeight', { ...at, height })} testId="layout-row-height" />
        <Choice label={t('props.rowHeightMode')} value={row?.mode ?? 'atLeast'} choices={(['atLeast', 'exactly'] as const).map((mode) => [mode, t(`rowHeight.${mode}`)] as const)} onChange={(mode) => void run('table.setRowHeight', { ...at, mode })} testId="layout-row-height-mode" />
        <LengthField label={t('props.columnWidth')} value={column?.width ?? null} unit={unit} min={3} max={10000} onCommit={(width) => void run('table.setColumnWidth', { ...at, width })} testId="layout-column-width" />
        <LengthField label={t('props.cellInset')} value={cell?.insets?.[0] ?? null} unit={unit} min={0} max={1000} onCommit={(insets) => void run('table.setCell', { ...at, insets })} testId="layout-cell-inset" />
        <Choice label={t('props.cellAlign')} value={cell?.vj ?? 'top'} choices={CELL_ALIGNS.map((vj) => [vj, t(`cellAlign.${vj}`)] as const)} onChange={(vj) => void run('table.setCell', { ...at, vj })} testId="layout-cell-align" />
        <NumberField label={t('props.headerRows')} value={table.headerRows ?? 0} min={0} max={Math.max(0, table.rows.length - 1)} step={1} onChange={(value, final) => final && void run('table.options', { story: table.story, table: table.table, headerRows: Math.max(0, Math.round(value)) })} testId="layout-header-rows" />
      </div>
    </PropertyGroup>
  )
}
