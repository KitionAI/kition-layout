// Scrolling stops at the pasteboard's edge: node tests/scroll.mjs [chromium|webkit].
// The engine draws the spreads and a margin of pasteboard around them (the canvas, in document
// pixels from its top-left). The wheel must not move the view past that canvas on any side, at
// the opening zoom or zoomed in; a view larger than the canvas keeps it centred.
import './platform.mjs'
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const surface = kit.getByTestId('kit-canvas-surface')
  await surface.waitFor()
  const workspace = kit.getByTestId('layout-workspace')
  for (let attempt = 0; attempt < 200 && !(await workspace.getAttribute('data-view')); attempt += 1) await page.waitForTimeout(150)

  // The canvas: the spreads' union and the same margin on every side (the leftmost and topmost spread sit one margin in).
  const placed = (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details.canvas.spreads
  const canvas = {
    width: Math.max(...placed.map((spread) => spread.x + spread.width)) + Math.min(...placed.map((spread) => spread.x)),
    height: Math.max(...placed.map((spread) => spread.y + spread.height)) + Math.min(...placed.map((spread) => spread.y)),
  }
  const box = await surface.boundingBox()
  const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 }
  const read = async () => {
    const [zoom, x, y] = (await workspace.getAttribute('data-view')).split(' ').map(Number)
    return { zoom, x, y }
  }
  /** The view once it has stopped moving. */
  const settled = async () => {
    let before = await read()
    for (let attempt = 0; attempt < 40; attempt += 1) {
      await page.waitForTimeout(150)
      const now = await read()
      if (now.zoom === before.zoom && now.x === before.x && now.y === before.y) return now
      before = now
    }
    return before
  }
  /** Turns the wheel far past any edge, then gives the view. */
  const wheel = async (dx, dy) => {
    await page.mouse.move(middle.x, middle.y)
    for (let turn = 0; turn < 4; turn += 1) await page.mouse.wheel(dx, dy)
    return settled()
  }
  /** Where the view's start may be along one axis: [lowest, highest]. A view longer than the canvas keeps it centred. */
  const range = (extent, length) => (extent >= length ? [(length - extent) / 2, (length - extent) / 2] : [0, length - extent])
  const check = async (when) => {
    const edges = [
      ['up', 0, -20000, 'y', 0],
      ['down', 0, 20000, 'y', 1],
      ['left', -20000, 0, 'x', 0],
      ['right', 20000, 0, 'x', 1],
    ]
    for (const [name, dx, dy, axis, end] of edges) {
      const view = await wheel(dx, dy)
      const want = range((axis === 'x' ? box.width : box.height) / view.zoom, axis === 'x' ? canvas.width : canvas.height)[end]
      assert(Math.abs(view[axis] - want) <= 1, `${when}, wheel ${name}: the view stops at the canvas's edge (${axis} ${view[axis]}, want ${want}; canvas ${canvas.width} x ${canvas.height}, zoom ${view.zoom})`)
    }
  }

  await check('at the opening zoom')

  // Zoomed in until the canvas is well larger than the view on both axes: now there is room to scroll.
  await page.mouse.move(middle.x, middle.y)
  await page.keyboard.down('Control')
  for (let turn = 0; turn < 40; turn += 1) {
    const { zoom } = await read()
    if (canvas.width * zoom > box.width * 1.5 && canvas.height * zoom > box.height * 1.5) break
    await page.mouse.wheel(0, -120)
    await page.waitForTimeout(100)
  }
  await page.keyboard.up('Control')
  const zoomed = await settled()
  assert(canvas.width * zoomed.zoom > box.width * 1.5 && canvas.height * zoomed.zoom > box.height * 1.5, `zoomed in: ${JSON.stringify(zoomed)}`)
  await check('zoomed in')

  assert.equal(await kit.getByTestId('layout-unsaved').count(), 0, 'scrolling is not an edit')
  assert.deepEqual(errors, [], 'no page errors')
  console.log(`PASS ${which}: scrolling stops at the pasteboard's edge`)
} finally {
  await browser.close()
  await server.close()
}
