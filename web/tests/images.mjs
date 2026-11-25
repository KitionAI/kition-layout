// Images and objects by hand (plan 2026-11-25, stage 6): node tests/images.mjs [chromium|webkit]
// Opens dev.html and checks, as a person works: over a frame with a placed image the Selection
// tool shows the hand on its content grabber, and a drag there moves the image inside the frame in
// one undo step; the Properties panel then measures the image (its X and its scale, which it sets);
// Escape goes back to the frame and a double click goes to the image and back; a double click on
// the frame's side handle fits the frame to the image across; the fitting keys (Cmd+Alt+Shift+C)
// change the image's scale; the Eyedropper puts the image's colour on a selected object and a
// text's formatting on selected text; X swaps fill and stroke in one undo step; several images
// dropped at once load the place cursor, which shows how many, the arrows choose the next and
// Escape drops one; a click on one of two selected objects makes it the key object; the Layers
// panel's right-click duplicates a layer; Links unembeds an embedded image.
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
const items = async () => (await details()).spreads.flatMap((spread) => spread.items)
const itemOf = async (id) => (await items()).find((item) => item.id === id)
const history = async () => (await one('file.history'))?.undo?.length ?? -1
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
const near = (a, b, tolerance = 0.6) => Math.abs(a - b) <= tolerance
const graphic = async (id) => one('transform.info', { ids: [id], content: true })
const selected = async () => {
  const d = await details()
  return { items: d.selection.items ?? [], content: Boolean(d.selectionContent) }
}
const tool = async (id) => {
  if ((await details()).tool === id) return true
  await kit.locator(`[data-kit-ui-id="tool:${id}"]`).first().click()
  return until(async () => (await details()).tool === id)
}
const view = async () => {
  const [zoom, x, y] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  return { zoom, x, y }
}
/** A spread point (points) on the screen. */
const toScreen = async (x, y) => {
  const now = await details()
  const placed = now.canvas.spreads[0]
  const left = Math.min(...now.spreads[0].pages.map((p) => p.bounds[0]))
  const v = await view()
  const box = await kit.getByTestId('kit-canvas-surface').boundingBox()
  return { x: box.x + (placed.x + x - left - v.x) * v.zoom, y: box.y + (placed.y + y - v.y) * v.zoom }
}
const cursor = () => kit.getByTestId('kit-canvas-surface').evaluate((element) => element.style.cursor)
/** A PNG `width` × `height` pixels, its left half red and its right half blue, as base64. */
const halves = (width, height) =>
  page.evaluate(
    async ([w, h]) => {
      const canvas = Object.assign(document.createElement('canvas'), { width: w, height: h })
      const context2d = canvas.getContext('2d')
      context2d.fillStyle = '#ff0000'
      context2d.fillRect(0, 0, w / 2, h)
      context2d.fillStyle = '#0000ff'
      context2d.fillRect(w / 2, 0, w / 2, h)
      const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
      const bytes = new Uint8Array(await blob.arrayBuffer())
      return btoa(String.fromCharCode(...bytes))
    },
    [width, height],
  )

try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+1')
  await page.waitForTimeout(400)
  const box = (await details()).spreads[0].pages[0].bounds
  const x0 = box[0] + 60
  const y0 = box[1] + 80

  // An image 200 × 100 pt in a frame half as wide: its right half (blue) is cropped.
  const placed = await one('file.place', { base64: await halves(200, 100), name: 'halves.png', x: x0, y: y0, width: 200 })
  const img = placed.id
  await one('transform.resize', { ids: [img], from: [x0, y0, x0 + 200, y0 + 100], to: [x0, y0, x0 + 100, y0 + 100] })
  await tool('selection')
  const inFrame = await toScreen(x0 + 20, y0 + 20)
  await page.mouse.click(inFrame.x, inFrame.y)
  await until(async () => same((await selected()).items, [img]))

  // The content grabber: the hand over it, a drag moves the image in the frame, one undo step.
  const middle = await toScreen(x0 + 50, y0 + 50)
  await page.mouse.move(middle.x - 3, middle.y)
  await page.mouse.move(middle.x, middle.y)
  step('the content grabber shows the hand', await until(async () => (await cursor()).includes('V6.5')), (await cursor()).slice(0, 60))
  const startX = (await graphic(img)).x
  const steps = await history()
  await page.mouse.down()
  await page.mouse.move(middle.x + 10 * (await view()).zoom, middle.y, { steps: 4 })
  await page.mouse.move(middle.x + 20 * (await view()).zoom, middle.y, { steps: 4 })
  await page.mouse.up()
  step('a drag on the grabber selects the image, not the frame', await until(async () => (await selected()).content), JSON.stringify(await selected()))
  step('and moves it inside the frame', await until(async () => near((await graphic(img)).x, startX + 20)), `${startX} → ${(await graphic(img)).x}`)
  const frame = await itemOf(img)
  step('the frame stays', near(frame.bounds[0], x0) && near(frame.bounds[2], x0 + 100), JSON.stringify(frame.bounds))
  step('the move is one undo step', (await history()) === steps + 1, `${steps} → ${await history()}`)

  // Properties measures the image: its X, and its scale, which it sets.
  const geometry = kit.getByTestId('layout-geometry')
  step('Properties measures the image', await until(async () => (await geometry.getAttribute('data-content')) === 'true'))
  const scaleField = kit.getByTestId('layout-scale-x').locator('input')
  await scaleField.fill('50')
  await scaleField.press('Enter')
  step('Properties sets the image\'s scale', await until(async () => near((await graphic(img)).scaleX, 50)), JSON.stringify(await graphic(img)))
  step('the frame keeps its size', same((await itemOf(img)).bounds, frame.bounds), JSON.stringify((await itemOf(img)).bounds))

  // Escape goes back to the frame; a double click to the image and back.
  await kit.locator('.kui-app').focus()
  await page.mouse.move(inFrame.x, inFrame.y)
  await page.keyboard.press('Escape')
  step('Escape goes back to the frame', await until(async () => same(await selected(), { items: [img], content: false })), JSON.stringify(await selected()))
  await page.mouse.dblclick(inFrame.x, inFrame.y)
  step('a double click selects the image', await until(async () => same(await selected(), { items: [img], content: true })), JSON.stringify(await selected()))
  await page.waitForTimeout(600)
  await page.mouse.dblclick(inFrame.x, inFrame.y)
  step('another double click selects the frame', await until(async () => same(await selected(), { items: [img], content: false })), JSON.stringify(await selected()))
  await page.waitForTimeout(600)

  // A double click on the frame's right handle fits it to the image across, not down.
  const g = await graphic(img)
  const right = await toScreen(x0 + 100, y0 + 50)
  await page.mouse.dblclick(right.x, right.y)
  const fitted = async () => (await itemOf(img)).bounds
  step('a double click on a side handle fits the frame across', await until(async () => near((await fitted())[2] - (await fitted())[0], g.width) && near((await fitted())[3] - (await fitted())[1], 100)), JSON.stringify(await fitted()))
  await page.waitForTimeout(600)

  // The fitting keys change the image's scale (the walkthrough saw them do nothing).
  await one('transform.resize', { ids: [img], from: await fitted(), to: [x0, y0, x0 + 100, y0 + 100] })
  await page.mouse.click(inFrame.x, inFrame.y)
  await until(async () => same(await selected(), { items: [img], content: false }))
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+Alt+Shift+KeyC')
  step('Fill Frame Proportionally (Cmd+Alt+Shift+C) scales the image to fill the frame', await until(async () => near((await graphic(img)).scaleX, 100)), JSON.stringify(await graphic(img)))
  await page.keyboard.press('ControlOrMeta+Alt+Shift+KeyE')
  step('Fit Content Proportionally (Cmd+Alt+Shift+E) fits it inside', await until(async () => near((await graphic(img)).scaleX, 50)), JSON.stringify(await graphic(img)))

  // The Eyedropper: the image's colour on a selected object.
  const shape = (await one('frame.add', { rect: [x0 + 300, y0, x0 + 350, y0 + 50], content: 'unassigned' })).id
  const onShape = await toScreen(x0 + 325, y0 + 25)
  await page.mouse.click(onShape.x, onShape.y)
  await until(async () => same((await selected()).items, [shape]))
  await tool('eyedropper')
  const red = await toScreen(x0 + 20, y0 + 50)
  await page.mouse.click(red.x, red.y)
  const fillHex = async () => {
    const d = await details()
    const item = d.spreads.flatMap((spread) => spread.items).find((entry) => entry.id === shape)
    return d.colors?.[item?.fill]?.hex ?? null
  }
  step('the Eyedropper puts the image\'s colour on the selected object', await until(async () => (await fillHex())?.toLowerCase() === '#ff0000'), String(await fillHex()))

  // The Eyedropper: a text's formatting on the selected text.
  const source = await one('frame.add', { rect: [x0, y0 + 150, x0 + 260, y0 + 200], content: 'text', text: 'Source', caret: false })
  await agent([
    { command: 'text.select', params: { story: source.story, anchor: 0, focus: 6 } },
    { command: 'text.char', params: { attrs: { size: 30, underline: true } } },
  ])
  const target = await one('frame.add', { rect: [x0, y0 + 220, x0 + 260, y0 + 260], content: 'text', text: 'Target', caret: false })
  await tool('type')
  const inTarget = await toScreen(x0 + 20, y0 + 230)
  await page.mouse.click(inTarget.x, inTarget.y, { clickCount: 5 })
  await until(async () => (await details()).selection.text?.story === target.story)
  await tool('eyedropper')
  const inSource = await toScreen(x0 + 10, y0 + 170)
  await page.mouse.click(inSource.x, inSource.y)
  const targetAttrs = async () => (await agent([{ command: 'text.select', params: { story: target.story, anchor: 0, focus: 6 } }, { command: 'text.selectionAttrs', params: {} }])).completed[1]?.result
  step('the Eyedropper puts a text\'s formatting on the selected text', await until(async () => (await targetAttrs())?.chars?.size === 30 && (await targetAttrs())?.chars?.underline === true), JSON.stringify((await targetAttrs())?.chars ?? {}).slice(0, 120))

  // X swaps fill and stroke in one undo step.
  await tool('selection')
  await page.mouse.click(onShape.x, onShape.y)
  await until(async () => same((await selected()).items, [shape]))
  const before = await itemOf(shape)
  const xSteps = await history()
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('x')
  step('X swaps fill and stroke', await until(async () => (await itemOf(shape)).fill === before.stroke?.swatch), JSON.stringify({ before: [before.fill, before.stroke?.swatch], after: [(await itemOf(shape)).fill, (await itemOf(shape)).stroke?.swatch] }))
  step('in one undo step', (await history()) === xSteps + 1, `${xSteps} → ${await history()}`)

  // A click on one of two selected objects makes it the key object; again, none.
  await page.keyboard.down('Shift')
  await page.mouse.click(inFrame.x, inFrame.y)
  await page.keyboard.up('Shift')
  step('Shift-click adds the frame', await until(async () => (await selected()).items.length === 2), JSON.stringify(await selected()))
  await page.waitForTimeout(600)
  await page.mouse.click(onShape.x, onShape.y)
  step('a click on one of the selected makes it the key object', await until(async () => (await details()).keyObject === shape), String((await details()).keyObject))
  await page.waitForTimeout(600)
  await page.mouse.click(onShape.x, onShape.y)
  step('again: no key object', await until(async () => (await details()).keyObject == null), String((await details()).keyObject))

  // Several images dropped at once load the place cursor: how many, the arrows, Escape.
  const frameBox = await page.locator('#kit').boundingBox()
  const dropAt = await toScreen(x0 + 400, y0 + 300)
  const kitFrame = await (await page.$('#kit')).contentFrame()
  await kitFrame.evaluate(
    ({ x, y, files }) => {
      const transfer = new DataTransfer()
      for (const [name, b64] of files) transfer.items.add(new File([Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))], name, { type: 'image/png' }))
      const target = document.elementFromPoint(x, y)
      for (const type of ['dragenter', 'dragover', 'drop']) target.dispatchEvent(new DragEvent(type, { dataTransfer: transfer, clientX: x, clientY: y, bubbles: true, cancelable: true }))
    },
    { x: dropAt.x - frameBox.x, y: dropAt.y - frameBox.y, files: [['one.png', await halves(20, 10)], ['two.png', await halves(30, 10)], ['three.png', await halves(40, 10)]] },
  )
  step('several dropped images load the place cursor', await until(async () => (await details()).tool === 'placeGun' && (await details()).placeQueue?.length === 3), JSON.stringify((await details()).placeQueue?.map((q) => q.name)))
  const badge = kit.getByTestId('layout-place-badge')
  await page.mouse.move(dropAt.x - 2, dropAt.y)
  await page.mouse.move(dropAt.x, dropAt.y)
  step('the cursor shows how many', await until(async () => (await badge.getAttribute('data-count')) === '3' && (await badge.isVisible())))
  step('and a picture of the next', await until(async () => (await badge.locator('img').count()) === 1))
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ArrowRight')
  step('the arrow brings the next', await until(async () => (await badge.getAttribute('data-next')) === 'two.png'), await badge.getAttribute('data-next'))
  await page.keyboard.press('Escape')
  step('Escape drops it', await until(async () => (await details()).placeQueue?.length === 2 && (await details()).tool === 'placeGun'), JSON.stringify((await details()).placeQueue?.map((q) => q.name)))
  const count = (await items()).length
  await page.mouse.click(dropAt.x, dropAt.y)
  step('a click places the next and keeps the cursor loaded', await until(async () => (await items()).length === count + 1 && (await details()).placeQueue?.length === 1 && (await details()).tool === 'placeGun'))
  await page.keyboard.press('Escape')
  step('the last Escape gives the Selection tool back', await until(async () => (await details()).tool === 'selection'), (await details()).tool)

  // The Layers panel duplicates a layer.
  const layers = (await details()).layers.length
  const layer = (await details()).activeLayer
  await kit.getByTestId(`layout-layer-${layer}`).click({ button: 'right', position: { x: 120, y: 10 } })
  await kit.getByTestId('layout-context-row-layer.duplicate').click()
  step('the layer menu duplicates a layer', await until(async () => (await details()).layers.length === layers + 1), `${layers} → ${(await details()).layers.length}`)
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+z')
  await until(async () => (await details()).layers.length === layers)

  // An agent reorders by index (the Layers panel's drag, stage 7).
  const order = async () => (await details()).spreads[0].items.map((item) => item.id)
  await one('arrange.reorder', { ids: [shape], index: 0 })
  const front = await order()
  step('arrange.reorder moves an object to a place in its layer', front.indexOf(shape) > front.indexOf(img), JSON.stringify(front.slice(-4)))

  // Links unembeds an embedded image: kept in a file beside the document and linked.
  await kit.getByText('Links', { exact: true }).click()
  const unembed = kit.getByTestId(`layout-link-unembed-${placed.asset}`)
  step('Links offers Unembed for the embedded image', await until(() => unembed.isVisible()))
  await unembed.click()
  const status = () => kit.getByTestId(`layout-link-${placed.asset}`).getAttribute('data-status')
  step('Unembed links it to a file', await until(async () => (await status()) !== 'embedded'), String(await status()))

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
