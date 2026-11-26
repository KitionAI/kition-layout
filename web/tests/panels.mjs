// Panels and styles by hand (plan 2026-11-25, stage 7): node tests/panels.mjs [chromium|webkit]
// Opens dev.html and checks, as a person works: a paragraph style's options open with its own
// values, show a change on the text at once, Cancel takes it back and OK keeps it as one step;
// a text override shows "+" and Redefine Style takes it into the style; a style is duplicated and
// deleted with its text handed to another; Swatch Options changes a swatch's colour (live),
// name and process/spot, and everything using it follows; a swatch in use is deleted with a
// replacement; a click on a layer's name makes it the layer new objects go on; layers and
// objects are dragged in the Layers panel (one undo step each); Layer Options sets a layer's
// name, colour and printing; a page dragged in the Pages panel moves, a parent dragged onto a
// page applies, a double-click on a page shows it, the panel menu opens Insert Pages, and a
// double-click on a parent edits it (an object drawn there lands on the parent); the Effects
// dialog shows opacity and a drop shadow live, Cancel leaves nothing and OK is one undo step.
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
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
const one = async (command, params = {}) => (await agent([{ command, params }])).completed[0]?.result
const history = async () => (await one('file.history'))?.undo?.length ?? -1
const items = async () => (await details()).spreads.flatMap((spread) => spread.items)
const itemOf = async (id) => (await items()).find((item) => item.id === id)
const styleDef = async (name) => (await details()).styleDefs.paragraph.find((style) => style.name === name)
const attrsOf = async (id) => (await agent([{ command: 'select.set', params: { ids: [id] } }, { command: 'text.selectionAttrs', params: {} }])).completed[1]?.result
const tab = (id) => kit.getByTestId(`layout-dock-tab-${id}`).click()
/** Selects an object as a person does: from its row in Layers. */
const select = async (id) => {
  await tab('layers')
  await kit.getByTestId(`layout-object-name-${id}`).click()
  await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([id]))
}
const deselect = async () => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('Escape')
  await until(async () => (await details()).selection.items.length === 0)
}
/** Types into a field (the input inside the test id, or the input itself); `enter` for a panel's field. */
const fill = async (testId, value, enter = false) => {
  const target = kit.getByTestId(testId)
  const input = (await target.evaluate((element) => element.tagName)) === 'INPUT' ? target : target.locator('input').first()
  await input.scrollIntoViewIfNeeded()
  await input.fill(String(value))
  // Tab, not Enter: Enter in a dialog is its OK.
  await input.press(enter ? 'Enter' : 'Tab').catch(() => undefined)
}
const valueOf = async (testId) => kit.getByTestId(testId).inputValue()
/** Drags from the middle of one element to a point of another (`at`: its fraction across and down). */
const drag = async (from, to, at = [0.5, 0.5]) => {
  // The dock's groups are short: both rows into view first.
  await kit.getByTestId(to).scrollIntoViewIfNeeded()
  await kit.getByTestId(from).scrollIntoViewIfNeeded()
  const a = await kit.getByTestId(from).boundingBox()
  const b = await kit.getByTestId(to).boundingBox()
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2)
  await page.mouse.down()
  await page.mouse.move(a.x + a.width / 2 + 3, a.y + a.height / 2 + 6, { steps: 3 })
  await page.mouse.move(b.x + b.width * at[0], b.y + b.height * at[1], { steps: 8 })
  await page.mouse.up()
}
/** Draws a rectangle with the Rectangle tool from the middle of the canvas region. */
const drawRectangle = async (dx = 0, dy = 0) => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('m')
  await until(async () => (await details()).tool === 'rectangle')
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  const x = region.x + region.width / 2 + dx
  const y = region.y + region.height / 2 + dy
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x + 30, y + 20, { steps: 4 })
  await page.mouse.move(x + 60, y + 40, { steps: 4 })
  await page.mouse.up()
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('v')
}

try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await kit.locator('.kui-app').focus()
  // A first press on the canvas (the editor leaves an empty opening layer then; stage 4).
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await page.mouse.click(region.x + 20, region.y + 20)
  await page.waitForTimeout(300)

  // ---- Paragraph Style Options: its own values, live, Cancel, OK. ----
  await one('style.paragraph.add', { name: 'Body Test', chars: { size: 11 }, para: { spaceAfter: 4 } })
  await select(7)
  await tab('styles')
  await kit.getByTestId('layout-paragraph-style-Body Test').click()
  await until(async () => ((await styleDef('Body Test'))?.uses ?? 0) > 0)
  step('a click applies the style to the frame', ((await styleDef('Body Test'))?.uses ?? 0) > 0)
  await kit.getByTestId('layout-paragraph-style-Body Test').dblclick()
  await until(async () => (await kit.getByTestId('layout-style-dialog').count()) === 1)
  step('a double-click opens the style options with its own values', (await valueOf('layout-style-size')) === '11' && (await valueOf('layout-style-spaceAfter')) === '4', `${await valueOf('layout-style-size')} / ${await valueOf('layout-style-spaceAfter')}`)
  step('… its Based On and the Indents and Spacing section', (await kit.getByTestId('layout-style-based-on').count()) === 1 && (await kit.getByTestId('layout-style-section-indentsSpacing').count()) === 1)
  const before = await history()
  await fill('layout-style-size', '20')
  step('a change shows on the style at once', await until(async () => (await styleDef('Body Test'))?.resolvedChars?.size === 20))
  step('… with no history step', (await history()) === before)
  await kit.getByTestId('layout-style-cancel').click()
  await until(async () => (await kit.getByTestId('layout-style-dialog').count()) === 0)
  step('Cancel takes the change back', await until(async () => (await styleDef('Body Test'))?.resolvedChars?.size === 11), String((await styleDef('Body Test'))?.resolvedChars?.size))
  step('… and leaves no step to undo or redo', (await history()) === before && !(await details()).canRedo)
  await kit.getByTestId('layout-paragraph-style-Body Test').dblclick()
  await until(async () => (await kit.getByTestId('layout-style-dialog').count()) === 1)
  await fill('layout-style-size', '20')
  await fill('layout-style-leftIndent', '6')
  await until(async () => (await styleDef('Body Test'))?.resolvedPara?.leftIndent === 6)
  await kit.getByTestId('layout-style-ok').click()
  await until(async () => (await kit.getByTestId('layout-style-dialog').count()) === 0)
  step('OK keeps the style as one step', await until(async () => (await styleDef('Body Test'))?.resolvedChars?.size === 20 && (await history()) === before + 1), `${(await styleDef('Body Test'))?.resolvedChars?.size} ${await history()} vs ${before}`)
  step('… and its text follows', (await attrsOf(7))?.chars?.size === 20 && (await attrsOf(7))?.para?.leftIndent === 6)

  // ---- Overrides and Redefine Style. ----
  await select(7)
  await tab('properties')
  await fill('layout-font-size', '26', true)
  await tab('styles')
  const overridden = kit.locator('[data-testid="layout-paragraph-style-Body Test"][data-overrides="true"]')
  step('a text override marks the style with "+"', await until(async () => (await overridden.count()) === 1))
  await kit.getByTestId('layout-paragraph-style-redefine').click()
  step('Redefine Style takes the text\'s look into the style', await until(async () => (await styleDef('Body Test'))?.resolvedChars?.size === 26), String((await styleDef('Body Test'))?.resolvedChars?.size))
  step('… and the "+" goes', await until(async () => (await overridden.count()) === 0))

  // ---- Duplicate and Delete with a replacement. ----
  await kit.getByTestId('layout-paragraph-style-Body Test').click()
  await kit.getByTestId('layout-paragraph-style-duplicate').click()
  step('Duplicate makes a copy with the same values', await until(async () => (await styleDef('Body Test copy'))?.resolvedChars?.size === 26))
  await kit.getByTestId('layout-paragraph-style-Body Test').click()
  await kit.getByTestId('layout-paragraph-style-delete').click()
  step('deleting a style text uses asks for a replacement', await until(async () => (await kit.getByTestId('layout-delete-style').count()) === 1))
  await kit.getByTestId('layout-delete-style-replace').selectOption('Body Test copy')
  await kit.getByTestId('layout-delete-style-ok').click()
  step('… and its text takes the replacement', await until(async () => !(await details()).paragraphStyles.includes('Body Test') && ((await styleDef('Body Test copy'))?.uses ?? 0) > 0))

  // ---- Swatch Options and Delete Swatch. ----
  await one('swatch.add', { name: 'Ink Test', color: '#336699' })
  await one('object.fill', { ids: [11], swatch: 'Ink Test' })
  await deselect()
  await tab('swatches')
  await kit.getByTestId('layout-swatch-Ink Test').dblclick()
  step('a double-click on a swatch opens Swatch Options', await until(async () => (await kit.getByTestId('layout-swatch-options').count()) > 0))
  step('… with its name', (await valueOf('layout-swatch-options-name')) === 'Ink Test')
  const beforeSwatch = await history()
  await fill('layout-swatch-options-color-r', '255')
  step('a colour change shows at once', await until(async () => (await details()).colors['Ink Test']?.hex === '#ff6699'), (await details()).colors['Ink Test']?.hex)
  await kit.getByTestId('layout-swatch-options-cancel').click()
  step('Cancel takes it back, no step', await until(async () => (await details()).colors['Ink Test']?.hex === '#336699') && (await history()) === beforeSwatch)
  await kit.getByTestId('layout-swatch-Ink Test').dblclick()
  await until(async () => (await kit.getByTestId('layout-swatch-options').count()) > 0)
  await fill('layout-swatch-options-name', 'Ocean Test')
  await kit.getByTestId('layout-swatch-options-type').selectOption('spot')
  await fill('layout-swatch-options-color-r', '255')
  await kit.getByTestId('layout-swatch-options-ok').click()
  step('OK renames the swatch, makes it spot, keeps the colour', await until(async () => (await details()).colors['Ocean Test']?.spot === true && (await details()).colors['Ocean Test']?.hex === '#ff6699'))
  step('… the object using it follows, in one step', (await itemOf(11))?.fill === 'Ocean Test' && (await history()) === beforeSwatch + 1, `${(await itemOf(11))?.fill} ${await history()}`)
  await kit.getByTestId('layout-swatch-Ocean Test').click()
  await kit.getByTestId('layout-delete-swatch').click()
  step('deleting a swatch in use asks for a replacement', await until(async () => (await kit.getByTestId('layout-delete-swatch-dialog').count()) > 0))
  await kit.getByTestId('layout-delete-swatch-dialog-ok').click()
  step('… and what used it takes [Black]', await until(async () => (await itemOf(11))?.fill === '[Black]' && !(await details()).swatches.includes('Ocean Test')))

  // ---- Layers: the active layer, dragging, Layer Options. ----
  await deselect()
  await tab('layers')
  await kit.getByTestId('layout-layer-name-16').click()
  step('a click on a layer\'s name makes it the active layer', await until(async () => (await details()).activeLayer === 16))
  const was = new Set((await items()).map((item) => item.id))
  await drawRectangle(-150, -100)
  await until(async () => (await items()).some((item) => !was.has(item.id)))
  const onNotes = (await items()).find((item) => !was.has(item.id))
  step('… new objects go on it', onNotes?.layer === 16, JSON.stringify(onNotes && { id: onNotes.id, layer: onNotes.layer }))
  await deselect()
  await tab('layers')
  await kit.getByTestId('layout-layer-name-1').click()
  step('another layer\'s name makes that one active', await until(async () => (await details()).activeLayer === 1))

  const beforeObjects = await history()
  await drag(`layout-object-${onNotes.id}`, 'layout-layer-1', [0.5, 0.5])
  step('an object dragged onto a layer goes onto it', await until(async () => (await itemOf(onNotes.id))?.layer === 1))
  step('… in one undo step', (await history()) === beforeObjects + 1)
  const spreadItems = async () => (await details()).spreads[0].items.map((item) => item.id)
  step('… in front of its objects', (await spreadItems()).at(-1) === onNotes.id, (await spreadItems()).join(','))
  // Dragged onto the upper half of frame 9's row: just in front of frame 9.
  await drag(`layout-object-${onNotes.id}`, 'layout-object-9', [0.5, 0.3])
  step('an object dragged onto another\'s row goes just in front of it', await until(async () => {
    const order = await spreadItems()
    return order.indexOf(onNotes.id) === order.indexOf(9) + 1
  }), (await spreadItems()).join(','))

  const layerOrder = async () => (await details()).layers.map((layer) => layer.id).join(',')
  const beforeLayers = await history()
  await drag('layout-layer-16', 'layout-layer-1', [0.5, 0.8])
  step('a layer dragged below another moves there', await until(async () => (await layerOrder()) === '1,16'), await layerOrder())
  step('… in one undo step', (await history()) === beforeLayers + 1)
  await kit.getByTestId('layout-layer-color-1').dblclick()
  step('a double-click on a layer\'s colour opens Layer Options', await until(async () => (await kit.getByTestId('layout-layer-options').count()) > 0))
  step('… with its name', (await valueOf('layout-layer-options-name')) === 'Layer 1')
  const beforeOptions = await history()
  await fill('layout-layer-options-name', 'Body Layer')
  step('a new name shows at once', await until(async () => (await details()).layers.some((layer) => layer.name === 'Body Layer')))
  await kit.getByTestId('layout-layer-options-color').selectOption('255,0,0')
  await kit.getByTestId('layout-layer-options-printable').click()
  await kit.getByTestId('layout-layer-options-ok').click()
  const body = async () => (await details()).layers.find((layer) => layer.id === 1)
  step('OK sets the name, colour and printing', await until(async () => (await body())?.name === 'Body Layer' && (await body())?.printable === false && (await body())?.color?.join(',') === '255,0,0'), JSON.stringify(await body()))
  step('… in one step', (await history()) === beforeOptions + 1)

  // ---- Pages: drag a page, drag a parent, double-click, the panel menu, edit a parent. ----
  await one('layout.pages.insert', { count: 1 })
  await one('layout.parents.new', { name: 'Alt' })
  await tab('pages')
  await until(async () => (await kit.getByTestId('layout-page-2').count()) === 1)
  const spreadOf = async (id) => (await details()).spreads.findIndex((spread) => spread.items.some((item) => item.id === id))
  const beforePages = await history()
  await drag('layout-page-0', 'layout-page-2', [0.9, 0.5])
  step('the first page dragged after the last moves there', await until(async () => (await spreadOf(7)) === 2), String(await spreadOf(7)))
  step('… in one step', (await history()) === beforePages + 1)
  await drag('layout-parent-B-Alt', 'layout-page-1', [0.5, 0.5])
  step('a parent dragged onto a page applies to it', await until(async () => (await details()).spreads[1].pages[0].parent === 'B-Alt'), (await details()).spreads[1].pages[0].parent)
  const viewOf = async () => kit.getByTestId('layout-workspace').getAttribute('data-view')
  await kit.getByTestId('layout-spread-0').click()
  await page.waitForTimeout(400)
  const viewBefore = await viewOf()
  await kit.getByTestId('layout-page-2').dblclick()
  step('a double-click on a page shows it', await until(async () => (await viewOf()) !== viewBefore))
  await kit.getByTestId('layout-pages-menu').click()
  step('the panel menu has the page rows', await until(async () => (await kit.getByTestId('layout-context-page').count()) === 1))
  await kit.getByTestId('layout-context-row-layout.pages.insert').click()
  step('… Insert Pages opens its dialog', await until(async () => (await kit.getByTestId('layout-dialog').count()) > 0))
  await kit.getByTestId('layout-dialog-cancel').click()
  await until(async () => (await kit.getByTestId('layout-dialog').count()) === 0)

  const parentItems = async () => (await details()).parents.find((parent) => parent.label === 'A-Parent')?.items ?? -1
  const onParent = await parentItems()
  await kit.getByTestId('layout-parent-A-Parent').dblclick()
  step('a double-click on a parent edits it', await until(async () => (await kit.getByTestId('layout-editing-parents').count()) === 1))
  step('… the panel lists the parent\'s page', await until(async () => ((await kit.getByTestId('layout-page-0').textContent()) ?? '').startsWith('A-Parent')), await kit.getByTestId('layout-page-0').textContent())
  await page.waitForTimeout(400)
  await drawRectangle()
  step('an object drawn there goes on the parent', await until(async () => (await parentItems()) === onParent + 1), `${onParent} → ${await parentItems()}`)
  await deselect()
  await tab('pages')
  await kit.getByTestId('layout-parents-done').click()
  step('Back to Pages shows the pages again', await until(async () => (await kit.getByTestId('layout-editing-parents').count()) === 0 && (await kit.getByTestId('layout-page-0').textContent()) === '1'))

  // ---- Effects: live, Cancel, OK as one step. ----
  // The rectangle moved with the first page: show its spread so Layers lists it.
  await kit.getByTestId(`layout-spread-${await spreadOf(11)}`).click()
  await select(11)
  await tab('properties')
  await kit.getByTestId('layout-effects-button').click()
  step('Effects opens with the object\'s values', await until(async () => (await kit.getByTestId('layout-effects-dialog').count()) === 1) && (await valueOf('layout-effects-opacity')) === '100')
  const beforeEffects = await history()
  await fill('layout-effects-opacity', '50')
  step('the opacity shows at once', await until(async () => (await itemOf(11))?.opacity === 0.5))
  await kit.getByTestId('layout-effects-page-dropShadow').click()
  await fill('layout-effects-dropShadow-distance', '12')
  step('a drop shadow shows at once (its settings turn it on)', await until(async () => (await itemOf(11))?.effects?.dropShadow?.on === true && (await itemOf(11))?.effects?.dropShadow?.distance === 12 && (await itemOf(11))?.opacity === 0.5))
  step('… with no history step', (await history()) === beforeEffects)
  await kit.getByTestId('layout-effects-cancel').click()
  step('Cancel takes it all back', await until(async () => (await itemOf(11))?.opacity === 1 && (await itemOf(11))?.effects?.dropShadow?.on === false))
  step('… and leaves no step', (await history()) === beforeEffects && !(await details()).canRedo)
  await kit.getByTestId('layout-effects-button').click()
  await until(async () => (await kit.getByTestId('layout-effects-dialog').count()) === 1)
  await kit.getByTestId('layout-effects-blend').selectOption('Multiply')
  await fill('layout-effects-opacity', '40')
  await kit.getByTestId('layout-effects-on-dropShadow').check()
  await until(async () => (await itemOf(11))?.effects?.dropShadow?.on === true)
  await kit.getByTestId('layout-effects-ok').click()
  const effected = async () => {
    const item = await itemOf(11)
    return Math.abs((item?.opacity ?? 0) - 0.4) < 1e-6 && item?.blend === 'Multiply' && item?.effects?.dropShadow?.on === true
  }
  step('OK keeps opacity, blend mode and shadow', await until(effected), JSON.stringify({ opacity: (await itemOf(11))?.opacity, blend: (await itemOf(11))?.blend }))
  step('… as one undo step', (await history()) === beforeEffects + 1)
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+z')
  step('Undo takes them all back', await until(async () => (await itemOf(11))?.opacity === 1 && (await itemOf(11))?.blend === 'Normal' && (await itemOf(11))?.effects?.dropShadow?.on === false))

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} threw - ${error?.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
