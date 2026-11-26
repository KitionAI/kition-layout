// Paths by hand (plan 2026-11-25, stage 8): node tests/paths.mjs [chromium|webkit]
// Opens dev.html and checks, as a person works: on a selected path the Pen's cursor says what a
// click does (add an anchor on the outline, delete an anchor, continue from an end point, close on
// the first anchor) and each click does it as one undo step; Direct Selection's marquee selects
// anchor points of two paths, a drag of one moves both, the arrows move them and Delete deletes
// them (the paths stay), each one undo step, and one undo takes a step back whole.
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
/** A path's first subpath as `inspect()` reads it back: closed, and its points on the spread. */
const subpath = async (id) => (await itemOf(id))?.subpaths?.[0] ?? null
const points = async (id) => (await subpath(id))?.anchors ?? []
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
const near = (a, b, tolerance = 1.1) => Math.abs(a - b) <= tolerance
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
/** The CSS cursor the engine named under the drawn one (its fallback). */
const named = async () => (await cursor()).split(',').pop().trim()
const hover = async (x, y) => {
  const at = await toScreen(x, y)
  await page.mouse.move(at.x - 3, at.y - 3)
  await page.mouse.move(at.x, at.y, { steps: 2 })
  await page.waitForTimeout(150)
}
const click = async (x, y) => {
  const at = await toScreen(x, y)
  await page.mouse.click(at.x, at.y)
  await page.waitForTimeout(150)
}
const key = async (name) => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press(name)
}
const tool = async (letter, id) => {
  await key(letter)
  return until(async () => (await details()).tool === id)
}

try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await kit.locator('.kui-app').focus()
  // A first press on the canvas (the editor leaves an empty opening layer then; stage 4).
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await page.mouse.click(region.x + 20, region.y + 20)
  await page.keyboard.press('ControlOrMeta+1')
  await page.waitForTimeout(400)
  // The sample's own objects are locked, so clicks and marquees reach only the paths drawn here.
  await agent([{ command: 'edit.selectAll', params: {} }, { command: 'object.lock', params: {} }])
  const box = (await details()).spreads[0].pages[0].bounds
  const x0 = box[0] + 300
  const y0 = box[1] + 420

  // ---- The Pen on a selected path. ----
  const path = await one('path.add', { anchors: [{ p: [x0, y0] }, { p: [x0 + 100, y0] }, { p: [x0 + 100, y0 + 100] }] })
  const id = path?.id
  step('an open path to work on', id != null && (await points(id)).length === 3, JSON.stringify(path))
  await tool('v', 'selection')
  await click(x0 + 50, y0)
  await until(async () => same((await details()).selection.items, [id]))
  step('a click on its outline selects it', same((await details()).selection.items, [id]), JSON.stringify((await details()).selection))
  await tool('p', 'pen')
  await hover(x0 + 50, y0)
  step('the Pen over the selected outline shows "add"', (await named()) === 'copy' && (await cursor()).startsWith('url('), await named())
  let before = await history()
  await click(x0 + 50, y0)
  await until(async () => (await points(id)).length === 4)
  step('a click on the outline adds an anchor there, one undo step', (await points(id)).length === 4 && (await history()) === before + 1, `${JSON.stringify(await points(id))} ${before} → ${await history()}`)
  await hover(x0 + 50, y0)
  step('over the new anchor the cursor says "delete"', (await named()) === 'no-drop', await named())
  await click(x0 + 50, y0)
  await until(async () => (await points(id)).length === 3)
  step('a click on an anchor deletes it', same((await points(id)).map(([x]) => Math.round(x)), [x0, x0 + 100, x0 + 100]), JSON.stringify(await points(id)))
  await hover(x0 + 100, y0 + 100)
  step('over the end point the cursor says "continue"', (await named()) === 'cell', await named())
  const count = (await items()).length
  before = await history()
  await click(x0 + 100, y0 + 100)
  await click(x0, y0 + 100)
  await until(async () => (await points(id)).length === 4)
  const continued = await points(id)
  step('a click on the end point and one beyond continue the same path', continued.length === 4 && (await items()).length === count && near(continued[3][0], x0) && near(continued[3][1], y0 + 100), JSON.stringify(continued))
  step('… the new anchor is one undo step', (await history()) === before + 1, `${before} → ${await history()}`)
  await hover(x0, y0)
  step('over the first anchor while drawing the cursor says "close"', (await named()) === 'alias', await named())
  await click(x0, y0)
  await until(async () => (await subpath(id))?.closed === true)
  step('a click on the first anchor closes the path', (await subpath(id))?.closed === true && (await points(id)).length === 4, JSON.stringify(await subpath(id)))
  await hover(x0 + 300, y0 + 300)
  step('away from paths the Pen is a pen', (await named()) === 'crosshair' && (await cursor()).startsWith('url('), await named())

  // ---- Direct Selection: several anchor points of two paths. ----
  // Below the sample's picture, where nothing else is.
  const yb = box[1] + 735
  const yc = box[1] + 765
  const b = (await one('path.add', { anchors: [{ p: [x0, yb] }, { p: [x0 + 100, yb] }, { p: [x0 + 200, yb] }] }))?.id
  const c = (await one('path.add', { anchors: [{ p: [x0, yc] }, { p: [x0 + 100, yc] }, { p: [x0 + 200, yc] }] }))?.id
  await tool('a', 'directSelection')
  await key('Escape')
  await until(async () => (await details()).selection.items.length === 0)
  const from = await toScreen(x0 + 80, yb - 9)
  const to = await toScreen(x0 + 120, yc + 10)
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 4 })
  await page.mouse.move(to.x, to.y, { steps: 4 })
  await page.mouse.up()
  await until(async () => ((await details()).selection.anchors ?? []).length === 2)
  step('a marquee selects the anchor points inside it, on both paths', same((await details()).selection.anchors, [[b, 0, 1], [c, 0, 1]]), JSON.stringify((await details()).selection))
  before = await history()
  const grab = await toScreen(x0 + 100, yb)
  const zoom = (await view()).zoom
  await page.mouse.move(grab.x, grab.y)
  await page.mouse.down()
  await page.mouse.move(grab.x, grab.y + 10 * zoom, { steps: 4 })
  await page.mouse.move(grab.x, grab.y + 30 * zoom, { steps: 4 })
  await page.mouse.up()
  await until(async () => near((await points(b))[1][1], yb + 30))
  const [pb, pc] = [await points(b), await points(c)]
  step('dragging one selected point moves both', near(pb[1][1], yb + 30) && near(pc[1][1], yc + 30) && near(pb[1][0], pc[1][0], 0.01), `${JSON.stringify(pb[1])} ${JSON.stringify(pc[1])}`)
  step('… the others stay put', near(pb[0][1], yb) && near(pc[2][1], yc))
  step('… one undo step', (await history()) === before + 1, `${before} → ${await history()}`)
  const xb = pb[1][0]
  await key('ArrowRight')
  await until(async () => near((await points(b))[1][0], xb + 1, 0.01))
  step('the arrow moves both points one point', near((await points(b))[1][0], xb + 1, 0.01) && near((await points(c))[1][0], xb + 1, 0.01), `${JSON.stringify((await points(b))[1])} ${JSON.stringify((await points(c))[1])}`)
  step('… one undo step', (await history()) === before + 2)
  await key('ControlOrMeta+z')
  await until(async () => near((await points(b))[1][0], xb, 0.01))
  step('one undo takes both back', near((await points(b))[1][0], xb, 0.01) && near((await points(c))[1][0], xb, 0.01))
  before = await history()
  await key('Delete')
  await until(async () => (await points(b)).length === 2)
  step('Delete deletes the selected points, the paths stay', (await points(b)).length === 2 && (await points(c)).length === 2 && (await itemOf(b)) != null, `${JSON.stringify(await points(b))} ${JSON.stringify(await points(c))}`)
  step('… one undo step', (await history()) === before + 1, `${before} → ${await history()}`)
  await key('ControlOrMeta+z')
  await until(async () => (await points(b)).length === 3)
  step('one undo brings both points back', (await points(b)).length === 3 && (await points(c)).length === 3)

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('the paths flow ran', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
if (failures) {
  console.log(`${failures} failed`)
  process.exit(1)
}
