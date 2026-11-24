// The undo history comes back from the host's stored replica, not from the file (kition-sdk Kit UI
// protocol 7; a `.klayout` holds the content, not its history): node tests/history.mjs [chromium|webkit].
// Opens the sample layout under the development host with its replica store on (`?replica=1`),
// adds a page (a person's edit), saves, reloads the document as its save left it
// (`devHost.reload()`, which keeps the stored replica) and undoes the edit made before the save.
// The control: the same file opened alone (`devHost.open`, no stored replica) has nothing to undo.
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
const until = async (check, tries = 100) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const details = () => page.evaluate(async () => (await window.devHost.connection().ai.inspect()).details)
const pageCount = async () => (await details()).pageCount
const kit = page.frameLocator('#kit')
const shortcut = async (keys) => {
  await kit.locator('.kui-app').focus()
  await page.keyboard.press(keys)
}
/** Runs `reopen` in the page and waits for the editor in the new frame to show the layout. */
const reopened = async (reopen, pages) => {
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.evaluate(() => {
    window.beforeReopen = true
  })
  await page.evaluate(reopen, FILE)
  const fresh = await (await page.$('#kit')).contentFrame()
  await fresh.waitForFunction(() => window.beforeReopen === undefined, null, { timeout: 60000 })
  await until(async () => (await pageCount()) === pages, 300)
  await page.waitForTimeout(500)
}

try {
  await page.goto(`${origin}/dev.html?replica=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  await page.waitForTimeout(1000)
  const before = await pageCount()
  step('the file alone opens with nothing to undo', (await details()).canUndo === false)

  // A person's edit, then a save: the file and the stored replica.
  await kit.getByTestId('layout-add-page').click()
  step('adds a page', await until(async () => (await pageCount()) === before + 1), `${await pageCount()} pages`)
  await page.waitForTimeout(500)
  const fileVersion = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, FILE)
  const saved = await fileVersion()
  await shortcut('ControlOrMeta+s')
  step('saves through the host', await until(async () => (await fileVersion()) > saved), `file version ${await fileVersion()}`)
  step('the host stores the replica', await until(async () => page.evaluate((path) => window.devHost.history.replica(path) != null, FILE)))

  // Reloaded as the save left it, on the stored replica: the edit from before the save undoes.
  await reopened(() => window.devHost.reload(), before + 1)
  step('the reloaded layout has the page', (await pageCount()) === before + 1, `${await pageCount()} pages`)
  step('Undo is enabled after the reload', await until(async () => (await details()).canUndo === true))
  await shortcut('ControlOrMeta+z')
  step('one undo takes back the page added before the save', await until(async () => (await pageCount()) === before), `${await pageCount()} pages`)

  // The control: the saved file opened alone (no stored replica) holds no history to undo.
  await reopened((path) => window.devHost.open(path, window.devHost.files.get(path).bytes), before + 1)
  step('the file alone has the page', (await pageCount()) === before + 1, `${await pageCount()} pages`)
  step("and nothing to undo: the history is the store's, not the file's", (await details()).canUndo === false)

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
