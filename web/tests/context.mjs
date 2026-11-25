// Right-click menus, the system clipboard, dropped files and the place cursor (plan 2026-11-25,
// stage 2): node tests/context.mjs [chromium|webkit]
// Opens dev.html in Chinese and checks, as a person works: a right-click on an object selects it
// and its menu's Rotate 90° turns it; on the pasteboard the menu sets 100% and hides the rulers;
// on a ruler, a page in Pages (Insert Pages… asks first), a layer and the text being typed each
// open their own menu; Cmd+V pastes the system clipboard's text into the text and, out of text,
// as a new text frame, and its image as a placed image; Cmd+C copies typed text to the system
// clipboard (both in Chromium: Playwright's WebKit grants no clipboard access); an image dropped
// on an empty frame fills it, elsewhere it lands where it was dropped; Place… loads the place
// cursor and a click puts the image down; the development host's picker offers the computer's files.
import './platform.mjs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
const clipboard = which === 'chromium'
if (clipboard) await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin })
context.setDefaultTimeout(15000)
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}
const skip = (name, why) => console.log(`SKIP ${which} ${name} - ${why}`)
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
const storyText = async (id) => (await one('story.get', { story: id }))?.text
const view = async () => {
  const [zoom, x, y] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  return { zoom, x, y }
}
/** A spread point (points) on the screen. */
const toScreen = async (x, y, spread = 0) => {
  const now = await details()
  const placed = now.canvas.spreads[spread]
  const left = Math.min(...now.spreads[spread].pages.map((p) => p.bounds[0]))
  const v = await view()
  const box = await kit.getByTestId('kit-canvas-surface').boundingBox()
  return { x: box.x + (placed.x + x - left - v.x) * v.zoom, y: box.y + (placed.y + y - v.y) * v.zoom }
}
const escape = async () => {
  await page.keyboard.press('Escape')
  await page.waitForTimeout(150)
}
const deselect = async () => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('Escape')
  await until(async () => (await details()).selection.items.length === 0)
}
/** A small red PNG, as base64. */
const png = () =>
  page.evaluate(async () => {
    const canvas = Object.assign(document.createElement('canvas'), { width: 16, height: 12 })
    const context2d = canvas.getContext('2d')
    context2d.fillStyle = '#ff0000'
    context2d.fillRect(0, 0, 16, 12)
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
    const bytes = new Uint8Array(await blob.arrayBuffer())
    return btoa(String.fromCharCode(...bytes))
  })

try {
  await page.goto(`${origin}/dev.html?locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  const box = (await details()).spreads[0].pages[0].bounds

  // Out of text, Cmd+V makes a text frame of text copied elsewhere (nothing copied here yet).
  if (clipboard) {
    await page.evaluate(() => navigator.clipboard.writeText('来自别处的文字'))
    await kit.locator('.kui-app').focus()
    const before = (await details()).stories.length
    await page.keyboard.press('ControlOrMeta+v')
    step('Cmd+V out of text makes a text frame of the system clipboard\'s text', await until(async () => (await details()).stories.some((story) => story.preview.includes('来自别处的文字'))), `${before} → ${(await details()).stories.length} stories`)
  } else skip('Cmd+V of text copied elsewhere', 'no clipboard access')

  // A right-click on an object selects it; its menu's Rotate 90° turns it.
  await deselect()
  const shape = await one('frame.create', { rect: [box[0] + 60, 420, box[0] + 260, 480], content: 'graphic' })
  const onShape = await toScreen(box[0] + 160, 450)
  await page.mouse.click(onShape.x, onShape.y, { button: 'right' })
  const objectMenu = kit.getByTestId('layout-context-object')
  step('a right-click on an object opens its menu', await until(() => objectMenu.isVisible()))
  step('and selects the object', await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([shape.id])), JSON.stringify((await details()).selection.items))
  step('the object menu is in Chinese', (await objectMenu.textContent()).includes('剪切'), (await objectMenu.textContent()).slice(0, 60))
  await kit.getByTestId('layout-context-row-transform.rotate|cw90').click()
  const turned = async () => {
    const [x0, y0, x1, y1] = (await itemOf(shape.id)).bounds
    return Math.abs(x1 - x0 - 60) < 0.5 && Math.abs(y1 - y0 - 200) < 0.5
  }
  step('its Rotate 90° turns the object', await until(turned), JSON.stringify((await itemOf(shape.id)).bounds))

  // On the pasteboard: the view and the rulers.
  await deselect()
  const region = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const blank = { x: region.x + 8, y: region.y + 8 }
  await page.mouse.click(blank.x, blank.y, { button: 'right' })
  step('a right-click on the pasteboard opens its menu', await until(() => kit.getByTestId('layout-context-canvas').isVisible()))
  await kit.getByTestId('layout-context-row-kit:view.actualSize').click()
  step('its Actual Size sets 100%', await until(async () => Math.abs((await view()).zoom - 1) < 1e-6), JSON.stringify(await view()))
  await page.mouse.click(blank.x, blank.y, { button: 'right' })
  await kit.getByTestId('layout-context-row-kit:view.rulers').click()
  step('its Hide Rulers hides the rulers', await until(async () => (await kit.locator('.layout-ruler').count()) === 0))
  await page.mouse.click(blank.x, blank.y, { button: 'right' })
  await kit.getByTestId('layout-context-row-kit:view.rulers').click()
  step('and Show Rulers brings them back', await until(async () => (await kit.locator('.layout-ruler').count()) === 2))

  // A ruler, a page in Pages, a layer: each has its menu.
  const ruler = await kit.getByTestId('layout-ruler-x').boundingBox()
  await page.mouse.click(ruler.x + 100, ruler.y + 6, { button: 'right' })
  step('a right-click on a ruler opens its menu', await until(() => kit.getByTestId('layout-context-ruler').isVisible()))
  await escape()
  await kit.getByTestId('layout-page-0').click({ button: 'right' })
  step('a right-click on a page in Pages opens the page menu', await until(() => kit.getByTestId('layout-context-page').isVisible()))
  await kit.getByTestId('layout-context-row-layout.pages.insert').click()
  step('its Insert Pages… asks first', await until(() => kit.getByTestId('layout-dialog').isVisible()))
  await kit.getByTestId('layout-dialog-cancel').click()
  const layer = (await details()).layers[0].id
  await kit.getByTestId(`layout-layer-${layer}`).click({ button: 'right', position: { x: 120, y: 10 } })
  step('a right-click on a layer opens the layer menu', await until(() => kit.getByTestId('layout-context-layer').isVisible()))
  await escape()

  // In the text being typed: the text menu; Cmd+C and Cmd+V with the system clipboard.
  const frame = await one('frame.create', { rect: [box[0] + 60, 520, box[0] + 400, 600], content: 'text', text: 'Kition 排版', caret: false })
  const story = (await details()).stories.find((item) => item.frames.includes(frame.id)).id
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('t')
  await until(async () => (await details()).tool === 'type')
  const inText = await toScreen(box[0] + 200, 540)
  await page.mouse.click(inText.x, inText.y)
  await until(async () => (await details()).selection.text != null)
  await page.mouse.click(inText.x, inText.y, { button: 'right' })
  const textMenu = kit.getByTestId('layout-context-text')
  step('a right-click in the text being typed opens the text menu', await until(() => textMenu.isVisible()))
  step('the text menu copies and pastes', (await kit.getByTestId('layout-context-row-edit.copy').count()) === 1 && (await kit.getByTestId('layout-context-row-edit.paste').count()) === 1)
  await escape()
  if (clipboard) {
    await page.keyboard.press('ControlOrMeta+a')
    await page.waitForTimeout(200)
    await page.keyboard.press('ControlOrMeta+c')
    step('Cmd+C puts the typed text on the system clipboard', await until(async () => (await page.evaluate(() => navigator.clipboard.readText())) === 'Kition 排版'), await page.evaluate(() => navigator.clipboard.readText()))
    await page.keyboard.press('End')
    await page.evaluate(() => navigator.clipboard.writeText('，外部'))
    await page.keyboard.press('ControlOrMeta+v')
    step('Cmd+V in text pastes the system clipboard\'s text', await until(async () => (await storyText(story)) === 'Kition 排版，外部'), await storyText(story))
  } else skip('Cmd+C and Cmd+V in text with the system clipboard', 'no clipboard access')
  await escape()
  await escape()
  await page.keyboard.press('v')
  await until(async () => (await details()).tool === 'selection')

  // An image on the system clipboard is placed.
  if (clipboard) {
    await deselect()
    const data = await png()
    await page.evaluate(async (b64) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': new Blob([bytes], { type: 'image/png' }) })])
    }, data)
    const images = (await items()).filter((item) => item.asset != null).length
    await kit.locator('.kui-app').focus()
    await page.keyboard.press('ControlOrMeta+v')
    step('Cmd+V places an image from the system clipboard', await until(async () => (await items()).filter((item) => item.asset != null).length === images + 1))
  } else skip('Cmd+V of an image', 'no clipboard access')

  // Dropped images: into the empty frame under them, else where they were dropped.
  const frameBox = await page.locator('#kit').boundingBox()
  const drop = async (at) => {
    const kitFrame = await (await page.$('#kit')).contentFrame()
    const data = await png()
    await kitFrame.evaluate(({ x, y, b64 }) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
      const transfer = new DataTransfer()
      transfer.items.add(new File([bytes], 'dropped.png', { type: 'image/png' }))
      const target = document.elementFromPoint(x, y)
      for (const type of ['dragenter', 'dragover', 'drop']) target.dispatchEvent(new DragEvent(type, { dataTransfer: transfer, clientX: x, clientY: y, bubbles: true, cancelable: true }))
    }, { x: at.x - frameBox.x, y: at.y - frameBox.y, b64: data })
  }
  await deselect()
  const empty = await one('frame.create', { rect: [box[0] + 300, 400, box[0] + 400, 480], content: 'graphic' })
  await drop(await toScreen(box[0] + 350, 440))
  step('an image dropped on an empty frame fills it', await until(async () => (await itemOf(empty.id))?.asset != null), JSON.stringify(await itemOf(empty.id)))
  const count = (await items()).length
  await drop(await toScreen(box[0] + 420, 100))
  step('an image dropped elsewhere lands where it was dropped', await until(async () => (await items()).length === count + 1))
  const landed = (await items()).find((item) => item.asset != null && Math.abs(item.bounds[0] - (box[0] + 420)) < 2 && Math.abs(item.bounds[1] - 100) < 2)
  step('at the drop point', landed != null, JSON.stringify((await items()).filter((item) => item.asset != null).map((item) => item.bounds)))

  // Place… loads the place cursor; a click puts the image down.
  await page.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
    window.devHost.files.set('photo.png', { bytes: bytes.buffer, version: 1 })
    window.devHost.scriptHostPick('photo.png')
  }, await png())
  await deselect()
  const before = (await items()).length
  await page.keyboard.press('ControlOrMeta+d')
  step('Place… loads the place cursor', await until(async () => (await details()).tool === 'placeGun'), (await details()).tool)
  const spot = await toScreen(box[0] + 100, 120)
  await page.mouse.click(spot.x, spot.y)
  step('a click puts the image down there', await until(async () => (await items()).length === before + 1 && (await details()).tool === 'selection'), `${(await items()).length} objects, tool ${(await details()).tool}`)

  step('no page errors', errors.length === 0, errors.join('; '))

  // Standalone, a person picks the file to place from the computer.
  const person = await context.newPage()
  await person.goto(`${origin}/dev.html?locale=en-US&pick=chooser`)
  await person.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await person.waitForTimeout(800)
  await person.frameLocator('#kit').locator('.kui-app').focus()
  await person.keyboard.press('ControlOrMeta+d')
  const chooser = person.locator('[data-dev-host-chooser]')
  step('the development host\'s Place… offers the computer\'s files', await until(() => chooser.isVisible()) && (await chooser.getByRole('button', { name: 'Choose from computer…' }).count()) === 1)
  await person.close()
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
