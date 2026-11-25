// The keyboard and the view (plan 2026-11-25, stage 2): node tests/keys.mjs [chromium|webkit]
// Opens dev.html and checks, as a person types: a tool key works before any click (the editor has
// the keyboard when it opens); an arrow nudges the selected object by the keyboard increment,
// Shift+arrow ten times as far, each one undo step; after Enter in a panel's field the next arrow
// nudges again; Alt+arrow nudges a copy, Delete deletes;
// typing, Cmd+Left goes to the line's start; one Undo takes back a word, not a letter; Cmd+1 is
// 100%, Cmd+0 fits the spread (not the pasteboard), Shift+Page Down shows the next page; a wheel
// notch with Cmd zooms by at most 1.2 times; the Zoom tool's drag zooms into its area; a double
// click on the Hand tool fits the spread and on the Zoom tool sets 100%; the status bar's zoom
// takes a typed percentage.
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
const history = async () => one('document.history')
const itemOf = async (id) => {
  const search = (items) => items.flatMap((item) => [item, ...search(item.children ?? [])])
  return search((await details()).spreads.flatMap((spread) => spread.items)).find((item) => item.id === id)
}
const itemCount = async () => (await details()).spreads.flatMap((spread) => spread.items).length
const storyText = async (id) => (await one('story.get', { story: id }))?.text
/** Selects an object as a person does, from its row in Layers. */
const select = async (id) => {
  await kit.getByTestId(`layout-object-name-${id}`).click()
  await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([id]))
}
const view = async () => {
  const [zoom, x, y] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  return { zoom, x, y }
}
/** The view once it has stopped moving. */
const settled = async () => {
  let before = await view()
  for (let attempt = 0; attempt < 30; attempt += 1) {
    await page.waitForTimeout(100)
    const now = await view()
    if (now.zoom === before.zoom && now.x === before.x && now.y === before.y) return now
    before = now
  }
  return before
}
const press = async (keys) => {
  await page.keyboard.press(keys)
  await page.waitForTimeout(150)
}

try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)

  // The editor has the keyboard when it opens: T picks the Type tool with no click first.
  await page.keyboard.press('t')
  step('a tool key works before any click', await until(async () => (await details()).tool === 'type'), (await details()).tool)
  await page.keyboard.press('Escape')
  await page.keyboard.press('v')
  await until(async () => (await details()).tool === 'selection')

  const box = (await details()).spreads[0].pages[0].bounds
  const shape = await one('frame.create', { rect: [box[0] + 60, 400, box[0] + 160, 450], content: 'graphic' })
  await select(shape.id)
  const at = async () => (await itemOf(shape.id)).bounds
  const start = await at()
  const before = await history()
  await press('ArrowRight')
  await until(async () => (await at())[0] !== start[0])
  const one1 = (await at())[0] - start[0]
  step('an arrow nudges the selection by the keyboard increment', one1 > 0 && (await at())[1] === start[1], `${one1}`)
  await press('Shift+ArrowDown')
  await until(async () => (await at())[1] !== start[1])
  step('Shift+arrow nudges ten times as far', Math.abs((await at())[1] - start[1] - one1 * 10) < 1e-6, `${(await at())[1] - start[1]}`)
  const after = await history()
  step('each nudge is one undo step', after.undo.length === before.undo.length + 2, JSON.stringify(after.undo.slice(-3)))
  // Enter in a panel's number field gives the keys back to the canvas: the next arrow nudges.
  const xField = kit.getByTestId('layout-x').locator('input').first()
  const shown = Number.parseFloat(await xField.inputValue())
  const beforeField = (await at())[0]
  await xField.fill(String(shown + 5))
  await xField.press('Enter')
  await until(async () => Math.abs((await at())[0] - beforeField - 5) < 1e-6)
  await press('ArrowRight')
  step('after Enter in a panel field, an arrow nudges again', await until(async () => Math.abs((await at())[0] - beforeField - 5 - one1) < 1e-6), `${(await at())[0] - beforeField}`)
  const count = await itemCount()
  await press('Alt+ArrowLeft')
  step('Alt+arrow nudges a copy', await until(async () => (await itemCount()) === count + 1), `${await itemCount()} objects`)
  await press('Delete')
  step('Delete deletes the selection', await until(async () => (await itemCount()) === count), `${await itemCount()} objects`)

  // Typing: Cmd+Left goes to the line's start; one Undo takes back a word.
  const frame = await one('frame.create', { rect: [box[0] + 60, 480, box[0] + 400, 560], content: 'text', text: '', caret: false })
  const story = (await details()).stories.find((item) => item.frames.includes(frame.id)).id
  const placed = (await details()).canvas?.spreads?.[0]
  const surface = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const v = await view()
  const left = Math.min(...(await details()).spreads[0].pages.map((p) => p.bounds[0]))
  const toScreen = (x, y) => ({ x: surface.x + ((placed?.x ?? 0) + x - left - v.x) * v.zoom, y: surface.y + ((placed?.y ?? 0) + y - v.y) * v.zoom })
  await page.keyboard.press('t')
  await until(async () => (await details()).tool === 'type')
  const inFrame = toScreen(box[0] + 100, 500)
  await page.mouse.click(inFrame.x, inFrame.y)
  await until(async () => (await details()).selection.text != null)
  await page.keyboard.type('one two', { delay: 30 })
  step('typing reaches the frame', await until(async () => (await storyText(story)) === 'one two'), await storyText(story))
  await press('ControlOrMeta+ArrowLeft')
  await page.keyboard.type('X')
  step('Cmd+Left moves the caret to the line\'s start', await until(async () => (await storyText(story)) === 'Xone two'), await storyText(story))
  // A pause ends the word being typed; one Undo takes back the last step (the X), the next a word.
  await page.waitForTimeout(1300)
  await press('End')
  await page.keyboard.type(' three', { delay: 30 })
  await until(async () => (await storyText(story)) === 'Xone two three')
  await page.waitForTimeout(1300)
  await press('ControlOrMeta+z')
  step('one Undo takes back a word, not a letter', await until(async () => (await storyText(story)) === 'Xone two '), await storyText(story))
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  await page.keyboard.press('v')
  await until(async () => (await details()).tool === 'selection')

  // The view's keys.
  await kit.locator('.kui-app').focus()
  await press('ControlOrMeta+1')
  step('Cmd+1 is 100%', await until(async () => Math.abs((await settled()).zoom - 1) < 1e-6), JSON.stringify(await view()))
  const spread0 = (await details()).spreads[0]
  const placedSpread = (await details()).canvas.spreads[0]
  await press('ControlOrMeta+0')
  const fitted = await settled()
  const fitZoom = Math.min(surface.width / placedSpread.width, surface.height / placedSpread.height)
  step('Cmd+0 fits the spread, not the pasteboard', Math.abs(fitted.zoom - fitZoom) / fitZoom < 0.15, `zoom ${fitted.zoom}, spread fit ${fitZoom}, ${spread0.pages.length} pages`)
  await press('Shift+PageDown')
  const next = await settled()
  step('Shift+Page Down shows the next page', next.y > fitted.y || next.x > fitted.x, `${JSON.stringify(fitted)} → ${JSON.stringify(next)}`)
  await press('ControlOrMeta+Shift+PageUp')
  const first = await settled()
  step('Cmd+Shift+Page Up goes back to the first page', first.y < next.y || first.x < next.x, JSON.stringify(first))

  // The wheel: one notch with Cmd zooms about 1.2 times, not 2.7.
  const middle = { x: surface.x + surface.width / 2, y: surface.y + surface.height / 2 }
  await page.mouse.move(middle.x, middle.y)
  const wheelFrom = await settled()
  await page.keyboard.down('ControlOrMeta')
  await page.mouse.wheel(0, -100)
  await page.keyboard.up('ControlOrMeta')
  const wheeled = await settled()
  const ratio = wheeled.zoom / wheelFrom.zoom
  step('a wheel notch with Cmd zooms by at most 1.2 times', ratio > 1.01 && ratio < 1.21, `${ratio}`)

  // The Zoom tool's drag zooms into the dragged area.
  await page.keyboard.press('z')
  await page.waitForTimeout(200)
  const zoomFrom = await settled()
  await page.mouse.move(middle.x - 60, middle.y - 40)
  await page.mouse.down()
  await page.mouse.move(middle.x, middle.y, { steps: 4 })
  await page.mouse.move(middle.x + 60, middle.y + 40, { steps: 4 })
  await page.mouse.up()
  const zoomed = await settled()
  step('the Zoom tool\'s drag zooms into its area', zoomed.zoom > zoomFrom.zoom * 2, `${zoomFrom.zoom} → ${zoomed.zoom}`)
  await kit.locator('[data-kit-ui-id="tool:zoom"]').first().dblclick()
  step('a double click on the Zoom tool sets 100%', await until(async () => Math.abs((await settled()).zoom - 1) < 1e-6), JSON.stringify(await view()))
  await kit.locator('[data-kit-ui-id="tool:hand"]').first().dblclick()
  const handFit = await settled()
  step('a double click on the Hand tool fits the spread', Math.abs(handFit.zoom - fitted.zoom) / fitted.zoom < 0.05, `${handFit.zoom} vs ${fitted.zoom}`)

  // The status bar's zoom takes a typed percentage.
  await kit.getByTestId('kit-zoom').click()
  const entry = kit.getByTestId('layout-zoom-entry')
  await entry.fill('150')
  await entry.press('Enter')
  step('the status bar takes a typed zoom', await until(async () => Math.abs((await settled()).zoom - 1.5) < 1e-6), JSON.stringify(await view()))

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
