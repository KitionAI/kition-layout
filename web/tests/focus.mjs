// Focus shows the canvas alone (no rulers) and Escape cancels first there: it ends a pen path or
// clears the selection and stays in Focus; only an Escape with nothing to cancel leaves Focus.
// node tests/focus.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const app = kit.locator('.kui-app')
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const selected = async () => (await details()).selection?.items?.length ?? 0
const mode = () => app.getAttribute('data-mode')
const until = async (name, check) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await check().catch(() => false)) {
      console.log(`PASS ${which} ${name}`)
      return
    }
    await page.waitForTimeout(100)
  }
  assert.fail(`${which} ${name}`)
}
const focus = async () => {
  await app.focus()
  await page.keyboard.press('ControlOrMeta+Backslash')
  await until('Cmd+\\ enters Focus', async () => (await mode()) === 'focus')
}

try {
  await page.goto(`${origin}/dev.html?modes=classic,line,menus,focus&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60_000 })
  await page.waitForTimeout(800)
  await until('the rulers show outside Focus', async () => (await kit.locator('.layout-ruler').count()) === 2)

  // Focus: no rulers, and the canvas takes their place.
  await focus()
  await until('no rulers in Focus', async () => (await kit.locator('.layout-ruler, .layout-ruler-corner').count()) === 0)
  const main = await kit.locator('.kui-frame-main').boundingBox()
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  assert.ok(Math.abs(region.x - main.x) < 2 && Math.abs(region.y - main.y) < 2, `the canvas starts where the frame's body does (${JSON.stringify(region)} in ${JSON.stringify(main)})`)
  console.log(`PASS ${which} the canvas takes the rulers' place`)

  // A pen path in progress is cancelled by Escape, which stays in Focus.
  await page.keyboard.press('p')
  await until('P picks the pen', async () => (await kit.locator('.kui-focus-tool').textContent())?.includes('Pen'))
  await page.mouse.click(region.x + region.width * 0.45, region.y + region.height * 0.1)
  await page.mouse.click(region.x + region.width * 0.55, region.y + region.height * 0.12)
  await until('the pen path has started', async () => (await selected()) === 1)
  // The editor's state follows the engine's answer to the click a moment later.
  await page.waitForTimeout(300)
  await page.keyboard.press('Escape')
  await until('Escape ends the pen path', async () => (await selected()) === 0)
  assert.equal(await mode(), 'focus', 'an Escape that ended a path does not leave Focus')
  console.log(`PASS ${which} an Escape that ended a path stays in Focus`)

  // Back to the selection tool; then a selection is something to cancel.
  await page.keyboard.press('v')
  await until('V picks the selection tool', async () => (await kit.locator('.kui-focus-tool').textContent())?.includes('Selection'))
  await page.mouse.click(region.x + region.width * 0.5, region.y + region.height * 0.72)
  await until('a click selects a frame', async () => (await selected()) === 1)
  // The editor's state follows the engine's answer to the click a moment later.
  await page.waitForTimeout(300)
  await page.keyboard.press('Escape')
  await until('Escape clears the selection', async () => (await selected()) === 0)
  assert.equal(await mode(), 'focus', 'an Escape that cleared the selection does not leave Focus')
  console.log(`PASS ${which} an Escape that cleared the selection stays in Focus`)

  // Nothing left to cancel: Escape leaves Focus, and the rulers come back.
  await page.keyboard.press('Escape')
  await until('Escape with nothing to cancel leaves Focus', async () => (await mode()) === 'classic')
  await until('the rulers come back', async () => (await kit.locator('.layout-ruler').count()) === 2)
  assert.deepEqual(errors, [])
  console.log(`PASS ${which} no page errors`)
} finally {
  await browser.close()
  await server.close()
}
