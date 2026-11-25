// Text and tables by hand (plan 2026-11-25, stage 5): node tests/text.mjs [chromium|webkit]
// Opens dev.html and checks, as a person works: with the Type tool two clicks select a word, three
// the line, four the paragraph, five the story, and a drag after a double click grows by words;
// a format chosen with only a caret (Underline) shows in the panel and goes to the text typed
// next; an input method's marked text (synthetic composition events) shows in the text with no
// undo step, the hidden field sits at the caret, the committed text is one step and a cancelled
// composition leaves nothing; Alt+Right kerns the pair at the caret; a click in an empty frame
// makes it a text frame; in a table Tab and Shift+Tab move between cells, dragging a column border
// resizes the column (Shift keeps the table's width), Esc selects the cell and Esc again its
// text, and the Properties panel's Table group sets the row height and the column count; the
// bullets key and the Home tab's numbering button switch lists; with the loaded text cursor the
// linked frame shows the unthread cursor and a click on it breaks the thread.
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
const storyText = async (id) => (await one('story.get', { story: id }))?.text
const textSel = async () => (await details()).selection.text ?? null
const range = async () => {
  const t = await textSel()
  return t ? [Math.min(t.anchor, t.focus), Math.max(t.anchor, t.focus)] : null
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
/** The formatting of `[from, to)` of a story (selects it, as an agent does). */
const attrsAt = async (story, from, to) => (await agent([{ command: 'text.select', params: { story, anchor: from, focus: to } }, { command: 'text.selectionAttrs', params: {} }])).completed[1]?.result
/** Picks a tool from the rail, as a person does, and waits for the engine to have it. */
const tool = async (id) => {
  if ((await details()).tool === id) return true
  await kit.locator(`[data-kit-ui-id="tool:${id}"]`).first().click()
  return until(async () => (await details()).tool === id)
}
const view = async () => {
  const [zoom, x, y] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  return { zoom, x, y }
}

try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  // Page 1 at 100%, its top-left in view.
  await kit.locator('.kui-app').focus()
  await page.keyboard.press('ControlOrMeta+1')
  await page.waitForTimeout(400)
  const box = (await details()).spreads[0].pages[0].bounds
  const surfaceBox = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const place = async () => {
    const d = await details()
    const placed = d.canvas?.spreads?.[0]
    const left = Math.min(...d.spreads[0].pages.map((p) => p.bounds[0]))
    const v = await view()
    return (x, y) => ({ x: surfaceBox.x + ((placed?.x ?? 0) + x - left - v.x) * v.zoom, y: surfaceBox.y + ((placed?.y ?? 0) + y - v.y) * v.zoom })
  }
  // Scroll page 1's top-left area into view: a frame we work in sits there.
  let toScreen = await place()
  const x0 = box[0] + 60
  const y0 = box[1] + 80

  // Several clicks with the Type tool.
  const frame = await one('frame.add', { rect: [x0, y0, x0 + 300, y0 + 150], content: 'text', text: 'Alpha beta gamma\nSecond paragraph here', caret: false })
  const story = (await details()).stories.find((item) => item.frames.includes(frame.id)).id
  toScreen = await place()
  await tool('type')
  const word = toScreen(x0 + 12, y0 + 8)
  for (const [clicks, want, name] of [[2, [0, 5], 'two clicks select a word'], [3, [0, 16], 'three clicks select the line'], [4, [0, 17], 'four clicks select the paragraph'], [5, [0, 38], 'five clicks select the story']]) {
    await page.mouse.click(word.x, word.y, { clickCount: clicks })
    step(name, await until(async () => same(await range(), want)), JSON.stringify(await range()))
    await page.waitForTimeout(600)
  }
  // A drag after a double click grows the selection by whole words.
  await page.mouse.click(word.x, word.y)
  await page.mouse.down({ clickCount: 2 })
  await page.mouse.move(word.x + 60, word.y, { steps: 5 })
  await page.mouse.up({ clickCount: 2 })
  await until(async () => (await range())?.[1] > 5)
  const dragged = await range()
  step('a drag after a double click grows by words', dragged?.[0] === 0 && [10, 16].includes(dragged?.[1]), JSON.stringify(dragged))
  await page.waitForTimeout(600)

  // A format chosen at a caret goes to the next typing, and the panel shows it.
  const end = toScreen(x0 + 290, y0 + 8)
  await page.mouse.click(end.x, end.y)
  await page.keyboard.press('End')
  await until(async () => same(await range(), [16, 16]))
  const underline = kit.getByTestId('layout-char-underline')
  await underline.click()
  step('Underline with a caret shows in the panel', await until(async () => (await underline.getAttribute('aria-pressed')) === 'true'))
  await page.keyboard.type('U', { delay: 30 })
  await until(async () => (await storyText(story))?.startsWith('Alpha beta gammaU'))
  await page.waitForTimeout(1300)

  // An input method: marked text in place, no step; the committed text one step; a cancel nothing.
  const sink = kit.getByTestId('kit-text-sink')
  const compose = (type, data, value) =>
    sink.evaluate(
      (element, [kind, text, field]) => {
        if (field !== null) element.value = field
        element.dispatchEvent(new CompositionEvent(kind, { bubbles: true, data: text }))
      },
      [type, data, value],
    )
  const stepsBefore = await history()
  await compose('compositionstart', '', null)
  await compose('compositionupdate', 'ni', 'ni')
  step('marked text shows in the text at the caret', await until(async () => (await storyText(story))?.startsWith('Alpha beta gammaUni')), await storyText(story))
  step('marked text is no undo step', (await history()) === stepsBefore, `${await history()} vs ${stepsBefore}`)
  const spot = await sink.evaluate((element) => ({ caret: element.dataset.caret ?? null, ...element.getBoundingClientRect().toJSON() }))
  const canvasRect = await kit.getByTestId('kit-canvas-surface').evaluate((element) => element.getBoundingClientRect().toJSON())
  const inside = spot.caret !== null && spot.left >= canvasRect.left && spot.left <= canvasRect.right && spot.top >= canvasRect.top && spot.top <= canvasRect.bottom
  step('the hidden field sits at the caret, for the candidate window', inside, JSON.stringify({ spot, canvasRect }))
  await compose('compositionupdate', '你', '你')
  await compose('compositionend', '你', '你')
  step('the committed text replaces the marked text', await until(async () => (await storyText(story))?.startsWith('Alpha beta gammaU你\n')), await storyText(story))
  await page.waitForTimeout(1300)
  step('the committed text is one undo step', (await history()) === stepsBefore + 1, `${await history()} vs ${stepsBefore}`)
  await compose('compositionstart', '', null)
  await compose('compositionupdate', 'ha', 'ha')
  await until(async () => (await storyText(story))?.includes('你ha'))
  await compose('compositionend', '', '')
  step('a cancelled composition leaves the text as it was', await until(async () => (await storyText(story))?.startsWith('Alpha beta gammaU你\n')), await storyText(story))
  step('and no undo step', (await history()) === stepsBefore + 1, `${await history()}`)

  // Alt+Right kerns the pair at the caret (between "Al").
  const al = toScreen(x0 + 2, y0 + 8)
  await page.mouse.click(al.x, al.y)
  await page.keyboard.press('Home')
  await page.keyboard.press('ArrowRight')
  await until(async () => same(await range(), [1, 1]))
  await page.keyboard.press('Alt+ArrowRight')
  await page.waitForTimeout(400)
  // Checked after the clicks above: the checks select text as an agent.
  const typedU = await attrsAt(story, 16, 17)
  step('Underline then typing underlines the typed text', typedU?.chars?.underline === true, JSON.stringify(typedU?.chars?.underline))
  const before = await attrsAt(story, 15, 16)
  step('the text before it stays as it was', before?.chars?.underline === false)
  const kern = await attrsAt(story, 0, 1)
  step('Alt+Right kerns the pair at the caret', kern?.chars?.kerning?.kind === 'manual' && kern.chars.kerning.value === 20, JSON.stringify(kern?.chars?.kerning))

  // A click in an empty frame makes it a text frame.
  const empty = await one('frame.add', { rect: [x0, y0 + 200, x0 + 150, y0 + 260], content: 'unassigned' })
  await tool('selection')
  await tool('type')
  const inEmpty = toScreen(x0 + 40, y0 + 230)
  await page.mouse.click(inEmpty.x, inEmpty.y)
  step('a click in an empty frame makes it a text frame', await until(async () => (await details()).stories.some((item) => item.frames.includes(empty.id))), JSON.stringify({ tool: (await details()).tool, selection: (await details()).selection, empty }))
  await page.keyboard.type('E', { delay: 30 })
  await page.waitForTimeout(1300)

  // Tables: a 3 × 3 table in a new frame.
  const made = (await agent([{ command: 'frame.add', params: { rect: [x0, y0 + 300, x0 + 300, y0 + 450], content: 'text', text: '', caret: true } }, { command: 'table.insert', params: { rows: 3, cols: 3, width: 240 } }])).completed
  const tableFrame = made[0]?.result
  const tableStory = (await details()).stories.find((item) => item.frames.includes(tableFrame.id)).id
  const tableId = made[1]?.result?.table
  const tableOf = async () => one('table.get', { story: tableStory, table: tableId })
  const widths = async () => (await tableOf())?.columns?.map((c) => Math.round(c.width * 100) / 100)
  step('the table is there', same(await widths(), [80, 80, 80]), JSON.stringify(await widths()))
  // Typing in cells: Tab goes on, Shift+Tab back.
  const cell0 = toScreen(x0 + 20, y0 + 306)
  await page.mouse.click(cell0.x, cell0.y)
  await until(async () => (await textSel())?.cell != null)
  await page.keyboard.type('A', { delay: 30 })
  await page.keyboard.press('Tab')
  await page.keyboard.type('B', { delay: 30 })
  await page.keyboard.press('Shift+Tab')
  await page.keyboard.press('End')
  await page.keyboard.type('a', { delay: 30 })
  const cellText = async () => (await tableOf())?.cells?.slice(0, 2).map((c) => c.text)
  step('Tab and Shift+Tab move between cells', await until(async () => same(await cellText(), ['Aa', 'B'])), JSON.stringify(await cellText()))
  // Esc selects the cell, Esc again its text.
  await page.keyboard.press('Escape')
  step('Esc in a cell selects the cell', await until(async () => (await details()).selection.cells != null), JSON.stringify((await details()).selection))
  await page.keyboard.press('Escape')
  step('Esc again selects the cell\'s text', await until(async () => (await details()).selection.cells == null && same(await range(), [0, 2])), JSON.stringify((await details()).selection))
  // The Table group in Properties.
  const rowHeight = kit.getByTestId('layout-row-height').locator('input').first()
  step('the Table group shows with the caret in a table', await until(async () => (await rowHeight.count()) > 0))
  await rowHeight.fill('30pt')
  await rowHeight.press('Enter')
  step('the Table group sets the row height', await until(async () => (await tableOf())?.rows?.[0]?.height === 30), JSON.stringify((await tableOf())?.rows?.[0]))
  await page.waitForTimeout(1300)
  // Dragging the first column's border.
  toScreen = await place()
  const zoom = (await view()).zoom
  const widthsBefore = (await tableOf()).columns.map((c) => c.width)
  const border = toScreen(x0 + widthsBefore[0], y0 + 306)
  const stepsTable = await history()
  await page.mouse.move(border.x, border.y)
  await page.mouse.down()
  await page.mouse.move(border.x + 15 * zoom, border.y, { steps: 3 })
  await page.mouse.move(border.x + 30 * zoom, border.y, { steps: 3 })
  await page.mouse.up()
  const resized = async () => (await tableOf()).columns.map((c) => c.width)
  step('dragging a column border resizes the column', await until(async () => Math.abs((await resized())[0] - widthsBefore[0] - 30) < 1.5), JSON.stringify(await resized()))
  step('the drag is one undo step', (await history()) === stepsTable + 1, `${await history()} vs ${stepsTable}`)
  const afterOne = await resized()
  const border2 = toScreen(x0 + afterOne[0], y0 + 306)
  await page.mouse.move(border2.x, border2.y)
  await page.keyboard.down('Shift')
  await page.mouse.down()
  await page.mouse.move(border2.x - 10 * zoom, border2.y, { steps: 3 })
  await page.mouse.move(border2.x - 20 * zoom, border2.y, { steps: 3 })
  await page.mouse.up()
  await page.keyboard.up('Shift')
  const total = (list) => list.reduce((a, b) => a + b, 0)
  step('Shift-dragging keeps the table width', await until(async () => Math.abs((await resized())[0] - afterOne[0] + 20) < 1.5 && Math.abs(total(await resized()) - total(afterOne)) < 0.01), JSON.stringify(await resized()))

  await page.mouse.click(cell0.x, cell0.y)
  const columns = kit.getByTestId('layout-table-columns').locator('input').first()
  await columns.fill('4')
  await columns.press('Enter')
  step('the Table group adds a column', await until(async () => (await tableOf())?.columns?.length === 4), `${(await tableOf())?.columns?.length}`)
  // Bullets: the key switches them on and off; the Home tab's button numbers.
  const para = toScreen(x0 + 12, y0 + 8)
  await page.mouse.click(para.x, para.y)
  await until(async () => (await textSel())?.story === story)
  const listOf = async () => (await attrsAt(story, 2, 2))?.para?.listType
  await page.keyboard.press('ControlOrMeta+Alt+Shift+B')
  step('the bullets key puts bullets on the paragraph', await until(async () => (await listOf()) === 'bullets'), `${await listOf()}`)
  await page.mouse.click(para.x, para.y)
  await until(async () => (await textSel())?.story === story)
  await page.keyboard.press('ControlOrMeta+Alt+Shift+B')
  step('and again takes them off', await until(async () => (await listOf()) === 'none'), `${await listOf()}`)
  await page.mouse.click(para.x, para.y)
  await until(async () => (await textSel())?.story === story)
  const numbers = kit.getByTestId('layout-home-numbers').first()
  step('the Home tab has a numbering button', (await numbers.count()) > 0)
  if (await numbers.count()) await numbers.click()
  step('the Home tab\'s button numbers the paragraph', await until(async () => (await listOf()) === 'numbers'), `${await listOf()}`)
  await page.keyboard.press('Escape')

  // Threads: with the loaded text cursor the linked frame shows unthread, and a click breaks it.
  const a = await one('frame.add', { rect: [x0 + 320, y0, x0 + 420, y0 + 60], content: 'text', text: 'Words keep coming. '.repeat(20), caret: false })
  const b = await one('frame.add', { rect: [x0 + 320, y0 + 100, x0 + 420, y0 + 160], content: 'text', text: '', caret: false })
  await one('frame.thread', { from: a.id, to: b.id })
  await tool('selection')
  toScreen = await place()
  const inA = toScreen(x0 + 360, y0 + 20)
  await page.mouse.click(inA.x, inA.y)
  await until(async () => same((await details()).selection.items, [a.id]))
  const port = toScreen(x0 + 420, y0 + 60 - 12 / (await view()).zoom)
  await page.mouse.click(port.x, port.y)
  step('the out port loads the text cursor', await until(async () => (await details()).tool === 'loadedText'), (await details()).tool)
  const overB = toScreen(x0 + 370, y0 + 130)
  await page.mouse.move(overB.x - 2, overB.y)
  await page.mouse.move(overB.x, overB.y)
  const cursor = () => kit.getByTestId('kit-canvas-surface').evaluate((element) => element.style.cursor)
  step('over the linked frame the cursor shows unthread', await until(async () => (await cursor()).includes('no-drop')), (await cursor()).slice(-40))
  await page.mouse.click(overB.x, overB.y)
  const framesOf = async () => (await details()).stories.find((item) => item.frames.includes(a.id))?.frames
  step('a click on the linked frame breaks the thread', await until(async () => same(await framesOf(), [a.id])), JSON.stringify(await framesOf()))

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
