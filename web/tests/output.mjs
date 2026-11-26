// Output by hand (plan 2026-11-25, stage 8): node tests/output.mjs [chromium|webkit]
// Opens dev.html in Chinese and checks, as a person works: Export's PDF options (printer's marks,
// PDF/X-4) reach the file; a page range the document does not have is refused in Chinese, with no
// English engine text; Preflight lists an overset frame in Chinese, and a click on its row selects
// the frame; Check Spelling walks the words, Change replaces one (one undo step) and Add puts a
// word in the user dictionary; the Links panel shows each graphic small and embeds a linked one;
// New Document makes a file with a primary text frame.
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
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
const size = (name) => page.evaluate((file) => window.devHost.files.get(file)?.bytes?.byteLength ?? 0, name)
const head = (name, count) => page.evaluate(([file, n]) => new TextDecoder().decode(new Uint8Array(window.devHost.files.get(file)?.bytes ?? new ArrayBuffer(0)).slice(0, n)), [name, count])
const toast = () => kit.locator('.kui-toast').last().textContent({ timeout: 300 }).catch(() => '')
/** Latin words (three letters or more) other than the names a message may quote. */
const english = (text) => /[A-Za-z]{3,}/.test(text.replace(/PDF|ppi|RGB|Sample|wrod|kitionish|\.png|\.pdf/g, ''))
/** Opens a menu of the menu bar and a row in it (through its submenus). */
const menu = async (name, ...rows) => {
  await kit.getByTestId(`layout-menu-${name}`).click()
  for (const row of rows) {
    const item = row.startsWith('sub:') ? kit.getByRole('menuitem', { name: row.slice(4), exact: true }) : kit.getByTestId(row)
    if (row.startsWith('sub:')) await item.hover()
    else await item.click()
  }
}
const storyText = async (story) => (await one('story.get', { story }))?.text ?? ''
/** A PNG of 8 × 8 red pixels, as base64. */
const red = () =>
  page.evaluate(async () => {
    const canvas = Object.assign(document.createElement('canvas'), { width: 8, height: 8 })
    const context2d = canvas.getContext('2d')
    context2d.fillStyle = '#ff0000'
    context2d.fillRect(0, 0, 8, 8)
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
    return btoa(String.fromCharCode(...new Uint8Array(await blob.arrayBuffer())))
  })

try {
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await kit.locator('.kui-app').focus()
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await page.mouse.click(region.x + 20, region.y + 20)
  await page.waitForTimeout(300)

  // ---- Export PDF: the options the exporter takes, and a refusal in Chinese. ----
  await menu('File', 'layout-menu-export')
  await kit.getByTestId('layout-export-format').selectOption('pdf')
  step('PDF shows its options', (await kit.getByTestId('layout-export-crop-marks').count()) === 1 && (await kit.getByTestId('layout-export-standard').count()) === 1 && (await kit.getByTestId('layout-export-spreads').count()) === 1)
  step('… in Chinese', (await kit.getByTestId('layout-export-dialog').textContent()).includes('裁切标记'))
  await kit.getByTestId('layout-export-range').selectOption('range')
  await kit.getByTestId('layout-export-pages').fill('40-50')
  await kit.getByTestId('layout-export-ok').click()
  await until(async () => (await toast()).includes('页面范围'))
  step('a page range the document does not have is refused in Chinese', (await toast()).includes('页面范围') && !english(await toast()), await toast())
  step('… nothing is written and the dialog stays open', (await size('Sample.pdf')) === 0 && (await kit.getByTestId('layout-export-dialog').isVisible()))
  await kit.getByTestId('layout-export-range').selectOption('all')
  await kit.getByTestId('layout-export-crop-marks').click()
  await kit.getByTestId('layout-export-standard').selectOption('x4')
  await kit.getByTestId('layout-export-ok').click()
  await until(async () => (await size('Sample.pdf')) > 0)
  step('PDF/X-4 with crop marks is written', (await head('Sample.pdf', 8)) === '%PDF-1.6', await head('Sample.pdf', 8))

  // ---- Preflight: an overset frame, in Chinese; a click selects it. ----
  const overset = await one('frame.add', { rect: [100, 100, 160, 112], content: 'text', text: 'A frame much too small for all of these words. '.repeat(4), caret: false })
  await menu('File', 'layout-cmd-preflight.run')
  await until(async () => (await kit.getByTestId('layout-preflight').count()) === 1)
  const row = kit.locator(`[data-testid^="layout-preflight-row-"][data-item="${overset.id}"]`)
  await until(async () => (await row.count()) === 1)
  const rowText = await row.textContent()
  step('Preflight lists the overset frame, in Chinese', rowText.includes('溢流文本') && !english(rowText), rowText)
  step('… with its page and a summary', rowText.includes('第 1 页') && (await kit.getByTestId('layout-preflight-summary').textContent()).includes('个错误'))
  await row.click()
  await until(async () => same((await details()).selection.items, [overset.id]))
  step('a click on the row selects the frame', same((await details()).selection.items, [overset.id]), JSON.stringify((await details()).selection))
  await kit.getByTestId('layout-preflight-done').click()

  // ---- Check Spelling: walk, change, add. ----
  const typo = await one('frame.add', { rect: [100, 600, 400, 640], content: 'text', text: 'One wrod and kitionish here', caret: false })
  await menu('Edit', 'sub:拼写检查', 'layout-cmd-spelling.check')
  await until(async () => (await kit.getByTestId('layout-spelling-word').count()) === 1)
  for (let i = 0; i < 6 && (await kit.getByTestId('layout-spelling-word').textContent()) !== 'wrod'; i += 1) {
    await kit.getByTestId('layout-spelling-skip').click()
    await page.waitForTimeout(150)
  }
  step('the check comes to the misspelled word', (await kit.getByTestId('layout-spelling-word').textContent()) === 'wrod')
  step('… in Chinese, with suggestions', (await kit.getByTestId('layout-spelling-status').textContent()) === '不在词典中：' && (await kit.getByTestId('layout-spelling-to').locator('input').inputValue().catch(() => kit.getByTestId('layout-spelling-to').inputValue())) === 'word')
  step('… and selects it in its frame', (await details()).selection.text?.story === typo.story, JSON.stringify((await details()).selection))
  let before = await history()
  await kit.getByTestId('layout-spelling-change').click()
  await until(async () => (await storyText(typo.story)).startsWith('One word'))
  step('Change replaces it, one undo step', (await storyText(typo.story)) === 'One word and kitionish here' && (await history()) === before + 1, `${await storyText(typo.story)} ${before} → ${await history()}`)
  await until(async () => (await kit.getByTestId('layout-spelling-word').textContent()) === 'kitionish')
  step('… and goes on to the next word', (await kit.getByTestId('layout-spelling-word').textContent()) === 'kitionish')
  await kit.getByTestId('layout-spelling-add').click()
  await until(async () => ((await one('spelling.words')) ?? []).includes('kitionish'))
  step('Add puts the word in the user dictionary', ((await one('spelling.words')) ?? []).includes('kitionish'))
  await until(async () => (await kit.getByTestId('layout-spelling-status').textContent()) === '拼写检查已完成。')
  step('… and the check is complete', (await kit.getByTestId('layout-spelling-status').textContent()) === '拼写检查已完成。', await kit.getByTestId('layout-spelling-status').textContent())
  await kit.getByTestId('layout-spelling-close').click()

  // ---- Links: a thumbnail; unembed, then embed again. ----
  const placed = await one('file.place', { base64: await red(), name: 'red.png', x: 300, y: 300, width: 40 })
  const asset = (await one('linkedFile.list'))?.[0]?.asset
  await kit.getByTestId('layout-dock-tab-links').click()
  await until(async () => (await kit.getByTestId(`layout-link-thumb-${asset}`).locator('img').count()) === 1)
  const drawnWidth = await kit.getByTestId(`layout-link-thumb-${asset}`).locator('img').evaluate((image) => image.naturalWidth).catch(() => 0)
  step('the Links panel shows the graphic small', drawnWidth === 8, `${drawnWidth} px (${JSON.stringify(placed)})`)
  await kit.getByTestId(`layout-link-unembed-${asset}`).click()
  await until(async () => (await kit.getByTestId(`layout-link-${asset}`).getAttribute('data-status')) !== 'embedded')
  step('unembedded, it is linked', (await kit.getByTestId(`layout-link-${asset}`).getAttribute('data-status')) === 'ok')
  await kit.getByTestId(`layout-link-embed-${asset}`).click()
  await until(async () => (await kit.getByTestId(`layout-link-${asset}`).getAttribute('data-status')) === 'embedded')
  step('Embed keeps only the copy in the document', (await kit.getByTestId(`layout-link-${asset}`).getAttribute('data-status')) === 'embedded')

  // ---- New Document with a primary text frame. ----
  await menu('File', 'layout-menu-new')
  await until(async () => (await kit.getByTestId('layout-new-primary').count()) === 1)
  await kit.getByTestId('layout-new-name').locator('input').fill('Primary').catch(() => kit.getByTestId('layout-new-name').fill('Primary'))
  await kit.getByTestId('layout-new-primary').click()
  step('the primary text frame option says what it does', (await kit.getByTestId('layout-dialog').textContent()).includes('智能文本重排'))
  await kit.getByTestId('layout-dialog-ok').click()
  await until(async () => (await size('Primary.klayout')) > 0)
  step('New Document writes the file with it', (await size('Primary.klayout')) > 0)

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('the output flow ran', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
if (failures) {
  console.log(`${failures} failed`)
  process.exit(1)
}
