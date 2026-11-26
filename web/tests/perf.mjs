// Typing reaches the screen first (plan 2026-11-25, phase 12): node tests/perf.mjs [chromium|webkit]
// The walkthrough measured 94–108 ms from a key to the picture on a 22-page story (the engine took
// 10): after every key the page rendered its whole view again, and the dock's panels rendered
// before it. Counted at the engine's worker, with no clock in the assertions: each key asks for one
// picture, of the lines it changed and not of the view, before any panel asks the engine anything;
// and the picture built from those patches is the one a full repaint draws.
import './platform.mjs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}

const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
// Every request to the engine's worker, in order, with what it asks for and when the frame's keys went down.
await context.addInitScript(() => {
  window.__requests = []
  addEventListener('keydown', () => window.__requests.push({ key: true }), true)
  const Original = window.Worker
  if (!Original) return
  window.Worker = class extends Original {
    constructor(...args) {
      super(...args)
      const post = this.postMessage.bind(this)
      this.postMessage = (message, transfer) => {
        const request = message?.request
        if (request?.method) window.__requests.push({ method: request.method, name: request.name ?? request.command ?? null, width: request.width ?? null, height: request.height ?? null })
        return post(message, transfer)
      }
    }
  }
})
context.setDefaultTimeout(15000)
const page = await context.newPage()
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const kit = page.frameLocator('#kit')
const frame = () => page.frames().find((f) => f !== page.mainFrame())
const until = async (check, tries = 80) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
const view = async () => (await kit.getByTestId('layout-workspace').getAttribute('data-view') ?? '1 0 0').split(' ').map(Number)
const toScreen = async (x, y) => {
  const d = await details()
  const placed = d.canvas.spreads[0]
  const left = Math.min(...d.spreads[0].pages.map((p) => p.bounds[0]))
  const [zoom, vx, vy] = await view()
  const box = await kit.getByTestId('layout-canvas-region').boundingBox()
  return { x: box.x + (placed.x + x - left - vx) * zoom, y: box.y + (placed.y + y - vy) * zoom }
}
const tool = async () => (await details()).tool

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await page.mouse.click(region.x + 20, region.y + 20)
  // A story of a few threaded columns' worth of text on the first spread.
  const para = 'The aurora is a natural light display in the sky, seen mostly in high-latitude regions. It forms when the solar wind disturbs the magnetosphere. '
  const text = Array.from({ length: 14 }, (_, i) => `${i + 1}. ${para}`).join('\n')
  await agent([{ command: 'frame.add', params: { spread: 0, rect: [60, 300, 300, 760], content: 'text', text, caret: false } }])
  await page.waitForTimeout(800)
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('t')
  await until(async () => (await tool()) === 'type', 30)
  const at = await toScreen(120, 320)
  await page.mouse.click(at.x, at.y)
  await until(async () => (await details()).selection.text != null)
  await page.waitForTimeout(1500)
  const surface = await kit.getByTestId('kit-canvas-surface').evaluate((canvas) => ({ width: canvas.width, height: canvas.height }))

  await frame().evaluate(() => { window.__requests.length = 0 })
  const keys = ['w', 'o', 'r', 'd', 's', 'Enter', 'a', 'n', 'd', 'Backspace']
  for (const key of keys) {
    await page.keyboard.press(key)
    await page.waitForTimeout(400)
  }
  // Taken before the document has been still long enough for the Pages panel's thumbnails.
  const log = await frame().evaluate(() => window.__requests)
  // The requests after each key, up to the next key.
  const perKey = []
  for (const entry of log) {
    if (entry.key) perKey.push([])
    else perKey.at(-1)?.push(entry)
  }
  const pictures = perKey.map((requests) => requests.filter((r) => r.method === 'render' || r.method === 'renderArea'))
  step(`each of the ${keys.length} keys asks for one picture`, perKey.length === keys.length && pictures.every((list) => list.length === 1), JSON.stringify(pictures.map((list) => list.length)))
  const area = surface.width * surface.height
  const sizes = pictures.map((list) => (list[0] ? list[0].width * list[0].height : Infinity))
  const shown = `${JSON.stringify(pictures.map((list) => list[0] && `${list[0].width}x${list[0].height}`))} of ${surface.width}x${surface.height}`
  // A letter repaints its line (under a quarter of the canvas); Enter and Backspace the lines
  // they move, still not the view (the page rendered a picture larger than the canvas each key).
  const moves = new Set(['Enter', 'Backspace'])
  // (A letter that wraps a word onto a new line moves the lines after it too.)
  const lines = sizes.filter((size, i) => !moves.has(keys[i]) && size * 4 < area).length
  step('… of the lines a letter changed, not the view (under a quarter of the canvas, for most letters)', lines >= keys.length - moves.size - 2, shown)
  step('… and of the lines Enter and Backspace move, under half of it', sizes.every((size) => size * 2 < area), shown)
  const before = perKey.map((requests) => {
    const picture = requests.findIndex((r) => r.method === 'render' || r.method === 'renderArea')
    return picture < 0 ? -1 : requests.slice(0, picture).filter((r) => !(r.method === 'tool' && /^tool(Text|Key)$/.test(r.name ?? ''))).length
  })
  step('… and no other round trip to the engine comes before the picture (the panels ask after)', before.every((count) => count === 0), JSON.stringify(before))

  // The picture made of patches is the one a full repaint draws: out of the text, Preview and back.
  await page.keyboard.press('Escape')
  await until(async () => (await details()).selection.text == null, 30)
  await page.waitForTimeout(1200)
  const shot = () => kit.getByTestId('kit-canvas-surface').screenshot()
  const patched = await shot()
  await page.keyboard.press('w')
  await page.waitForTimeout(1000)
  await page.keyboard.press('w')
  await page.waitForTimeout(1500)
  const full = await shot()
  step('the patched picture is the fully repainted one', patched.equals(full), `${patched.length} and ${full.length} bytes`)
  step('typing: no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} perf threw - ${error?.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
