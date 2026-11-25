// The menus ask before they run (plan 2026-11-25, stage 1): node tests/dialogs.mjs [chromium|webkit]
// Opens dev.html in Chinese with the menu bar and checks, as a person clicks: the rows are in
// Chinese and the ones that ask end in "…"; Text Frame Options opens a dialog and changes nothing
// until OK, Cancel leaves the history as it was, OK adds exactly one step; File › New… writes an
// A4 layout in millimetres with four margins of its own; Insert Table brings the Table menu to
// life; Find/Change changes text; a greyed-out row says why; the sample asks before it replaces an
// edited document; Delete Pages asks first; a click with the Rectangle Frame tool asks the size.
import './platform.mjs'
import { readFileSync } from 'node:fs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
context.setDefaultTimeout(15000)
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 80) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const kit = page.frameLocator('#kit')
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
const one = async (command, params = {}) => (await agent([{ command, params }])).completed[0]?.result
/** The person's history, as the engine lists it. */
const history = async () => one('file.history')
const itemOf = async (id) => {
  const search = (items) => items.flatMap((item) => [item, ...search(item.children ?? [])])
  return search((await details()).spreads.flatMap((spread) => spread.items)).find((item) => item.id === id)
}
/** Selects an object as a person does, from its row in Layers (scrolled into the list's drawn rows). */
const select = async (id) => {
  const list = kit.getByTestId('layout-layer-list')
  const row = kit.getByTestId(`layout-object-name-${id}`)
  for (let top = 0; !(await row.count()); top += 200) {
    const past = await list.evaluate((element, at) => {
      element.scrollTop = at
      return element.scrollTop < at
    }, top)
    await page.waitForTimeout(50)
    if (past) break
  }
  await row.click()
  await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([id]))
}
/** Opens a menu of the menu bar and a row in it (through its submenus). */
const menu = async (name, ...rows) => {
  await kit.getByTestId(`layout-menu-${name}`).click()
  for (const row of rows) {
    const item = row.startsWith('sub:') ? kit.getByRole('menuitem', { name: row.slice(4), exact: true }) : kit.getByTestId(row)
    if (row.startsWith('sub:')) await item.hover()
    else await item.click()
  }
}
const dialog = kit.getByTestId('layout-dialog')
const press = async (keys) => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press(keys)
}

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1000)
  const pageBox = (await details()).spreads[0].pages[0].bounds

  // The Object menu: in Chinese, the rows that ask end in "…", and the Window rows came home.
  const frame = await one('frame.add', { rect: [pageBox[0] + 60, 60, pageBox[0] + 300, 200], content: 'text', text: 'Kition 排版 Kition', caret: false })
  await select(frame.id)
  await kit.getByTestId('layout-menu-Object').click()
  const tfo = kit.getByTestId('layout-cmd-object.textFrameOptions')
  step('Object › Text Frame Options… reads in Chinese and asks first', (await tfo.textContent())?.includes('文本框架选项…'), await tfo.textContent())
  const group = kit.getByTestId('layout-cmd-arrange.group')
  step('Group is greyed out with one object, and says why', (await group.isDisabled()) && (await until(async () => (await group.getAttribute('title'))?.includes('两个'))), String(await group.getAttribute('title')))
  await kit.getByRole('menuitem', { name: '对齐', exact: true }).hover()
  step('Object › Align holds Align Left Edges (moved from Window)', await kit.getByTestId('layout-cmd-object.align|left').isVisible())
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  step('the Window menu is gone (its rows moved home)', (await kit.getByTestId('layout-menu-Window').count()) === 0)

  // Text Frame Options: nothing changes until OK; Cancel leaves the history; OK adds one step.
  const before = await history()
  await select(frame.id)
  await menu('Object', 'layout-cmd-object.textFrameOptions')
  step('Text Frame Options… opens a dialog', await dialog.isVisible())
  step('opening it changes nothing', JSON.stringify(await history()) === JSON.stringify(before) && (await itemOf(frame.id)).columns === 1)
  await kit.getByTestId('layout-tfo-columns').fill('3')
  await kit.getByTestId('layout-dialog-cancel').click()
  await until(async () => !(await dialog.count()))
  step('Cancel leaves the frame and the history (and nothing to redo)', (await itemOf(frame.id)).columns === 1 && JSON.stringify(await history()) === JSON.stringify(before), JSON.stringify(await history()))
  await select(frame.id)
  await press('ControlOrMeta+b')
  step('its key (Cmd+B) opens the dialog too', await until(async () => dialog.isVisible()))
  await kit.getByTestId('layout-tfo-columns').fill('2')
  await kit.getByTestId('layout-tfo-inset-top').fill('6')
  await kit.getByTestId('layout-dialog-ok').click()
  await until(async () => (await itemOf(frame.id)).columns === 2)
  const after = await history()
  step('OK sets the columns in one history step', (await itemOf(frame.id)).columns === 2 && after.undo.length === before.undo.length + 1 && after.redo.length === 0, JSON.stringify(after))

  // A generic dialog: Corner Options asks its shape (a drop-down in Chinese) and size.
  await select(frame.id)
  await menu('Object', 'layout-cmd-object.cornerOptions')
  const shape = kit.getByTestId('layout-dialog-param-shape')
  step('Corner Options… asks its shape in Chinese', (await shape.locator('option').allTextContents()).includes('圆角'))
  await shape.selectOption('rounded')
  await kit.getByTestId('layout-dialog-param-size').fill('9')
  await kit.getByTestId('layout-dialog-ok').click()
  await until(async () => (await itemOf(frame.id)).corner?.size === 9)
  step('its values run', (await itemOf(frame.id)).corner?.shape === 'rounded' && (await itemOf(frame.id)).corner?.size === 9, JSON.stringify((await itemOf(frame.id)).corner))

  // Find/Change, open beside the layout: Change All.
  await menu('Edit', 'layout-cmd-edit.find')
  await kit.getByTestId('layout-find-text').fill('Kition')
  await kit.getByTestId('layout-find-change-text').fill('Layout')
  await kit.getByTestId('layout-find-change-all').click()
  const storyText = async () => (await one('story.get', { frame: frame.id }))?.text ?? ''
  await until(async () => (await storyText()).includes('Layout 排版 Layout'))
  step('Find/Change › Change All changes every match', (await storyText()).includes('Layout 排版 Layout'), await storyText())
  step('and says how many', (await kit.getByTestId('layout-find-status').textContent())?.includes('2'), await kit.getByTestId('layout-find-status').textContent())
  await kit.getByTestId('layout-find-done').click()

  // Insert Table: greyed out until the text cursor is in text; then the Table menu comes alive.
  await kit.getByTestId('layout-menu-Table').click()
  const insertTable = kit.getByTestId('layout-cmd-table.insert')
  step('Table › Insert Table… is the first row, greyed out without a text cursor', (await insertTable.isDisabled()) && (await kit.locator('.kui-menu-item').first().getAttribute('data-testid')) === 'layout-cmd-table.insert')
  await page.keyboard.press('Escape')
  const typing = await one('frame.add', { rect: [pageBox[0] + 60, 260, pageBox[0] + 400, 420], content: 'text', text: 'Table here', caret: false })
  await kit.getByTestId('layout-tool-type').click()
  await kit.getByTestId('layout-spread-0').click()
  await page.waitForTimeout(400)
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  const placed = (await details()).canvas.spreads[0]
  const [zoom, vx, vy] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  const left = Math.min(...(await details()).spreads[0].pages.map((p) => p.bounds[0]))
  await page.mouse.click(region.x + (placed.x + pageBox[0] + 200 - left - vx) * zoom, region.y + (placed.y + 280 - vy) * zoom)
  await until(async () => (await details()).selection.text != null)
  await menu('Table', 'layout-cmd-table.insert')
  await kit.getByTestId('layout-table-rows').fill('2')
  await kit.getByTestId('layout-table-cols').fill('3')
  await kit.getByTestId('layout-dialog-ok').click()
  await until(async () => (await details()).selection.text?.cell != null)
  await menu('Table', 'sub:插入')
  step('after Insert Table the caret is in the first cell and the Table rows are alive', !(await kit.getByTestId('layout-cmd-table.insertRowBelow').isDisabled()) && (await details()).selection.text?.cell?.row === 0, JSON.stringify((await details()).selection))
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  await kit.getByTestId('layout-tool-selection').click()
  void typing

  // A click with the Rectangle Frame tool asks the frame's size (the engine's dialog request).
  const count = async () => (await details()).spreads[0].items.length
  const items = await count()
  await kit.getByTestId('layout-tool-rectangleFrame').click()
  await page.mouse.click(region.x + (placed.x + pageBox[0] + 100 - left - vx) * zoom, region.y + (placed.y + 600 - vy) * zoom)
  step('a click with the Rectangle Frame tool asks the size', await until(async () => dialog.isVisible()))
  await kit.getByTestId('layout-frame-width').fill('100')
  await kit.getByTestId('layout-dialog-ok').click()
  await until(async () => (await count()) === items + 1)
  const made = (await details()).spreads[0].items.at(-1)
  step('OK makes a frame of that size', (await count()) === items + 1 && Math.abs(made.bounds[2] - made.bounds[0] - 100) < 0.01, JSON.stringify(made?.bounds))
  // A click with the Page tool asks the page's size.
  await kit.getByTestId('layout-tool-page').click()
  await page.mouse.click(region.x + (placed.x + pageBox[0] + 300 - left - vx) * zoom, region.y + (placed.y + 500 - vy) * zoom)
  step('a click with the Page tool asks the page size', await until(async () => (await dialog.isVisible()) && (await kit.getByTestId('layout-dialog-param-width').count()) === 1))
  if (await dialog.isVisible()) await kit.getByTestId('layout-dialog-cancel').click()
  await kit.getByTestId('layout-tool-selection').click()

  // Delete Pages asks first.
  const pages = (await details()).pageCount
  await menu('Layout', 'sub:页面', 'layout-cmd-layout.pages.delete')
  step('Delete Pages… asks first', await dialog.isVisible())
  await kit.getByTestId('layout-dialog-cancel').click()
  step('Cancel keeps the pages', (await details()).pageCount === pages)

  // Help › Sample Document over an edited layout asks first; Cancel keeps it.
  await menu('Help', 'layout-cmd-file.newSample')
  step('the sample asks before it replaces an edited document', await kit.getByTestId('layout-sample-confirm').isVisible())
  await kit.getByTestId('layout-sample-confirm-cancel').click()
  step('Cancel keeps the document', (await details()).pageCount === pages && (await itemOf(frame.id)) != null)

  // File › New…: an A4 layout in millimetres, four margins of its own, written beside the document.
  await kit.getByTestId('layout-menu-File').click()
  await kit.getByTestId('layout-menu-new').click()
  step('File › New… opens New Document in millimetres', (await kit.getByTestId('layout-new-unit').inputValue()) === 'mm' && (await kit.getByTestId('layout-new-width').inputValue()) === '210')
  await kit.getByTestId('layout-new-name').fill('Brochure')
  await kit.getByTestId('layout-new-margins-link').uncheck()
  for (const [side, value] of [['top', '20'], ['bottom', '25'], ['inside', '15'], ['outside', '10']]) await kit.getByTestId(`layout-new-margins-${side}`).fill(value)
  await kit.getByTestId('layout-new-pages').fill('3')
  await kit.getByTestId('layout-dialog-ok').click()
  const written = () => page.evaluate(() => [...window.devHost.files.keys()].find((path) => path.startsWith('Brochure')))
  await until(async () => (await written()) != null, 150)
  const path = await written()
  step('it writes the new layout beside the document', path === 'Brochure.klayout', String(path))
  const bytes = path ? await page.evaluate((file) => Array.from(new Uint8Array(window.devHost.files.get(file).bytes)), path) : []
  const engine = await import(new URL('../public/engine/kition_layout_kit.js', import.meta.url).href)
  engine.initSync({ module: readFileSync(new URL('../public/engine/kition_layout_kit_bg.wasm', import.meta.url)) })
  const opened = bytes.length ? JSON.parse(engine.LayoutDocument.open('Brochure.klayout', new Uint8Array(bytes)).inspect()) : null
  const first = opened?.spreads[0]?.pages[0]
  const mm = (points) => Math.round((points / 72) * 25.4 * 100) / 100
  step('A4, three pages, facing, margins 20 / 25 / 15 / 10 mm', opened?.pageCount === 3 && mm(opened.settings.pageWidth) === 210 && mm(opened.settings.pageHeight) === 297 && opened.settings.facingPages && JSON.stringify(first && [mm(first.margins.top), mm(first.margins.bottom), mm(first.margins.inside), mm(first.margins.outside)]) === '[20,25,15,10]', JSON.stringify({ settings: opened?.settings, margins: first?.margins }))
  step('the open document stays as it was', (await details()).pageCount === pages)

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
