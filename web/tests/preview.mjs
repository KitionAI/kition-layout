// A dialog's live preview (plan 2026-11-25, stage 3): node tests/preview.mjs [chromium|webkit]
// Opens dev.html and checks, as a person works: Object › Text Frame Options… shows a changed column
// count on the frame while the dialog is open; Cancel puts it back and leaves the history as it
// was, with nothing to redo; OK keeps it as exactly one history step, which Undo takes back and Redo
// brings again; Object › Transform › Move… moves the object while its values change and Cancel puts
// it back.
import './platform.mjs'
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
const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
const one = async (command, params = {}) => (await agent([{ command, params }])).completed[0]?.result
const history = async () => one('file.history')
/** The frame as the editor reports it to its host (the engine's report, refreshed after each preview). */
const shown = async (id) => {
  const details = (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
  const search = (items) => items.flatMap((item) => [item, ...search(item.children ?? [])])
  return search(details.spreads.flatMap((spread) => spread.items)).find((item) => item.id === id) ?? null
}
const dialog = kit.getByTestId('layout-dialog')
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
}
/** Opens a menu of the menu bar and a row in it (through its submenus, `sub:<name>`). */
const menu = async (name, ...rows) => {
  await kit.getByTestId(`layout-menu-${name}`).click()
  for (const row of rows) {
    const item = row.startsWith('sub:') ? kit.getByRole('menuitem', { name: row.slice(4), exact: true }) : kit.getByTestId(row)
    if (row.startsWith('sub:')) await item.hover()
    else await item.click()
  }
}

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  const frame = await one('frame.add', { rect: [100, 400, 400, 600], content: 'text', text: 'Words '.repeat(60), caret: false })
  await until(async () => (await shown(frame.id)) !== null)
  await select(frame.id)
  const before = await history()

  // Text Frame Options: the columns show while the dialog is open.
  await menu('Object', 'layout-cmd-object.textFrameOptions')
  step('Text Frame Options… opens with its Preview on', await until(() => dialog.isVisible()))
  await kit.getByTestId('layout-tfo-columns').fill('3')
  step('the frame shows three columns while the dialog is open', await until(async () => (await shown(frame.id))?.columns === 3), JSON.stringify(await shown(frame.id)))
  await kit.getByTestId('layout-dialog-cancel').click()
  await until(async () => !(await dialog.count()))
  step('Cancel puts the frame back', await until(async () => (await shown(frame.id))?.columns === 1))
  const cancelled = await history()
  step('Cancel leaves the history as it was, with nothing to redo', JSON.stringify(cancelled) === JSON.stringify(before) && cancelled.redo.length === 0, JSON.stringify(cancelled))

  await menu('Object', 'layout-cmd-object.textFrameOptions')
  await until(() => dialog.isVisible())
  await kit.getByTestId('layout-tfo-columns').fill('2')
  await until(async () => (await shown(frame.id))?.columns === 2)
  await kit.getByTestId('layout-dialog-ok').click()
  await until(async () => !(await dialog.count()))
  const kept = await history()
  step('OK keeps it as one history step', (await shown(frame.id))?.columns === 2 && kept.undo.length === before.undo.length + 1 && kept.redo.length === 0, JSON.stringify(kept))
  // The person's Undo and Redo (an agent's would undo only its own steps).
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+z')
  step('Undo takes it back', await until(async () => (await shown(frame.id))?.columns === 1))
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+Shift+z')
  step('Redo brings it again', await until(async () => (await shown(frame.id))?.columns === 2))

  // Move…: the object moves while the values change; Cancel puts it back.
  const start = (await shown(frame.id)).bounds
  const moves = await history()
  await select(frame.id)
  await menu('Object', 'sub:Transform', 'layout-cmd-transform.move')
  step('Move… opens with its Preview on', await until(() => dialog.isVisible()))
  await kit.getByTestId('layout-dialog-param-dx').fill('36')
  await kit.getByTestId('layout-dialog-param-dy').fill('0')
  step('the frame moves while the dialog is open', await until(async () => Math.abs((await shown(frame.id)).bounds[0] - (start[0] + 36)) < 0.01), JSON.stringify((await shown(frame.id))?.bounds))
  await kit.getByTestId('layout-dialog-cancel').click()
  await until(async () => !(await dialog.count()))
  step('Cancel puts it back, with no history step', await until(async () => Math.abs((await shown(frame.id)).bounds[0] - start[0]) < 0.01) && JSON.stringify(await history()) === JSON.stringify(moves))
  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} ${error?.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
