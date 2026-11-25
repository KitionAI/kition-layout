/** The Table panel's rules (plan 2026-11-25, stage 5), without React: tested in Node. */

/** The vertical justifications a cell takes (`table.setCell`'s `vj`). */
export const CELL_ALIGNS = ['top', 'center', 'bottom', 'justify'] as const

/**
 * The commands that make a table `want` rows (or columns) long from `have`: rows added after the
 * last one, or the last ones deleted (a table keeps one). Pure, so it is tested in Node.
 */
export function resizeTableSteps(table: { story: number; table: number }, axis: 'rows' | 'columns', have: number, want: number): [string, Record<string, unknown>][] {
  const at = { story: table.story, table: table.table }
  const next = Math.max(1, Math.min(axis === 'rows' ? 500 : 200, Math.round(want)))
  if (!Number.isFinite(want) || next === have || have < 1) return []
  const last = have - 1
  if (next > have) {
    return axis === 'rows'
      ? [['table.insertRow', { ...at, rows: [last, last], cols: [0, 0], where: 'below', count: next - have }]]
      : [['table.insertColumn', { ...at, rows: [0, 0], cols: [last, last], where: 'right', count: next - have }]]
  }
  return axis === 'rows' ? [['table.deleteRow', { ...at, rows: [next, last], cols: [0, 0] }]] : [['table.deleteColumn', { ...at, rows: [0, 0], cols: [next, last] }]]
}
