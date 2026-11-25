// The view options, ruler guides and units (plan 2026-11-25, stage 3): node tests/view.mjs [chromium|webkit]
// Opens dev.html and checks, as a person works: a guide dragged out of a ruler shows its line and
// where it is while it moves, lands where it was dropped and is drawn on the page; a press on it
// selects it, an arrow moves it, a drag moves it, Delete or a drop on the ruler deletes it; Cmd+;
// hides the guides and W (Preview) hides the guides and the frame edges; a shape drawn near a guide
// snaps to it, and not once snapping is off; a drawn shape is as tall as the drag (no hidden
// baseline grid catching it); the ruler's menu sets millimetres, and the rulers, the panel and
// Document Setup then read an A4 page as 210 × 297; Preferences sets the ruler unit; the view
// options outlast a reload and never touch the document.
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
const inspect = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
const one = async (command, params = {}) => (await agent([{ command, params }])).completed[0]?.result
const guides = async () => (await one('guide.list', { spread: 0 })) ?? []
const itemOf = async (id) => {
  const search = (items) => items.flatMap((item) => [item, ...search(item.children ?? [])])
  return search((await inspect()).spreads.flatMap((spread) => spread.items)).find((item) => item.id === id)
}
const press = async (keys) => {
  await page.keyboard.press(keys)
  await page.waitForTimeout(200)
}
/** A spread point (spread 0, points) on the screen. */
const toScreen = async (x, y) => {
  const now = await inspect()
  const placed = now.canvas.spreads[0]
  const left = Math.min(...now.spreads[0].pages.map((p) => p.bounds[0]))
  const [zoom, vx, vy] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  const box = await kit.getByTestId('kit-canvas-surface').boundingBox()
  return { x: box.x + (placed.x + x - left - vx) * zoom, y: box.y + (placed.y + y - vy) * zoom, zoom }
}
/** The canvas's pixel under a screen point: the bluest of a little area across it, or (`darkest`) the darkest. */
const pixel = async (at, darkest = false) =>
  kit.getByTestId('kit-canvas-surface').evaluate((canvas, [point, dark]) => {
    const box = canvas.getBoundingClientRect()
    const scale = canvas.width / box.width
    const frame = window.frameElement?.getBoundingClientRect() ?? { left: 0, top: 0 }
    const x = Math.round((point.x - frame.left - box.left) * scale)
    const y = Math.round((point.y - frame.top - box.top) * scale)
    // The brightest-blue pixel of a little area across the line (a 1-pixel line can fall between).
    const { data } = canvas.getContext('2d').getImageData(x - 2, y - 2, 5, 5)
    let best = [255, 255, 255]
    for (let at = 0; at < data.length; at += 4) {
      const better = dark ? data[at] + data[at + 1] + data[at + 2] < best[0] + best[1] + best[2] : data[at + 2] - data[at] > best[2] - best[0]
      if (better) best = [data[at], data[at + 1], data[at + 2]]
    }
    return best
  }, [at, darkest])
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
  await until(async () => JSON.stringify((await inspect()).selection.items) === JSON.stringify([id]))
}
/** A ruler guide's cyan (or a selected guide's layer blue): blue well above red. */
const guideBlue = (rgb) => rgb[2] - rgb[0] > 80

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await press('ControlOrMeta+1')
  const pageBounds = (await inspect()).spreads[0].pages[0].bounds
  await kit.getByTestId('layout-page-0').click().catch(() => undefined)
  await page.waitForTimeout(400)

  // A guide out of the top ruler: its line and readout while dragging, then on the page.
  const before = (await guides()).length
  const ruler = await kit.getByTestId('layout-ruler-x').boundingBox()
  const target = await toScreen(pageBounds[0] + 200, 250)
  await page.mouse.move(target.x, ruler.y + ruler.height / 2)
  await page.mouse.down()
  await page.mouse.move(target.x, target.y - 20, { steps: 4 })
  await page.mouse.move(target.x, target.y, { steps: 2 })
  const readout = await kit.getByTestId('layout-guide-readout').textContent().catch(() => '')
  step('dragging from the ruler shows the guide and where it is', (await kit.getByTestId('layout-guide-drag').isVisible()) && /^Y: \d/.test(readout ?? ''), readout)
  await page.mouse.up()
  await until(async () => (await guides()).length === before + 1)
  const added = (await guides()).at(-1)
  step('the guide lands where it was dropped', (await guides()).length === before + 1 && Math.abs(added.position - 250) <= 2 / target.zoom + 4, JSON.stringify(added))
  const onGuide = await toScreen(pageBounds[0] + 120, added.position)
  step('the guide is drawn on the page', await until(async () => guideBlue(await pixel(onGuide))), JSON.stringify(await pixel(onGuide)))

  // Pressed, it is selected; an arrow moves it a point, a drag moves it, Delete deletes it.
  await page.mouse.click(onGuide.x, onGuide.y)
  await press('ArrowDown')
  step('a selected guide moves with an arrow', await until(async () => Math.abs((await guides()).find((g) => g.index === added.index && g.page === added.page).position - (added.position + 1)) < 1e-6))
  const moved = (await guides()).find((g) => g.index === added.index && g.page === added.page)
  const grab = await toScreen(pageBounds[0] + 120, moved.position)
  const drop = await toScreen(pageBounds[0] + 120, moved.position + 61)
  await page.mouse.move(grab.x, grab.y)
  await page.mouse.down()
  await page.mouse.move(drop.x, drop.y, { steps: 6 })
  await page.mouse.up()
  step('a drag moves the guide', await until(async () => Math.abs((await guides()).find((g) => g.index === added.index && g.page === added.page).position - (moved.position + 61)) < 3), JSON.stringify(await guides()))
  const now = (await guides()).find((g) => g.index === added.index && g.page === added.page)
  const press2 = await toScreen(pageBounds[0] + 120, now.position)
  await page.mouse.click(press2.x, press2.y)
  await press('Delete')
  step('Delete deletes the selected guide', await until(async () => (await guides()).length === before))

  // A guide dropped back on its ruler goes.
  await one('guide.add', { orientation: 'horizontal', position: 300, spread: 0, page: 0 })
  await until(async () => (await guides()).length === before + 1)
  const back = await toScreen(pageBounds[0] + 150, 300)
  await page.mouse.move(back.x, back.y)
  await page.mouse.down()
  await page.mouse.move(back.x, ruler.y + ruler.height / 2, { steps: 8 })
  await page.mouse.up()
  step('a guide dragged onto the ruler is deleted', await until(async () => (await guides()).length === before))

  // Show and hide: Cmd+; the guides; W the preview (no guides, no frame edges).
  await one('guide.add', { orientation: 'horizontal', position: 320, spread: 0, page: 0 })
  const shown = await toScreen(pageBounds[0] + 100, 320)
  await until(async () => guideBlue(await pixel(shown)))
  await page.mouse.click(10, 300).catch(() => undefined)
  await kit.getByTestId('layout-workspace').focus().catch(() => undefined)
  await press('ControlOrMeta+;')
  step('Cmd+; hides the guides', await until(async () => !guideBlue(await pixel(shown))), JSON.stringify(await pixel(shown)))
  await press('ControlOrMeta+;')
  step('and shows them again', await until(async () => guideBlue(await pixel(shown))))
  // An empty graphic frame between the sample's text and its rectangle: its edge and cross are frame edges.
  const frame = await one('frame.add', { rect: [pageBounds[0] + 300, 350, pageBounds[0] + 420, 420], content: 'graphic' })
  const edge = await toScreen(pageBounds[0] + 300, 385)
  const edgeShown = async () => {
    const rgb = await pixel(edge, true)
    return rgb[0] < 250 || rgb[1] < 250 || rgb[2] < 250
  }
  step('an empty frame shows its edges', await until(edgeShown), JSON.stringify(await pixel(edge, true)))
  await press('w')
  step('W shows the preview: no guides', await until(async () => !guideBlue(await pixel(shown))))
  step('and no frame edges', await until(async () => !(await edgeShown())), JSON.stringify(await pixel(edge, true)))
  await press('w')
  step('W again: back to Normal', await until(async () => guideBlue(await pixel(shown)) && (await edgeShown())), `${JSON.stringify(await pixel(shown))} ${JSON.stringify(await pixel(edge, true))}`)
  await one('edit.delete', { ids: [frame.id] })

  // Drawing: a shape is as tall as the drag (no hidden baseline grid); near a guide it snaps, and not with snapping off.
  await one('guide.deleteAll', { spread: 0 })
  await press('m')
  const count = async () => (await inspect()).spreads[0].items.length
  const drawn = async (x, y0, y1) => {
    const items = await count()
    const a = await toScreen(pageBounds[0] + x, y0)
    const b = await toScreen(pageBounds[0] + x + 80, y1)
    await page.mouse.move(a.x, a.y)
    await page.mouse.down()
    await page.mouse.move(b.x, b.y, { steps: 8 })
    await page.mouse.up()
    await until(async () => (await count()) === items + 1)
    return (await inspect()).spreads[0].items.at(-1)
  }
  const tall = await drawn(150, 103, 263)
  step('a drawn shape is as tall as the drag', Math.abs(tall.bounds[3] - tall.bounds[1] - 160) < 1.5, JSON.stringify(tall.bounds))
  await one('guide.add', { orientation: 'horizontal', position: 400, spread: 0, page: 0 })
  await until(async () => (await guides()).length === 1)
  const snapped = await drawn(300, 330, 398)
  step('a shape drawn to near a guide snaps to it', Math.abs(snapped.bounds[3] - 400) < 0.01, JSON.stringify(snapped.bounds))
  await press('ControlOrMeta+Shift+;')
  await press('ControlOrMeta+u')
  const loose = await drawn(420, 330, 398)
  step('with snapping off it does not', Math.abs(loose.bounds[3] - 398) < 0.6, JSON.stringify(loose.bounds))
  await press('ControlOrMeta+Shift+;')
  await press('ControlOrMeta+u')
  await press('v')

  // Units: the ruler's menu sets millimetres; the panel and Document Setup read an A4 page in them.
  await one('layout.documentSetup', { width: 595.276, height: 841.89 })
  const rulerBox = await kit.getByTestId('layout-ruler-x').boundingBox()
  await page.mouse.click(rulerBox.x + 200, rulerBox.y + 6, { button: 'right' })
  await kit.getByTestId('layout-context-row-kit:view.unit.mm').click()
  step('the ruler menu sets millimetres', await until(async () => (await kit.getByTestId('layout-ruler-x').getAttribute('data-unit')) === 'mm'))
  const a4 = await one('frame.add', { rect: [pageBounds[0], 0, pageBounds[0] + 595.276, 841.89], content: 'graphic' })
  await one('object.stroke', { weight: 0, ids: [a4.id] })
  await select(a4.id)
  const field = (id) => kit.getByTestId(id).locator('input').first()
  step('the panel reads an A4 page as 210 × 297 mm', await until(async () => Number.parseFloat(await field('layout-w').inputValue()) === 210 && Number.parseFloat(await field('layout-h').inputValue()) === 297), `${await field('layout-w').inputValue().catch(() => '?')} × ${await field('layout-h').inputValue().catch(() => '?')}`)
  await kit.getByTestId('layout-menu-File').click()
  await kit.getByTestId('layout-menu-document-setup').click()
  const setupW = kit.getByTestId('layout-setup-width').locator('input').first()
  const setupH = kit.getByTestId('layout-setup-height').locator('input').first()
  step('Document Setup reads it as 210 × 297 mm', await until(async () => Number.parseFloat(await setupW.inputValue()) === 210 && Number.parseFloat(await setupH.inputValue()) === 297))
  await press('Escape')

  // Preferences: the ruler unit; a length typed with its own unit.
  await press('ControlOrMeta+k')
  step('Cmd+K opens Preferences', await until(() => kit.getByTestId('layout-preferences').isVisible()))
  await kit.getByTestId('layout-preferences-unit').selectOption('cm')
  await kit.getByTestId('layout-preferences-increment').fill('2mm')
  await kit.getByTestId('layout-dialog-ok').click()
  step('its ruler unit is the rulers\' unit', await until(async () => (await kit.getByTestId('layout-ruler-x').getAttribute('data-unit')) === 'cm'))
  const increment = async () => (await one('file.preferences', {}))?.keyboardIncrement
  step('a length typed as 2mm is 2 mm', await until(async () => Math.abs((await increment()) - (2 * 72) / 25.4) < 1e-6), String(await increment()))

  // The view options outlast a reload, and are not the document's.
  await press('ControlOrMeta+;')
  await page.reload()
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  step('the unit outlasts a reload', await until(async () => (await kit.getByTestId('layout-ruler-x').getAttribute('data-unit')) === 'cm'))
  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} ${error?.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
