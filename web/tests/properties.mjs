// The Properties panel and the dock (plan 2026-11-25, stage 4): node tests/properties.mjs [chromium|webkit]
// Opens dev.html in Chinese and works as a person does: a shape drawn in the sample goes on its
// content layer (not the empty Notes layer the sample opens on); with a text frame selected the panel sets
// underline, superscript, a left indent typed in millimetres, space before, the paragraph's
// alignment, bullets, the language and the text colour, and each reads back from the engine; the
// font menu finds a family by name, shows it in its own face, sets it, and loads a font file; on a
// frame it sets the stroke's type, cap and arrowhead, a CMYK colour from the colour picker (kept as
// CMYK) and adds it to the swatches; text wrap with an offset typed in millimetres; three objects
// aligned left; X swaps fill and stroke and D restores the defaults; the reference point moves X;
// with nothing selected the document's margins; the status bar shows the pointer, the selection's
// size and the story's words and overset; Alt-click on a style's New opens its options; and at
// 1024 × 700 every tab of the dock shows with room for its panel, no group over another.
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
const itemOf = async (id) => {
  const search = (items) => items.flatMap((item) => [item, ...search(item.children ?? [])])
  return search((await details()).spreads.flatMap((spread) => spread.items)).find((item) => item.id === id)
}
/** The resolved text attributes of a frame's story. */
const attrsOf = async (id) => (await agent([{ command: 'select.set', params: { ids: [id] } }, { command: 'text.selectionAttrs', params: {} }])).completed[1]?.result
/** Selects objects as a person does: the first from its row in Layers, the others Shift-clicked. */
const select = async (...ids) => {
  const list = kit.getByTestId('layout-layer-list')
  for (const [index, id] of ids.entries()) {
    const row = kit.getByTestId(`layout-object-name-${id}`)
    for (let top = 0; !(await row.count()); top += 200) {
      const past = await list.evaluate((element, at) => {
        element.scrollTop = at
        return element.scrollTop < at
      }, top)
      await page.waitForTimeout(50)
      if (past) break
    }
    await row.click({ modifiers: index ? ['Shift'] : [] })
  }
  await until(async () => JSON.stringify([...(await details()).selection.items].sort()) === JSON.stringify([...ids].sort()))
}
/** Types into a panel field (the input inside the test id) and presses Enter. */
const setField = async (testId, value) => {
  const input = kit.getByTestId(testId).locator('input').first()
  await input.scrollIntoViewIfNeeded()
  await input.fill(String(value))
  await input.press('Enter')
}
const close = (a, b, within = 0.05) => Math.abs(a - b) <= within

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1000)
  const pageBox = (await details()).spreads[0].pages[0].bounds

  // The sample opens on its content layer: the empty "Notes" layer on top is not the active one.
  const layers = (await details()).layers
  const notes = layers.find((layer) => layer.name === 'Notes')
  const content = layers.find((layer) => layer.name !== 'Notes')
  await kit.getByTestId('layout-tool-rectangle').click()
  const region = await kit.getByTestId('kit-canvas-surface').boundingBox()
  await page.mouse.move(region.x + region.width * 0.3, region.y + region.height * 0.86)
  await page.mouse.down()
  await page.mouse.move(region.x + region.width * 0.4, region.y + region.height * 0.92, { steps: 4 })
  await page.mouse.up()
  await until(async () => (await details()).selection.items.length === 1)
  const drawn = await itemOf((await details()).selection.items[0])
  step('a shape drawn by hand goes on the content layer, not the empty Notes layer the sample opened on', drawn?.layer === content.id, JSON.stringify({ layer: drawn?.layer, notes: notes.id }))
  step('Layers marks the content layer active', await until(async () => (await kit.getByTestId(`layout-layer-${content.id}`).getAttribute('data-active')) === 'true' && (await kit.getByTestId(`layout-layer-${notes.id}`).getAttribute('data-active')) === 'false'))
  // The status bar: where the pointer is, and the selection's size.
  await page.mouse.move(region.x + region.width * 0.5, region.y + region.height * 0.5)
  step('the status bar shows where the pointer is', await until(async () => /X .+ Y /.test((await kit.getByTestId('layout-status-pointer').textContent()) ?? '')), await kit.getByTestId('layout-status-pointer').textContent())
  step('the status bar shows the selection\'s size', await until(async () => /W .+ H /.test((await kit.getByTestId('layout-status-size').textContent()) ?? '')), await kit.getByTestId('layout-status-size').textContent().catch(() => ''))
  await kit.getByTestId('layout-tool-selection').click()

  // Text: a frame with too much text for it.
  const long = 'Aurora light words fill the frame quickly. '.repeat(30)
  const text = await one('frame.add', { rect: [pageBox[0] + 60, 60, pageBox[0] + 300, 160], content: 'text', text: long, caret: false })
  await select(text.id)
  step('a text frame shows Transform, Appearance, Text Frame, Character and Paragraph', await until(async () => {
    for (const id of ['layout-transform', 'layout-appearance', 'layout-text-frame', 'layout-character', 'layout-paragraph']) if (!(await kit.getByTestId(id).count())) return false
    return true
  }))
  const story = kit.getByTestId('layout-status-story')
  step('the status bar counts the story\'s words and overset characters', await until(async () => Number(await story.getAttribute('data-words')) === 7 * 30 && Number(await story.getAttribute('data-overset')) > 0), `${await story.textContent().catch(() => '')}`)
  await kit.getByTestId('layout-char-underline').click()
  step('Underline sets and reads back', await until(async () => (await attrsOf(text.id))?.chars?.underline === true))
  step('the Underline button shows it pressed', await until(async () => (await kit.getByTestId('layout-char-underline').getAttribute('aria-pressed')) === 'true'))
  await kit.getByTestId('layout-char-superscript').click()
  step('Superscript sets and reads back', await until(async () => (await attrsOf(text.id))?.chars?.position === 'superscript'))
  await setField('layout-left-indent', '10mm')
  step('a left indent typed in millimetres reads back in points', await until(async () => close((await attrsOf(text.id))?.para?.leftIndent ?? 0, 28.3465)), String((await attrsOf(text.id))?.para?.leftIndent))
  await setField('layout-space-before', '6')
  step('Space before sets and reads back', await until(async () => (await attrsOf(text.id))?.para?.spaceBefore === 6))
  await kit.getByTestId('layout-align-centerJustified').click()
  step('the paragraph takes Justify with last line centred', await until(async () => (await attrsOf(text.id))?.para?.align === 'centerJustified'))
  await kit.getByTestId('layout-bullets').click()
  step('Bullets turn the paragraphs into a bulleted list', await until(async () => (await attrsOf(text.id))?.para?.listType === 'bullets'))
  await kit.getByTestId('layout-language').selectOption('French')
  step('the language sets and reads back', await until(async () => (await attrsOf(text.id))?.chars?.language === 'French'))
  await kit.getByTestId('layout-text-fill').selectOption('C=100 M=0 Y=0 K=0')
  step('the text colour sets from a swatch', await until(async () => (await attrsOf(text.id))?.chars?.fill === 'C=100 M=0 Y=0 K=0'))
  await kit.getByTestId('layout-frame-columns').locator('input').first().fill('2')
  await kit.getByTestId('layout-frame-columns').locator('input').first().press('Enter')
  step('Text Frame columns set from the panel', await until(async () => (await itemOf(text.id))?.columns === 2))
  await kit.getByTestId('layout-frame-vj-center').click()
  step('vertical justification sets and reads back', await until(async () => (await itemOf(text.id))?.textFrame?.verticalJustification === 'center'))

  // The font menu: search, each family in its own face, a click sets it.
  await kit.getByTestId('layout-font-browse').click()
  await kit.getByTestId('layout-font-search').fill('lor')
  const lora = kit.getByTestId('layout-font-family-Lora')
  step('the font menu finds a family by name, and only matching ones', await until(async () => (await lora.count()) === 1 && (await kit.getByTestId('layout-font-family-Inter').count()) === 0))
  step('the font menu shows a family in its own face', /Lora/.test(await lora.evaluate((element) => getComputedStyle(element).fontFamily)))
  step('the font menu offers the computer\'s fonts where the browser can list them, else says so', which === 'chromium' ? (await kit.getByTestId('layout-font-computer').count()) === 1 : (await kit.locator('.layout-font-menu .layout-note').count()) === 1)
  await lora.click()
  step('a family from the font menu sets the text', await until(async () => (await attrsOf(text.id))?.chars?.fontFamily === 'Lora'))
  // Load Font File…: the host picks a font from the workspace; its family is set.
  await page.evaluate(async (bytes) => window.devHost.files.set('Source Sans 3.ttf', { bytes: new Uint8Array(bytes).buffer, version: 1 }), [...readFileSync(new URL('../../assets/fonts/SourceSans3-Regular.ttf', import.meta.url))])
  await kit.getByTestId('layout-font-browse').click()
  await kit.getByTestId('layout-font-file').click()
  step('Load Font File… gives the engine the font and sets its family', await until(async () => (await attrsOf(text.id))?.chars?.fontFamily === 'Source Sans 3'), String((await attrsOf(text.id))?.chars?.fontFamily))

  // A frame: the stroke's options, the colour picker in CMYK, Add to Swatches.
  const box = await one('frame.add', { rect: [pageBox[0] + 60, 400, pageBox[0] + 160, 500], content: 'graphic' })
  await one('object.stroke', { ids: [box.id], weight: 2, swatch: '[Black]' })
  await select(box.id)
  await kit.getByTestId('layout-stroke-type').selectOption('dashed')
  step('the stroke type sets and reads back', await until(async () => (await itemOf(box.id))?.strokeDetail?.type?.kind === 'dashed'))
  await kit.getByTestId('layout-stroke-cap-round').click()
  step('the stroke cap sets and reads back', await until(async () => (await itemOf(box.id))?.strokeDetail?.cap === 'round'))
  await kit.getByTestId('layout-arrow-end').selectOption('triangle')
  step('the end arrowhead sets and reads back', await until(async () => (await itemOf(box.id))?.strokeDetail?.end === 'triangle'))
  const swatchesBefore = (await details()).swatchColors.length
  await kit.getByTestId('layout-fill-well').click()
  await kit.getByTestId('layout-fill-well-editor-model-cmyk').click()
  for (const [part, value] of [['c', 0], ['m', 60], ['y', 90], ['k', 10]]) {
    const field = kit.getByTestId(`layout-fill-well-editor-${part}`).locator('input').first()
    await field.fill(String(value))
    await field.press('Enter')
    await page.waitForTimeout(150)
  }
  const cmykOf = async () => (await details()).colors?.[(await itemOf(box.id))?.fill]?.cmyk
  step('the colour picker sets a CMYK fill, kept as CMYK', await until(async () => JSON.stringify(await cmykOf()) === JSON.stringify([0, 60, 90, 10])), JSON.stringify(await cmykOf()))
  await kit.getByTestId('layout-fill-well-add').click()
  step('Add to Swatches makes the colour a swatch', await until(async () => (await details()).swatchColors.length === swatchesBefore + 1))
  await page.keyboard.press('Escape')

  // Text wrap with an offset typed in millimetres.
  await kit.getByTestId('layout-wrap-boundingBox').click()
  step('Text Wrap › bounding box from the panel', await until(async () => (await itemOf(box.id))?.wrap?.mode === 'boundingBox'))
  await setField('layout-wrap-top', '2mm')
  step('a wrap offset typed in millimetres', await until(async () => close((await itemOf(box.id))?.wrap?.offsets?.[0] ?? 0, 5.6693)))

  // X swaps fill and stroke; D gives no fill and a black stroke.
  await one('object.fill', { ids: [box.id], swatch: 'C=100 M=0 Y=0 K=0' })
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('x')
  step('X swaps the fill and the stroke', await until(async () => (await itemOf(box.id))?.fill === '[Black]' && (await itemOf(box.id))?.stroke.swatch === 'C=100 M=0 Y=0 K=0'), JSON.stringify([(await itemOf(box.id))?.fill, (await itemOf(box.id))?.stroke]))
  await page.keyboard.press('d')
  step('D gives the default fill and stroke', await until(async () => (await itemOf(box.id))?.fill === '[None]' && (await itemOf(box.id))?.stroke.swatch === '[Black]'))

  // The reference point: X reads at the centre once it is chosen.
  const xBefore = Number.parseFloat(await kit.getByTestId('layout-x').locator('input').inputValue())
  await kit.getByTestId('layout-ref-4').click()
  const width = (await one('transform.info', { ids: [box.id] })).width
  step('the reference point moves where X reads', await until(async () => close(Number.parseFloat(await kit.getByTestId('layout-x').locator('input').inputValue()), xBefore + width / 2, 0.02)))
  await kit.getByTestId('layout-ref-0').click()

  // Align: three objects to their left edges.
  const three = []
  for (const [index, x] of [100, 180, 240].entries()) three.push((await one('frame.add', { rect: [pageBox[0] + x, 560 + index * 50, pageBox[0] + x + 40, 590 + index * 50], content: 'graphic' })).id)
  await select(...three)
  await kit.getByTestId('layout-align-edge-left').scrollIntoViewIfNeeded()
  await kit.getByTestId('layout-align-edge-left').click()
  step('Align left edges lines up three objects', await until(async () => new Set(await Promise.all(three.map(async (id) => Math.round((await itemOf(id)).bounds[0] * 100)))).size === 1))

  // Nothing selected: the document's margins.
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('Escape')
  await until(async () => (await details()).selection.items.length === 0)
  await setField('layout-doc-margin-top', '20mm')
  step('with nothing selected, the panel sets the margins', await until(async () => close((await details()).spreads[0].pages[0].margins?.top ?? 0, 56.6929)), JSON.stringify((await details()).spreads[0].pages[0].margins))

  // Alt-click on New Paragraph Style opens its options; OK makes the style with its name.
  await kit.getByTestId('layout-dock-tab-styles').click()
  await kit.getByTestId('layout-paragraph-style-new').click({ modifiers: ['Alt'] })
  step('Alt-click on New opens the new style\'s options', await until(async () => (await kit.getByTestId('layout-style-dialog').count()) === 1))
  await kit.getByTestId('layout-style-name').fill('Lead')
  await kit.getByTestId('layout-style-ok').click()
  step('its OK makes the style', await until(async () => (await details()).paragraphStyles.includes('Lead')))

  // A narrow window: every tab shows, each with room for its panel, no group over another.
  await page.setViewportSize({ width: 1024, height: 700 })
  await page.waitForTimeout(500)
  const tabs = ['properties', 'pages', 'layers', 'swatches', 'links', 'styles']
  let fits = true
  const sizes = []
  for (const tab of tabs) {
    await kit.getByTestId(`layout-dock-tab-${tab}`).click()
    await page.waitForTimeout(150)
    const groups = await kit.locator('.kui-dock-group').evaluateAll((list) => list.map((group) => {
      const box = group.getBoundingClientRect()
      const body = group.querySelector('.kui-dock-body')?.getBoundingClientRect()
      return { top: box.top, bottom: box.bottom, body: body ? body.height : 0, tab: group.querySelector('[aria-selected="true"]')?.getAttribute('data-testid') }
    }))
    const shown = groups.find((group) => group.tab === `layout-dock-tab-${tab}`)
    sizes.push(`${tab}:${Math.round(shown?.body ?? 0)}`)
    const overlapping = groups.some((group, index) => index > 0 && group.top < groups[index - 1].bottom - 1)
    if (!shown || shown.body < 60 || overlapping || groups.at(-1).bottom > 700) fits = false
  }
  step('at 1024 × 700 every tab shows with room for its panel and no group over another', fits, sizes.join(' '))
  await kit.getByTestId('layout-dock-tab-properties').click()
  await select(text.id)
  await setField('layout-space-after', '4')
  step('at 1024 × 700 a field low in Properties scrolls into view and sets', await until(async () => (await attrsOf(text.id))?.para?.spaceAfter === 4))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
