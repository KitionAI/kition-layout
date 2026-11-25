// Every menu row against the built web engine (plan 2026-11-25, stage 1): in each state a person
// meets (nothing selected, a frame, three frames, an image, the text cursor, a table cell) a row
// opens its dialog, runs, or is greyed out with a reason; none ends in an English engine error.
// Every generic dialog's starting values run, and every menu row, tool and dialog control has a
// Chinese label. Skipped until `pnpm --dir web engine` has built the engine. Run with
// `pnpm --dir web test:unit` (node --test).
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'
import assert from 'node:assert/strict'
import { VARIANTS, CATALOG_COMMANDS, engineCommandOf, layoutMenus } from '../menus/catalog.ts'
import { HIDDEN } from '../menus/hidden.ts'
import { contextReason, placeParams, refusalReason } from '../menus/context.ts'
import { CATALOGS, PHRASES, withEllipsis } from '../strings.ts'
import { asks, dialogFor } from './routes.ts'
import { humanize, initialValues, paramFields, paramsOf, requiredKeys } from './params.ts'

const engineDir = new URL('../../public/engine/', import.meta.url)
const sdk = existsSync(new URL('kition_layout_kit_bg.wasm', engineDir))
  ? await (async () => {
      const module = await import(new URL('kition_layout_kit.js', engineDir).href)
      module.initSync({ module: readFileSync(new URL('kition_layout_kit_bg.wasm', engineDir)) })
      return module
    })()
  : null
const skip = sdk ? false : 'the web engine is not built'
const sample = readFileSync(new URL('../../public/sample/Sample.klayout', import.meta.url))
const commands = sdk ? JSON.parse(sdk.LayoutDocument.create('Untitled', 612, 792).commands()) : []
const byId = new Map(commands.map((command) => [command.id, command]))
const rows = () => layoutMenus(commands, asks).filter((row) => row.command !== '---')

/** A one-pixel PNG, placed as an image. */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='

/** The states a person meets, each made on the sample layout. */
const STATES = {
  nothing: (run) => run('select.set', { ids: [] }),
  frame: (run) => {
    const frame = run('frame.add', { spread: 0, rect: [40, 40, 240, 240], content: 'text', text: 'Kition a\tb\nc\td', caret: false })
    run('select.set', { ids: [frame.id] })
  },
  three: (run) => {
    const ids = [0, 1, 2].map((index) => run('frame.add', { spread: 0, rect: [40 + index * 60, 300, 90 + index * 60, 350], content: 'graphic' }).id)
    run('select.set', { ids })
  },
  image: (run) => {
    const frame = run('frame.add', { spread: 0, rect: [40, 400, 140, 500], content: 'graphic' })
    run('file.place', { base64: PNG, name: 'dot.png', frame: frame.id })
    run('select.set', { ids: [frame.id] })
  },
  caret: (run) => {
    const frame = run('frame.add', { spread: 0, rect: [40, 40, 240, 240], content: 'text', text: 'Kition a\tb\nc\td', caret: false })
    run('text.select', { story: frame.story, anchor: 3, focus: 3 })
  },
  table: (run) => {
    const frame = run('frame.add', { spread: 0, rect: [40, 40, 240, 240], content: 'text', text: 'Kition', caret: false })
    run('text.select', { story: frame.story, anchor: 6, focus: 6 })
    run('table.insert', { rows: 2, cols: 2 })
  },
}

/** A layout in `state`, with its `run` and the selection as the menu rules read it. */
function layoutIn(state) {
  const doc = sdk.LayoutDocument.open('Sample.klayout', sample)
  const run = (command, params = {}) => JSON.parse(doc.execute(command, JSON.stringify(params)) || 'null')
  STATES[state](run)
  const info = JSON.parse(doc.inspect())
  const items = info.spreads.flatMap((spread) => spread.items)
  const selected = (info.selection?.items ?? []).map((id) => items.find((item) => item.id === id)).filter(Boolean)
  const context = { items: selected.map((item) => ({ kind: item.kind, image: item.asset != null, story: item.story != null })), editingText: info.selection?.text != null }
  return { doc, run, context, disabled: new Set(JSON.parse(doc.disabledCommands())) }
}

/** What clicking `row` does in `state`: `dialog`, `runs`, `greyed` (with a reason), `refused` (worded for the person), or the English error. */
function click(row, state) {
  const variant = VARIANTS.get(row.command)
  const command = variant?.command ?? engineCommandOf(row.command)
  const layout = layoutIn(state)
  try {
    if (layout.disabled.has(command) || contextReason(command, layout.context)) return 'greyed'
    // Find Next with no search yet opens Find/Change.
    if (!variant && (asks(command) || command === 'edit.findNext')) return 'dialog'
    const params = { ...placeParams(command, { page: 0, spread: 0 }), ...variant?.params }
    // In text, Paste takes the system clipboard's text (the editor reads it).
    if (command.startsWith('edit.paste') && layout.context.editingText) params.text = 'clipboard'
    layout.run(command, params)
    return 'runs'
  } catch (error) {
    return refusalReason(error) ? 'refused' : `error: ${String(error).slice(0, 160)}`
  } finally {
    layout.doc.free()
  }
}

test('every engine command with a menu path is in the menus or left out with a reason', { skip }, () => {
  const missing = commands.filter((command) => command.menu?.length && !CATALOG_COMMANDS.has(command.id) && !HIDDEN[command.id]).map((command) => command.id)
  assert.deepEqual(missing, [])
  for (const [command, reason] of Object.entries(HIDDEN)) assert.ok(reason.trim().length > 10, command)
})

test('no menu row ends in an English engine error, in any state a person meets', { skip }, (t) => {
  const failures = []
  const best = new Map()
  const rank = { dialog: 3, runs: 3, refused: 2, greyed: 1 }
  for (const row of rows()) {
    if (row.command.startsWith('kit:')) continue
    for (const state of Object.keys(STATES)) {
      const outcome = click(row, state)
      if (outcome.startsWith('error')) failures.push(`${state} ${row.command}: ${outcome}`)
      const was = best.get(row.command)
      if (!was || (rank[outcome] ?? 0) > (rank[was] ?? 0)) best.set(row.command, outcome)
    }
  }
  assert.deepEqual(failures, [], 'rows that end in an English engine error')
  const counts = {}
  for (const outcome of best.values()) counts[outcome] = (counts[outcome] ?? 0) + 1
  t.diagnostic(`rows by their best outcome across the states: ${JSON.stringify(counts)}; kit rows ${rows().filter((row) => row.command.startsWith('kit:')).length}; left out ${Object.keys(HIDDEN).length}`)
})

/** Values a person types where the dialog cannot start with one (a name, what a QR code encodes). */
const TYPED = {
  name: 'Sample', latex: 'x^2', word: 'kition', family: 'Inter', toFamily: 'Lora', definition: '<pageNum />', content: 'https://kition.ai', styles: '[Basic Paragraph]',
  topic: 'Kition', paragraph: 'Kition', dx: 12, dy: 12, sx: 1.5, sy: 1.5, angle: 15, size: 6, width: 300, height: 400, page: 1, pages: '1', spread: 0, text: 'Kition',
}

test("every generic dialog's starting values run, once the values a person must type are in", { skip }, () => {
  const refused = []
  let ran = 0
  for (const command of commands) {
    if (dialogFor(command.id) !== 'params') continue
    const fields = paramFields(command)
    const required = requiredKeys(command)
    const typed = Object.fromEntries(fields.filter((field) => required.includes(field.key) && TYPED[field.key] !== undefined).map((field) => [field.key, TYPED[field.key]]))
    const params = { ...placeParams(command.id, { page: 0, spread: 0 }), ...paramsOf(fields, initialValues(fields, typed)) }
    // The first state where the row is not greyed out.
    let tried = false
    for (const state of ['frame', 'image', 'three', 'caret', 'table', 'nothing']) {
      const layout = layoutIn(state)
      try {
        // Insert Variable chooses one of the document's variables: there is one.
        if (command.id === 'variable.insert') layout.run('variable.define', { name: TYPED.name, type: 'custom' })
        if (layout.disabled.has(command.id) || contextReason(command.id, layout.context)) continue
        tried = true
        layout.run(command.id, params)
        ran += 1
      } catch (error) {
        refused.push(`${command.id} ${JSON.stringify(params)}: ${String(error).slice(0, 160)}`)
      } finally {
        layout.doc.free()
      }
      if (tried) break
    }
  }
  assert.deepEqual(refused, [], 'dialogs whose starting values the engine refuses')
  assert.ok(ran > 40, `ran ${ran}`)
})

test('every menu row, submenu, tool and dialog control has a Chinese label', { skip }, () => {
  const zh = CATALOGS['zh-CN']
  const phrase = withEllipsis((english) => PHRASES['zh-CN'][english] ?? english)
  const missing = []
  for (const row of rows()) {
    if (!/^\d+ pt$/.test(row.label) && phrase(row.label) === row.label) missing.push(`row ${row.label}`)
    for (const part of row.path) if (phrase(part) === part) missing.push(`menu ${part}`)
  }
  for (const tool of JSON.parse(sdk.LayoutDocument.create('x', 10, 10).tools())) if (phrase(tool.label) === tool.label) missing.push(`tool ${tool.label}`)
  for (const command of commands) {
    if (dialogFor(command.id) !== 'params') continue
    if (phrase(command.title) === command.title) missing.push(`title ${command.title}`)
    for (const field of paramFields(command)) {
      if (!zh[`param.${field.key}`] && phrase(humanize(field.key)) === humanize(field.key)) missing.push(`${command.id}: ${field.key}`)
      if (field.kind === 'choice') for (const option of field.options) if (option && !zh[`option.${option}`]) missing.push(`${command.id}: ${field.key}=${option}`)
    }
  }
  assert.deepEqual([...new Set(missing)], [], 'labels without a Chinese translation (add them to strings.ts)')
})
