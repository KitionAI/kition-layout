// Named things by hand (plan 2026-11-25, phase 11): node tests/styles.mjs [chromium|webkit] [section…]
// Opens dev.html and checks, as a person works: a parent's right-click menu and the Pages panel
// menu duplicate a parent, open Parent Page Options (prefix, name, based on, pages; Cancel leaves
// nothing, OK is one step) and delete a parent with its pages handed to another; and the sections
// the later items add. A section's name after the browser runs that section alone.
import './platform.mjs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const only = new Set(process.argv.slice(3))
const wanted = (section) => only.size === 0 || only.has(section)
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
const history = async () => (await one('file.history'))?.undo?.length ?? -1
const items = async () => (await details()).spreads.flatMap((spread) => spread.items)
const itemOf = async (id) => (await items()).find((item) => item.id === id)
const tab = (id) => kit.getByTestId(`layout-dock-tab-${id}`).click()
const shown = async (testId) => until(async () => (await kit.getByTestId(testId).count()) > 0)
const gone = async (testId) => until(async () => (await kit.getByTestId(testId).count()) === 0)
/** Selects an object as a person does: from its row in Layers. */
const select = async (id) => {
  await tab('layers')
  await kit.getByTestId(`layout-object-name-${id}`).scrollIntoViewIfNeeded()
  await kit.getByTestId(`layout-object-name-${id}`).click()
  await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([id]))
}
/** Types into a field (the input inside the test id, or the input itself), then Tab (Enter in a dialog is its OK). */
const fill = async (testId, value) => {
  const target = kit.getByTestId(testId)
  const input = (await target.evaluate((element) => element.tagName)) === 'INPUT' ? target : target.locator('input').first()
  await input.scrollIntoViewIfNeeded()
  await input.fill(String(value))
  await input.press('Tab').catch(() => undefined)
}
const valueOf = async (testId) => {
  const target = kit.getByTestId(testId)
  const input = ['INPUT', 'SELECT'].includes(await target.evaluate((element) => element.tagName)) ? target : target.locator('input, select').first()
  return input.inputValue()
}
const parents = async () => (await details()).parents
const parentOf = async (label) => (await parents()).find((parent) => parent.label === label)
const pageParents = async () => (await details()).spreads.flatMap((spread) => spread.pages.map((one) => one.parent))

try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await kit.locator('.kui-app').focus()
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await page.mouse.click(region.x + 20, region.y + 20)
  await page.waitForTimeout(300)

  if (wanted('parents')) {
    // ---- Parent pages: duplicate, options, delete (item 1). ----
    await tab('pages')
    const start = await history()
    await kit.getByTestId('layout-parent-A-Parent').click({ button: 'right' })
    step('a right-click on a parent opens its menu', await shown('layout-context-parent'))
    await kit.getByTestId('layout-context-row-layout.parents.duplicate').click()
    step('Duplicate Parent Spread makes B-Parent with A\'s objects', await until(async () => (await parentOf('B-Parent'))?.items === (await parentOf('A-Parent'))?.items), JSON.stringify(await parents()))
    step('… in one step', (await history()) === start + 1)

    await kit.getByTestId('layout-parent-B-Parent').click({ button: 'right' })
    await kit.getByTestId('layout-context-row-layout.parents.set').click()
    step('Parent Page Options opens with its values', (await shown('layout-parent-options')) && (await valueOf('layout-parent-prefix')) === 'B' && (await valueOf('layout-parent-name')) === 'Parent' && (await valueOf('layout-parent-pages')) === String((await parentOf('B-Parent'))?.pages))
    await fill('layout-parent-name', 'Chapter')
    await kit.getByTestId('layout-parent-based-on').selectOption('A-Parent')
    await kit.getByTestId('layout-parent-options-cancel').click()
    await gone('layout-parent-options')
    step('Cancel leaves the parent as it was, with no step', (await parentOf('B-Parent'))?.basedOn == null && (await history()) === start + 1, JSON.stringify(await parentOf('B-Parent')))

    await kit.getByTestId('layout-parent-B-Parent').click({ button: 'right' })
    await kit.getByTestId('layout-context-row-layout.parents.set').click()
    await shown('layout-parent-options')
    await fill('layout-parent-name', 'Chapter')
    await kit.getByTestId('layout-parent-based-on').selectOption('A-Parent')
    await fill('layout-parent-pages', '1')
    // A's own choices leave out B once B is based on A (no loop): checked on A's dialog below.
    await kit.getByTestId('layout-parent-options-ok').click()
    const renamed = async () => {
      const b = await parentOf('B-Chapter')
      return b?.basedOn === 'A-Parent' && b?.pages === 1
    }
    step('OK renames it, bases it on A and gives it one page', await until(renamed), JSON.stringify(await parents()))
    step('… in one step', (await history()) === start + 2)
    await kit.getByTestId('layout-parent-A-Parent').click({ button: 'right' })
    await kit.getByTestId('layout-context-row-layout.parents.set').click()
    await shown('layout-parent-options')
    const choices = await kit.getByTestId('layout-parent-based-on').locator('option').allTextContents()
    step('A cannot be based on B, which is based on A', !choices.includes('B-Chapter'), choices.join(', '))
    await kit.getByTestId('layout-parent-options-cancel').click()
    await gone('layout-parent-options')

    // The Pages panel menu has the chosen parent's rows: delete B, its pages take A.
    await one('layout.pages.applyParent', { pages: [1], parent: 'B-Chapter' })
    await until(async () => (await pageParents())[0] === 'B-Chapter')
    const beforeDelete = await history()
    await kit.getByTestId('layout-parent-B-Chapter').click()
    await kit.getByTestId('layout-pages-menu').click()
    await kit.getByTestId('layout-context-row-layout.parents.delete').click()
    step('Delete Parent Spread asks which parent its pages take', await shown('layout-delete-parent'))
    await kit.getByTestId('layout-delete-parent-replace').selectOption('A-Parent')
    await kit.getByTestId('layout-delete-parent-ok').click()
    step('OK deletes B; its page takes A', await until(async () => !(await parentOf('B-Chapter')) && (await pageParents())[0] === 'A-Parent'), JSON.stringify(await pageParents()))
    step('… in one step', (await history()) === beforeDelete + 1)
  }

  if (wanted('objectStyles')) {
    // ---- Object styles: apply, options (live, one step), "+", clear, redefine, delete (item 2). ----
    await one('swatch.add', { name: 'Card Ink', color: '#108080' })
    const id = (await one('frame.add', { spread: 0, rect: [60, 60, 160, 160], content: 'graphic' })).id
    await one('style.object.add', { name: 'Card', fromSelection: false, fill: 'Card Ink' })
    await select(id)
    await tab('styles')
    const row = kit.getByTestId('layout-object-style-Card')
    await row.scrollIntoViewIfNeeded()
    await row.click()
    step('a click applies the object style', await until(async () => (await itemOf(id))?.objectStyle === 'Card' && (await itemOf(id))?.fill === 'Card Ink'), JSON.stringify(await itemOf(id)))
    const before = await history()
    await row.dblclick()
    step('a double-click opens Object Style Options with its values', (await shown('layout-object-style-options')) && (await valueOf('layout-object-style-fill')) === 'Card Ink' && (await kit.getByTestId('layout-object-style-fill-on').isChecked()) && !(await kit.getByTestId('layout-object-style-stroke-on').isChecked()))
    await kit.getByTestId('layout-object-style-stroke-on').check()
    await kit.getByTestId('layout-object-style-stroke').selectOption('[Black]')
    await fill('layout-object-style-stroke-weight', '4')
    step('a change shows on the object at once, with no step', await until(async () => (await itemOf(id))?.stroke?.weight === 4) && (await history()) === before, JSON.stringify((await itemOf(id))?.stroke))
    await kit.getByTestId('layout-object-style-options-ok').click()
    await gone('layout-object-style-options')
    const def = async () => (await details()).styleDefs.object.find((style) => style.name === 'Card')
    step('OK keeps it in the style, one step', (await def())?.own?.strokeWeight === 4 && (await history()) === before + 1, JSON.stringify((await def())?.own))
    // An override shows "+"; Clear Overrides takes the style's fill again.
    await one('object.fill', { ids: [id], swatch: '[Paper]' })
    step('an object that overrides its style shows "+"', await until(async () => (await row.getAttribute('data-overrides')) === 'true'))
    await kit.getByTestId('layout-object-style-clear-overrides').click()
    step('Clear Overrides takes the style\'s fill again', await until(async () => (await itemOf(id))?.fill === 'Card Ink' && (await row.getAttribute('data-overrides')) !== 'true'))
    await one('object.fill', { ids: [id], swatch: '[Paper]' })
    await until(async () => (await row.getAttribute('data-overrides')) === 'true')
    await kit.getByTestId('layout-object-style-redefine').click()
    step('Redefine Style takes the object\'s fill into the style', await until(async () => (await def())?.own?.fill === '[Paper]' && (await row.getAttribute('data-overrides')) !== 'true'), JSON.stringify((await def())?.own))
    // Delete asks which style its object takes; the object keeps its look.
    await kit.getByTestId('layout-object-style-delete').click()
    step('Delete asks for a replacement when an object uses the style', await shown('layout-delete-object-style'))
    await kit.getByTestId('layout-delete-object-style-ok').click()
    step('… the object takes [None] and keeps its fill', await until(async () => (await itemOf(id))?.objectStyle === '[None]' && (await itemOf(id))?.fill === '[Paper]' && !(await def())), JSON.stringify(await itemOf(id)))
  }

  if (wanted('gradients')) {
    // ---- Gradient swatches: new with a stop added, edited live, a stop dragged off, on a stroke (item 3). ----
    // Nothing selected: a click on a swatch row would put it on the selection.
    await kit.locator('.kui-app').focus()
    await page.keyboard.press('Escape')
    await until(async () => (await details()).selection.items.length === 0)
    await tab('swatches')
    const start = await history()
    await kit.getByTestId('layout-new-gradient').click()
    await shown('layout-gradient-dialog')
    await fill('layout-gradient-name', 'Sky Test')
    const bar = await kit.getByTestId('layout-gradient-bar').boundingBox()
    await page.mouse.click(bar.x + bar.width * 0.5, bar.y + bar.height / 2)
    step('a click on the bar adds a stop', await until(async () => (await kit.locator('.layout-gradient-marker').count()) === 3))
    await kit.getByTestId('layout-gradient-dialog-ok').click()
    await gone('layout-gradient-dialog')
    const sky = async () => (await details()).colors['Sky Test']?.gradient
    step('OK makes the gradient swatch with three stops, one step', await until(async () => (await sky())?.stops?.length === 3) && (await history()) === start + 1, JSON.stringify(await sky()))
    step('… its chip shows the gradient', (await kit.getByTestId('layout-swatch-Sky Test').locator('.layout-chip').evaluate((element) => getComputedStyle(element).backgroundImage)).includes('gradient'))

    await kit.getByTestId('layout-swatch-Sky Test').dblclick()
    await shown('layout-gradient-dialog')
    const markers = async () => kit.locator('.layout-gradient-marker').count()
    const edit = await kit.getByTestId('layout-gradient-bar').boundingBox()
    const middle = await kit.getByTestId('layout-gradient-marker-1').boundingBox()
    await page.mouse.move(middle.x + middle.width / 2, middle.y + middle.height / 2)
    await page.mouse.down()
    await page.mouse.move(edit.x + edit.width * 0.65, middle.y + middle.height / 2, { steps: 4 })
    await page.mouse.move(edit.x + edit.width * 0.8, middle.y + middle.height / 2, { steps: 4 })
    await page.mouse.up()
    const moved = async () => Math.abs(((await sky())?.stops?.[1]?.location ?? 0) - 80) < 4
    step('a stop dragged along the bar moves, live with no step', (await until(moved)) && (await history()) === start + 1, JSON.stringify((await sky())?.stops))
    const again = await kit.getByTestId('layout-gradient-marker-1').boundingBox()
    await page.mouse.move(again.x + again.width / 2, again.y + again.height / 2)
    await page.mouse.down()
    await page.mouse.move(again.x + again.width / 2, again.y + 90, { steps: 6 })
    await page.mouse.up()
    step('a stop dragged off the bar goes', await until(async () => (await markers()) === 2))
    await kit.getByTestId('layout-gradient-kind').selectOption('radial')
    await kit.getByTestId('layout-gradient-dialog-ok').click()
    await gone('layout-gradient-dialog')
    step('OK keeps a radial gradient with two stops, one step', await until(async () => (await sky())?.kind === 'radial' && (await sky())?.stops?.length === 2) && (await history()) === start + 2, JSON.stringify(await sky()))

    // On a stroke from the Swatches panel.
    const id = (await one('frame.add', { spread: 0, rect: [200, 60, 300, 160], content: 'graphic' })).id
    await one('object.stroke', { ids: [id], weight: 6 })
    await select(id)
    await tab('swatches')
    await kit.getByTestId('layout-swatch-target-stroke').click()
    await kit.getByTestId('layout-swatch-Sky Test').click()
    step('a click puts the gradient on the selected object\'s stroke', await until(async () => (await itemOf(id))?.stroke?.swatch === 'Sky Test'), JSON.stringify((await itemOf(id))?.stroke))
    await kit.getByTestId('layout-swatch-target-fill').click()
  }

  if (wanted('pages')) {
    // ---- Pages chosen together; a section's mark and its options (item 4). ----
    await one('layout.pages.insert', { count: 3 })
    await tab('pages')
    const count = (await details()).pageCount
    // The three new (empty) pages.
    await kit.getByTestId('layout-page-2').click()
    await kit.getByTestId('layout-page-4').click({ modifiers: ['Shift'] })
    const active = async () => kit.locator('.layout-page-name[data-active="true"]').count()
    step('Shift chooses a run of pages', (await active()) === 3, String(await active()))
    await kit.getByTestId('layout-page-parent').selectOption('')
    const parentsOf = async () => (await details()).spreads.flatMap((spread) => spread.pages).map((one) => one.parent)
    step('the parent chosen applies to all of them', await until(async () => (await parentsOf()).slice(2, 5).every((parent) => parent === null)), JSON.stringify(await parentsOf()))
    await kit.getByTestId('layout-page-3').click({ modifiers: [process.platform === 'darwin' ? 'Meta' : 'Control'] })
    step('Cmd/Ctrl takes one away', (await active()) === 2, String(await active()))
    const before = await history()
    await kit.getByTestId('layout-delete-page').click()
    step('the trash deletes the chosen pages together, one step', await until(async () => (await details()).pageCount === count - 2) && (await history()) === before + 1, String((await details()).pageCount))
    // A section starting on page 2: its mark, and its options open with its values.
    await one('layout.section', { page: 2, startNumber: 7, style: 'lowerRoman' })
    step('a page that starts a section is marked', await until(async () => (await kit.getByTestId('layout-page-1').getAttribute('data-section')) === 'true'))
    await kit.getByTestId('layout-page-1').click({ button: 'right' })
    await kit.getByTestId('layout-context-row-layout.section').click()
    await shown('layout-dialog')
    step('Numbering & Section Options opens with the section\'s values', (await valueOf('layout-dialog-param-style')) === 'lowerRoman', await valueOf('layout-dialog-param-style'))
    await page.keyboard.press('Escape')
    await gone('layout-dialog')
  }

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} threw - ${error?.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
