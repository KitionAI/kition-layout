// The editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html in Chinese, and prints PASS or FAIL per step.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await page.waitForTimeout(1000)
  step('Chinese menu names', (await kit.getByTestId('layout-menu-Layout').textContent()) === '版面')
  step('status counts the pages', (await kit.getByTestId('layout-status').textContent()) === '2 页')
  await kit.getByTestId('layout-add-page').click()
  await until(async () => (await details()).pageCount === 3)
  step('adds a page', (await details()).pageCount === 3)

  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await kit.getByTestId('layout-spread-0').click()
  await page.waitForTimeout(400)
  await page.mouse.click(region.x + region.width * 0.5, region.y + region.height * 0.72)
  await until(async () => (await details()).selection.items.length === 1)
  step('selects a frame on the canvas', (await details()).selection.items.length === 1)
  await kit.getByText('色板').click()
  await kit.getByTestId('layout-swatch-C=0 M=100 Y=100 K=0').click()
  await page.waitForTimeout(400)
  const now = await details()
  const item = now.spreads[0].items.find((entry) => entry.id === now.selection.items[0])
  step('fills it from the Swatches panel', JSON.stringify(item ?? {}).includes('M=100 Y=100'))

  await kit.getByTestId('layout-tool-type').click()
  await page.mouse.click(region.x + region.width * 0.5, region.y + region.height * 0.42)
  await page.waitForTimeout(300)
  if (which === 'chromium') {
    const cdp = await page.context().newCDPSession(page)
    await cdp.send('Input.imeSetComposition', { text: 'ji', selectionStart: 2, selectionEnd: 2 })
    await cdp.send('Input.insertText', { text: '极光' })
  } else {
    await page.keyboard.insertText('极光')
  }
  const typed = () => page.evaluate(async () => {
    const stories = (await window.devHost.connection().ai.inspect()).details.stories
    const result = await window.devHost.connection().ai.run(stories.map((story) => ({ command: 'story.get', params: { story: story.id } })), false)
    return JSON.stringify(result.completed.map((item) => item.result.text))
  })
  await until(async () => (await typed()).includes('极光'))
  step('types Chinese into a frame', (await typed()).includes('极光'))
  await page.keyboard.press('Escape')

  await kit.getByTestId('layout-new-layer').click()
  await until(async () => (await details()).layers.length === 3)
  step('adds a layer', (await details()).layers.length === 3)
  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'layout.pages.insert', params: { count: 1 } }], false))
  await page.waitForTimeout(300)
  step('the agent edits the live layout', run.completed[0].changed && (await page.evaluate(() => window.devHost.changes.at(-1).by)) === 'agent')
  // Threading by hand (W3-22 DE5): click the out port of an overset frame, then an empty frame.
  const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
  const before = await details()
  const pageBox = before.spreads[0].pages[0].bounds
  const long = 'Words keep coming and the frame fills up quickly with them. '.repeat(14)
  const made = await agent([
    { command: 'frame.create', params: { rect: [pageBox[0] + 60, 60, pageBox[0] + 260, 160], content: 'text', text: long, caret: false } },
    { command: 'frame.create', params: { rect: [pageBox[0] + 60, 200, pageBox[0] + 260, 420], content: 'unassigned' } },
  ])
  const [source, target] = made.completed.map((item) => item.result.id)
  await agent([{ command: 'selection.set', params: { ids: [source] } }])
  await kit.getByTestId('layout-spread-0').click()
  await page.waitForTimeout(400)
  // Document pixels to the page: the spread's place on the canvas, then the canvas view.
  const toScreen = async (x, y) => {
    const now = await details()
    const placed = (await inspect()).details.canvas.spreads[0]
    const left = Math.min(...now.spreads[0].pages.map((p) => p.bounds[0]))
    const [zoom, vx, vy] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
    const box = await kit.getByTestId('layout-canvas-region').boundingBox()
    return { x: box.x + (placed.x + x - left - vx) * zoom, y: box.y + (placed.y + y - vy) * zoom, zoom }
  }
  const storyOf = async (frame) => (await details()).stories.find((story) => story.frames.includes(frame))
  step('the frame oversets before threading', (await storyOf(source))?.overset === true)
  const probe = await toScreen(0, 0)
  const port = await toScreen(pageBox[0] + 260, 160 - 12 / probe.zoom)
  await page.mouse.click(port.x, port.y)
  await until(async () => (await details()).tool === 'loadedText')
  step('the out port loads the text cursor', (await details()).tool === 'loadedText')
  const into = await toScreen(pageBox[0] + 160, 310)
  await page.mouse.click(into.x, into.y)
  await until(async () => (await storyOf(source))?.frames.length === 2)
  step('a click threads the empty frame', JSON.stringify((await storyOf(source))?.frames) === JSON.stringify([source, target]) && (await details()).tool === 'selection')
  await page.keyboard.press('ControlOrMeta+z')
  await until(async () => (await storyOf(source))?.frames.length === 1)
  step('undo takes the thread back', (await storyOf(source))?.frames.length === 1)
  // A shortcut pressed while the editor still settles the last command is not taken: settle first.
  await page.waitForTimeout(500)
  await page.keyboard.press('ControlOrMeta+Shift+z')
  await until(async () => (await storyOf(source))?.frames.length === 2)
  step('redo brings it back', (await storyOf(source))?.frames.length === 2)
  const threaded = await agent([{ command: 'frame.unthread', params: { frame: source } }])
  step('the agent breaks the thread', threaded.completed[0].result.unthreaded === true && (await storyOf(source))?.frames.length === 1)

  // The package's fonts (W3-22 DE15): the four OFL families Kition's designs use draw from the package.
  const families = ['Inter', 'Lora', 'JetBrains Mono', 'Bricolage Grotesque']
  const fontFrames = await agent(families.map((family, index) => ({ command: 'frame.create', params: { rect: [pageBox[0] + 300, 60 + index * 40, pageBox[0] + 520, 90 + index * 40], content: 'text', text: `${family} Bold 123`, caret: false } })))
  await agent(fontFrames.completed.flatMap((item, index) => [
    { command: 'selection.set', params: { ids: [item.result.id] } },
    { command: 'type.char', params: { attrs: { fontFamily: families[index], fontStyle: 'Bold' } } },
  ]))
  const used = (await agent([{ command: 'font.list', params: {} }])).completed[0].result
  const served = families.filter((family) => used.some((font) => font.family === family && !font.missing && !font.styleMissing))
  step('the package serves Inter, Lora, JetBrains Mono and Bricolage Grotesque', served.length === families.length, JSON.stringify(used.filter((font) => families.includes(font.family))))

  // The panels (W3-22 DE11a).
  const itemOf = async (id) => {
    const search = (items) => items.flatMap((item) => [item, ...search(item.children ?? [])])
    return search((await details()).spreads.flatMap((spread) => spread.items)).find((item) => item.id === id)
  }
  const setField = async (testId, value) => {
    const input = kit.getByTestId(testId).locator('input').first()
    await input.fill(String(value))
    await input.press('Enter')
  }
  const box = (await agent([{ command: 'frame.create', params: { rect: [pageBox[0] + 60, 460, pageBox[0] + 160, 560], content: 'graphic' } }])).completed[0].result.id
  await agent([{ command: 'selection.set', params: { ids: [box] } }])
  await until(async () => (await kit.getByTestId('layout-geometry').getAttribute('data-item').catch(() => null)) === String(box))
  await setField('layout-w', 150)
  await until(async () => Math.abs((await itemOf(box)).bounds[2] - (await itemOf(box)).bounds[0] - 150) < 0.01)
  const sized = await itemOf(box)
  step('Properties sets the width', Math.abs(sized.bounds[2] - sized.bounds[0] - 150) < 0.01, JSON.stringify(sized.bounds))
  await kit.getByTestId('layout-fill').selectOption('C=100 M=0 Y=0 K=0')
  await until(async () => (await itemOf(box)).fill === 'C=100 M=0 Y=0 K=0')
  step('Properties sets the fill', (await itemOf(box)).fill === 'C=100 M=0 Y=0 K=0')
  await setField('layout-corner', 12)
  await until(async () => (await itemOf(box)).corner?.size === 12)
  step('Properties rounds the corners', (await itemOf(box)).corner?.size === 12 && (await itemOf(box)).corner?.shape === 'rounded')
  await kit.getByTestId('layout-flip-h').click()
  const mirrored = async () => {
    const info = (await agent([{ command: 'transform.info', params: { ids: [box] } }])).completed[0].result
    return info.scaleX * info.scaleY < 0
  }
  await until(mirrored)
  step('Properties flips', await mirrored())

  await agent([{ command: 'selection.set', params: { ids: [fontFrames.completed[0].result.id] } }])
  await until(async () => (await kit.getByTestId('layout-font').count()) > 0)
  await kit.getByTestId('layout-font').selectOption('Lora')
  const fontAt = async () => (await agent([{ command: 'type.selectionAttrs', params: {} }])).completed[0].result?.chars?.fontFamily
  await until(async () => (await fontAt()) === 'Lora')
  step('Properties sets the font of a text frame', (await fontAt()) === 'Lora')
  await until(async () => (await kit.getByTestId('layout-font-style').locator('option').allTextContents()).includes('SemiBold'))
  await kit.getByTestId('layout-font-style').selectOption('SemiBold')
  const styleAt = async () => (await agent([{ command: 'type.selectionAttrs', params: {} }])).completed[0].result?.chars?.fontStyle
  await until(async () => (await styleAt()) === 'SemiBold')
  step('Properties sets the weight from the family\'s styles', (await styleAt()) === 'SemiBold')

  // Layers: thousands of objects, only the rows in view drawn.
  await agent([{ command: 'frame.grid', params: { rect: [pageBox[0] + 20, 600, pageBox[0] + 400, 760], cols: 50, rows: 40, gutter: 1, content: 'graphic' } }])
  await until(async () => Number(await kit.getByTestId('layout-layer-list').getAttribute('data-rows')) > 2000)
  const rows = Number(await kit.getByTestId('layout-layer-list').getAttribute('data-rows'))
  const drawn = await kit.getByTestId('layout-layer-list').locator('.layout-row').count()
  step('Layers lists 2,000 objects and draws only those in view', rows > 2000 && drawn < 60, `${rows} rows, ${drawn} drawn`)
  await kit.getByTestId(`layout-object-name-${box}`).scrollIntoViewIfNeeded().catch(() => undefined)
  const boxRow = kit.getByTestId(`layout-object-name-${box}`)
  if (!(await boxRow.count())) {
    // Scroll the list to the box's row.
    await kit.getByTestId('layout-layer-list').evaluate((list) => { list.scrollTop = list.scrollHeight })
    await page.waitForTimeout(200)
  }
  await kit.getByTestId(`layout-object-name-${box}`).click()
  await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([box]))
  step('Layers selects an object', JSON.stringify((await details()).selection.items) === JSON.stringify([box]))
  await kit.getByTestId(`layout-object-visibility-${box}`).click()
  await until(async () => (await itemOf(box)).hidden === true)
  step('Layers hides an object', (await itemOf(box)).hidden === true)
  // Undo the person's hide in the person's history, then the agent's grid in its own (KitModel M12a).
  await agent([{ command: 'edit.undo', params: { scope: 'person' } }, { command: 'edit.undo', params: {} }])

  // Swatches: a new swatch from a hex value, applied to the selection.
  await kit.getByTestId('layout-new-swatch').click()
  await kit.getByTestId('layout-new-swatch-name').fill('Brand')
  await kit.getByTestId('layout-new-swatch-color').fill('#5645d4')
  await kit.getByTestId('layout-new-swatch-ok').click()
  await until(async () => (await details()).swatches.includes('Brand'))
  const brand = (await inspect()).details.swatchColors?.find((swatch) => swatch.name === 'Brand')
  step('Swatches makes a new swatch', brand?.color === '#5645d4', JSON.stringify(brand))
  await agent([{ command: 'selection.set', params: { ids: [box] } }])
  await kit.getByTestId('layout-swatch-Brand').click()
  await until(async () => (await itemOf(box)).fill === 'Brand')
  step('Swatches fills the selection', (await itemOf(box)).fill === 'Brand')

  // Paragraph styles: new from the selected text, edited in its dialog.
  await agent([{ command: 'selection.set', params: { ids: [fontFrames.completed[1].result.id] } }])
  await kit.getByTestId('layout-paragraph-styles').getByText('段落样式', { exact: true }).first().click()
  const before2 = (await details()).paragraphStyles.length
  await kit.getByTestId('layout-paragraph-style-new').click()
  await until(async () => (await details()).paragraphStyles.length === before2 + 1)
  const made2 = (await details()).paragraphStyles.at(-1)
  step('Styles makes a paragraph style from the selection', (await details()).paragraphStyles.length === before2 + 1, made2)
  await kit.getByTestId(`layout-paragraph-style-${made2}`).click()
  await kit.getByTestId('layout-paragraph-style-edit').click()
  await kit.getByTestId('layout-style-size').fill('30')
  await kit.getByTestId('layout-style-ok').click()
  const sizeAt = async () => (await agent([{ command: 'type.selectionAttrs', params: {} }])).completed[0].result?.chars?.size
  await until(async () => (await sizeAt()) === 30)
  step('Styles edits the style, and its text follows', (await sizeAt()) === 30)

  // Links: place an image the host picks into an empty frame.
  await page.evaluate(async () => {
    const canvas = Object.assign(document.createElement('canvas'), { width: 8, height: 8 })
    const context = canvas.getContext('2d')
    context.fillStyle = '#ff0000'
    context.fillRect(0, 0, 8, 8)
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
    window.devHost.files.set('photo.png', { bytes: await blob.arrayBuffer(), version: 1 })
  })
  const empty = (await agent([{ command: 'frame.create', params: { rect: [pageBox[0] + 300, 460, pageBox[0] + 400, 560], content: 'graphic' } }])).completed[0].result.id
  await agent([{ command: 'selection.set', params: { ids: [empty] } }])
  await kit.getByText('链接', { exact: true }).click()
  await kit.getByTestId('layout-place-image').click()
  await until(async () => (await itemOf(empty))?.asset != null)
  step('Links places a picked image into the frame', (await itemOf(empty))?.asset != null)
  await until(async () => (await kit.locator('[data-testid^="layout-link-go-"]').count()) > 0)
  step('Links lists it', (await kit.locator('[data-testid^="layout-link-go-"]').count()) > 0)

  // Document Setup: a design size preset.
  await kit.getByTestId('layout-document-setup').click()
  await kit.getByTestId('layout-preset-poster').click()
  await kit.getByTestId('layout-setup-ok').click()
  await until(async () => (await details()).settings.pageWidth === 1080)
  const settings = (await details()).settings
  step('Document Setup applies the Poster size', settings.pageWidth === 1080 && settings.pageHeight === 1440 && settings.intent === 'web', JSON.stringify(settings))

  // Rulers: they count from the page in view, at the canvas zoom.
  await kit.getByTestId('layout-spread-0').click()
  await page.waitForTimeout(400)
  const ruler = kit.getByTestId('layout-ruler-x')
  const rulerBox = await ruler.boundingBox()
  const pageLeft = await toScreen((await details()).spreads[0].pages[0].bounds[0], 0)
  const rulerAt = Number(await ruler.getAttribute('data-origin'))
  step('the ruler counts from the page', Math.abs(rulerBox.x + rulerAt - pageLeft.x) < 1.5, `${rulerBox.x + rulerAt} vs ${pageLeft.x}`)
  const guides = async () => (await agent([{ command: 'guide.list', params: { spread: 0 } }])).completed[0].result.length
  const guidesBefore = await guides()
  await page.mouse.move(rulerBox.x + rulerBox.width / 2, rulerBox.y + rulerBox.height / 2)
  await page.mouse.down()
  await page.mouse.move(rulerBox.x + rulerBox.width / 2, rulerBox.y + 200, { steps: 4 })
  await page.mouse.up()
  await until(async () => (await guides()) === guidesBefore + 1)
  step('dragging from the ruler makes a guide', (await guides()) === guidesBefore + 1)

  // Export: PNG with a transparent background, then PDF, written next to the document.
  const exported = (name) => page.evaluate((file) => window.devHost.files.get(file)?.bytes?.byteLength ?? 0, name)
  await kit.getByTestId('layout-menu-File').click()
  await kit.getByTestId('layout-menu-export').click()
  await kit.getByTestId('layout-export-format').selectOption('png')
  await kit.getByTestId('layout-export-range').selectOption('current')
  await kit.getByTestId('layout-export-transparent').check()
  await kit.getByTestId('layout-export-ok').click()
  await until(async () => (await exported('Sample.png')) > 0)
  const pngHead = await page.evaluate(() => Array.from(new Uint8Array(window.devHost.files.get('Sample.png')?.bytes ?? new ArrayBuffer(0)).slice(0, 4)))
  step('exports the page as PNG', JSON.stringify(pngHead) === JSON.stringify([137, 80, 78, 71]))
  await kit.getByTestId('layout-menu-File').click()
  await kit.getByTestId('layout-menu-export').click()
  step('the export dialog remembers its choices', (await kit.getByTestId('layout-export-transparent').isChecked()) === true)
  await kit.getByTestId('layout-export-format').selectOption('pdf')
  await kit.getByTestId('layout-export-range').selectOption('all')
  await kit.getByTestId('layout-export-ok').click()
  await until(async () => (await exported('Sample.pdf')) > 0)
  const pdfHead = await page.evaluate(() => new TextDecoder().decode(new Uint8Array(window.devHost.files.get('Sample.pdf')?.bytes ?? new ArrayBuffer(0)).slice(0, 5)))
  step('exports PDF', pdfHead === '%PDF-')

  // Coverage of Design Studio's remaining rows (W3-22): autoflow, text threads, fit text,
  // character style delete, templates, page background, sizes and "export all sizes".
  const pagesBefore = (await details()).pageCount
  const flowFrom = (await agent([{ command: 'frame.create', params: { rect: [pageBox[0] + 60, 60, pageBox[0] + 200, 120], content: 'text', text: long.repeat(6), caret: false } }])).completed[0].result.id
  await agent([{ command: 'frame.loadText', params: { frame: flowFrom } }])
  await until(async () => (await details()).tool === 'loadedText')
  await kit.getByTestId('layout-spread-0').click()
  await page.waitForTimeout(400)
  const empty2 = await toScreen((await details()).spreads[0].pages[0].bounds[0] + 300, 640)
  await page.keyboard.down('Shift')
  await page.mouse.click(empty2.x, empty2.y)
  await page.keyboard.up('Shift')
  await until(async () => (await storyOf(flowFrom))?.overset === false)
  step('Shift-click autoflows until the story fits', (await storyOf(flowFrom))?.overset === false && (await details()).pageCount > pagesBefore, `${(await storyOf(flowFrom))?.frames.length} frames`)
  const threadsOn = (await agent([{ command: 'view.showTextThreads', params: { on: true } }])).completed[0].result
  step('View › Show Text Threads turns on', threadsOn?.on === true)
  await agent([{ command: 'view.showTextThreads', params: { on: false } }])

  await agent([{ command: 'style.character.create', params: { name: 'Doomed', chars: { fontStyle: 'Italic' } } }])
  await until(async () => (await details()).characterStyles.includes('Doomed'))
  await agent([{ command: 'edit.deselectAll', params: {} }])
  await kit.getByTestId('layout-character-styles').getByText('字符样式', { exact: true }).first().click()
  await kit.getByTestId('layout-character-style-Doomed').click()
  await kit.getByTestId('layout-character-style-delete').click()
  await until(async () => !(await details()).characterStyles.includes('Doomed'))
  step('Character Styles deletes a style', !(await details()).characterStyles.includes('Doomed'))

  // Templates: File › New from Template puts Kition's Editorial layout in the document.
  await kit.getByTestId('layout-menu-File').click()
  await kit.getByTestId('layout-menu-templates').click()
  await until(async () => (await kit.locator('[data-testid^="layout-template-"]').count()) === 4)
  step('New from Template lists the four templates', (await kit.locator('[data-testid^="layout-template-"]').count()) === 4)
  await kit.getByTestId('layout-template-editorial').click()
  await until(async () => (await details()).settings.pageWidth === 1080 && (await details()).pageCount === 1)
  const templated = await details()
  const names = templated.spreads[0].items.map((entry) => entry.name)
  step('the template makes the Editorial layout', names.includes('Headline') && templated.swatches.includes('Primary') && templated.spreads[0].pages[0].background === 'Surface', JSON.stringify(names))

  // Page background: with nothing selected, Properties sets it (none = transparent).
  await agent([{ command: 'edit.deselectAll', params: {} }])
  await until(async () => (await kit.getByTestId('layout-background').count()) > 0)
  await kit.getByTestId('layout-background').selectOption('')
  await until(async () => (await details()).spreads[0].pages[0].background == null)
  step('Properties clears the page background', (await details()).spreads[0].pages[0].background == null)
  await kit.getByTestId('layout-background').selectOption('Accent')
  await until(async () => (await details()).spreads[0].pages[0].background === 'Accent')
  step('Properties sets the page background from a swatch', (await details()).spreads[0].pages[0].background === 'Accent')

  // Fit text to frame.
  const headline = templated.spreads[0].items.find((entry) => entry.name === 'Headline')
  await agent([{ command: 'selection.set', params: { ids: [headline.id] } }])
  const sizeOf = async () => (await agent([{ command: 'type.selectionAttrs', params: {} }])).completed[0].result?.chars?.size
  const sizeBefore = await sizeOf()
  await until(async () => (await kit.getByTestId('layout-fit-text').count()) > 0)
  await kit.getByTestId('layout-fit-text').click()
  await until(async () => (await sizeOf()) !== sizeBefore)
  step('Fit Text to Frame sets the largest size that fits', (await sizeOf()) !== sizeBefore && (await storyOf(headline.id))?.overset === false, `${sizeBefore} → ${await sizeOf()}`)

  // Sizes: a Square and a Story variant of the poster, then every size exported as PNG.
  await kit.getByTestId('layout-new-size').click()
  await kit.getByTestId('layout-size-preset-square').click()
  await kit.getByTestId('layout-size-ok').click()
  await until(async () => (await details()).layouts?.length === 2)
  await kit.getByTestId('layout-new-size').click()
  await kit.getByTestId('layout-size-preset-story').click()
  await kit.getByTestId('layout-size-ok').click()
  await until(async () => (await details()).layouts?.length === 3)
  const layouts = (await details()).layouts ?? []
  step('Sizes adds alternate layouts', JSON.stringify(layouts.map((entry) => [entry.name, entry.height])) === JSON.stringify([[null, 1440], ['方形', 1080], ['快拍', 1920]]), JSON.stringify(layouts))
  const squareBackground = (await details()).spreads[1]?.pages[0]?.background
  step('each size has its own background', squareBackground === 'Accent', String(squareBackground))
  await kit.getByTestId('layout-menu-File').click()
  await kit.getByTestId('layout-menu-export').click()
  await kit.getByTestId('layout-export-format').selectOption('png')
  await kit.getByTestId('layout-export-range').selectOption('sizes')
  await kit.getByTestId('layout-export-ok').click()
  await until(async () => (await exported('Sample-快拍.png')) > 0)
  step('exports every size as PNG', (await exported('Sample-方形.png')) > 0 && (await exported('Sample-快拍.png')) > 0)
  await kit.getByTestId('layout-size-方形').click()
  await kit.getByTestId('layout-delete-size').click()
  await until(async () => (await details()).layouts?.length === 2)
  step('Sizes deletes a size', (await details()).layouts?.length === 2 && (await details()).pageCount === 2)

  await kit.getByTestId('layout-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.klayout').version)) > 1)
  step('saves through the host', (await page.evaluate(() => window.devHost.files.get('Sample.klayout').version)) > 1)
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
