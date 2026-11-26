// An agent's draft and a restored version in the editor (Kit UI protocol sections 7d and 7m; the
// engine's mergeChanges and restoreState): node tests/drafts.mjs [chromium|webkit].
// Under the development host with `?checkpoints=1`: the agent adds a frame in its draft, the person
// adds a page to the layout meanwhile, the person accepts the draft (the host passes the agent's
// origin): both are there, and one Undo of the person takes the whole draft back and keeps the
// page. Then a checkpoint of the saved layout, a page added after it, the checkpoint restored: the
// layout is as the checkpoint kept it, and one Undo takes the restore back.
// Needs the engine built into public/ (pnpm engine).
import './platform.mjs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const FILE = 'Sample.klayout'
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
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 150) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const kit = page.frameLocator('#kit')
const details = () => page.evaluate(async () => (await window.devHost.connection().ai.inspect()).details)
const pageCount = async () => (await details()).pageCount
const frames = (inspected) => inspected.spreads.reduce((sum, spread) => sum + spread.items.length, 0)
const frameCount = async () => frames(await details())
/** One agent step, in the layout or (`draft`) in the agent's draft. */
const run = (command, params = {}, options = {}) =>
  page.evaluate(async ({ command, params, options }) => (await window.devHost.connection().ai.run([{ command, params }], false, options)).completed[0].result, { command, params, options })
const shortcut = async (keys) => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press(keys)
}
const fileVersion = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, FILE)

try {
  await page.goto(`${origin}/dev.html?checkpoints=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  await page.waitForTimeout(1000)
  const pages = await pageCount()
  const framesBefore = await frameCount()

  // The agent's draft: a frame the layout does not show yet.
  await run('frame.add', { rect: [40, 40, 200, 120], content: 'unassigned' }, { draft: true })
  const inDraft = await run('file.inspect', {}, { draft: true })
  step('the draft holds the agent\'s frame', frames(inDraft) === framesBefore + 1, `${frames(inDraft)} frames in the draft`)
  step('the layout does not, yet', (await frameCount()) === framesBefore)

  // The person adds a page meanwhile.
  await kit.getByTestId('layout-add-page').click()
  step('the person adds a page', await until(async () => (await pageCount()) === pages + 1), `${await pageCount()} pages`)
  await page.waitForTimeout(500)

  // The person accepts the draft; the host passes the agent's origin.
  const merged = await page.evaluate(() => window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' }))
  step('the draft merges', merged?.merged === true, JSON.stringify(merged))
  step('its frame shows', await until(async () => (await frameCount()) === framesBefore + 1), `${await frameCount()} frames`)
  step('the page added meanwhile stays', (await pageCount()) === pages + 1, `${await pageCount()} pages`)

  // One Undo of the person takes the whole draft back and keeps the person's page.
  await shortcut('ControlOrMeta+z')
  step('one Undo takes the draft back', await until(async () => (await frameCount()) === framesBefore), `${await frameCount()} frames`)
  step('and keeps the person\'s page', (await pageCount()) === pages + 1, `${await pageCount()} pages`)
  await page.waitForTimeout(500)

  // A checkpoint of the saved layout, a page added after it, the checkpoint restored, Undo.
  const saved = await fileVersion()
  await shortcut('ControlOrMeta+s')
  step('saves through the host', await until(async () => (await fileVersion()) > saved), `file version ${await fileVersion()}`)
  step('the host stores the replica', await until(() => page.evaluate((path) => window.devHost.history.replica(path) != null, FILE)))
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save' }))
  step('a checkpoint is recorded', Boolean(checkpoint?.id), JSON.stringify(checkpoint))
  await kit.getByTestId('layout-add-page').click()
  step('a page after the checkpoint', await until(async () => (await pageCount()) === pages + 2), `${await pageCount()} pages`)
  await page.waitForTimeout(500)
  await page.evaluate((id) => window.devHost.restoreCheckpoint(id), checkpoint.id)
  step('the restore gives back the checkpoint\'s pages', await until(async () => (await pageCount()) === pages + 1), `${await pageCount()} pages`)
  step('and its frames', (await frameCount()) === framesBefore, `${await frameCount()} frames`)
  await page.waitForTimeout(500)
  await shortcut('ControlOrMeta+z')
  step('one Undo takes the restore back', await until(async () => (await pageCount()) === pages + 2), `${await pageCount()} pages`)

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
