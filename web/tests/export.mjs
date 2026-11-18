// File › Export through a host that writes like Kition: node tests/export.mjs [chromium|webkit]
// Kition writes a new file (`host/write`, mode `export`) only in a format some kit's kit.json
// `exports` declares; before layout declared IDML and EPUB, Kition refused them and nothing was
// written. The development host is made to hold the same rule, from this kit's kit.json.
import { readFileSync } from 'node:fs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const declared = (JSON.parse(readFileSync(new URL('../../kit.json', import.meta.url), 'utf8')).exports ?? []).map((entry) => entry.extension)
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
// A step that cannot act (a dialog left open) fails instead of waiting for ever.
page.setDefaultTimeout(20000)
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

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  // Kition's rule for a new file, and a switch to refuse a write as a read-only workspace would.
  await page.evaluate((allowed) => {
    const files = window.devHost.files
    const set = files.set.bind(files)
    window.refuseWrites = null
    files.set = (name, file) => {
      const extension = name.split('.').pop().toLowerCase()
      if (extension !== 'klayout' && !allowed.includes(extension)) throw new Error(`no kit edits \`${name}\``)
      if (window.refuseWrites?.test(name)) throw new Error('the workspace is read-only')
      return set(name, file)
    }
  }, declared)
  const size = (name) => page.evaluate((file) => window.devHost.files.get(file)?.bytes?.byteLength ?? 0, name)
  const text = (name, from, to) =>
    page.evaluate(([file, a, b]) => new TextDecoder().decode(new Uint8Array(window.devHost.files.get(file)?.bytes ?? new ArrayBuffer(0)).slice(a, b)), [name, from, to])
  const toast = () => kit.locator('.kui-toast').last().textContent({ timeout: 300 }).catch(() => '')
  const exportAs = async (format, range) => {
    await kit.getByTestId('layout-menu-File').click()
    await kit.getByTestId('layout-menu-export').click()
    await kit.getByTestId('layout-export-format').selectOption(format)
    if (range) await kit.getByTestId('layout-export-range').selectOption(range)
    await kit.getByTestId('layout-export-ok').click()
  }

  await exportAs('idml')
  await until(async () => (await size('Sample.idml')) > 0)
  step('exports IDML through the host', (await text('Sample.idml', 0, 4)) === 'PK\x03\x04', `${await size('Sample.idml')} bytes`)
  step('the IDML package starts with its stored mimetype', (await text('Sample.idml', 30, 38)) === 'mimetype' && (await text('Sample.idml', 38, 81)) === 'application/vnd.adobe.indesign-idml-package')
  await until(async () => (await toast()).includes('Sample.idml'))
  step('says it exported, naming the file the host wrote', (await toast()).includes('Sample.idml'), await toast())

  await exportAs('epub')
  await until(async () => (await size('Sample.epub')) > 0)
  step('exports EPUB through the host', (await size('Sample.epub')) > 0)
  step('the EPUB starts with its uncompressed mimetype', (await text('Sample.epub', 30, 38)) === 'mimetype' && (await text('Sample.epub', 38, 58)) === 'application/epub+zip')
  await until(async () => (await toast()).includes('Sample.epub'))
  step('says it exported the EPUB', (await toast()).includes('Sample.epub'), await toast())

  await exportAs('pdf')
  await until(async () => (await size('Sample.pdf')) > 0)
  step('PDF still exports', (await text('Sample.pdf', 0, 5)) === '%PDF-')
  await exportAs('html')
  await until(async () => (await size('Sample.html')) > 0)
  step('HTML still exports', (await text('Sample.html', 0, 2)) === '<!')
  await exportAs('png', 'all')
  await until(async () => (await size('Sample-2.png')) > 0)
  step('PNG still exports every page, numbered', (await text('Sample-1.png', 1, 4)) === 'PNG' && (await size('Sample-2.png')) > 0)
  await exportAs('jpeg', 'current')
  await until(async () => (await size('Sample.jpg')) > 0)
  step('JPEG still exports the page in view', (await size('Sample.jpg')) > 0)

  // A second export takes an unused name; the host picks it.
  await exportAs('idml')
  await until(async () => (await size('Sample 2.idml')) > 0)
  step('a second export is a new file beside the first', (await size('Sample 2.idml')) > 0 && (await toast()).includes('Sample 2.idml'), await toast())

  // The host refuses: its error is shown, no success, nothing written, and the dialog stays open.
  await page.evaluate(() => {
    window.refuseWrites = /\.epub$/
  })
  await exportAs('epub')
  await until(async () => (await toast()).includes('read-only'))
  step("shows the host's error when it does not write the file", (await toast()).includes('read-only') && !(await toast()).includes('已导出'), await toast())
  step('the dialog stays open after a refusal', await kit.getByTestId('layout-export-dialog').isVisible())
  step('nothing is written after a refusal', (await size('Sample 2.epub')) === 0)
  await kit.getByTestId('layout-export-dialog').getByRole('button', { name: '取消' }).click()

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('the export flow ran', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} FAILED (${which})` : `all passed (${which})`)
process.exit(failures ? 1 : 0)
