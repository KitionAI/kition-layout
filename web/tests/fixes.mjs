// The walkthrough's fixes (plan 2026-11-25, phase 9): node tests/fixes.mjs [chromium|webkit] [section…]
// Opens dev.html in Chinese with the menu bar and checks, as a person works, what the hands-on
// walkthrough after the eight phases found broken: the keyboard comes back to the editor after
// every kind of dialog closes (OK, Cancel, Esc) and after Esc out of text; and the sections
// below, one per fix; and phase 10's (a width typed right after selecting, Redo right after Undo,
// spelling's Skip and Ignore All kept, a new style named in the list, the dock's rows). A section's
// name after the browser runs that section alone.
import './platform.mjs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const only = new Set(process.argv.slice(3))
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const MOD = process.platform === 'darwin' ? 'Meta' : 'Control'
let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}

/** Opens dev.html (Chinese, the menu bar) at a size and gives the helpers a test uses. */
async function opened({ width = 1440, height = 900, query = '', countRenders = false, slowQuery = '', slowBy = 0 } = {}) {
  const context = await browser.newContext({ viewport: { width, height } })
  // The engine's answers to one query command come `slowBy` ms late (a busy engine), the rest at once.
  if (slowQuery)
    await context.addInitScript(([command, delay]) => {
      const Original = window.Worker
      if (!Original) return
      window.Worker = class extends Original {
        constructor(...args) {
          super(...args)
          const slow = new Set()
          const post = this.postMessage.bind(this)
          this.postMessage = (message, transfer) => {
            if (JSON.stringify(message?.request ?? '').includes(command)) slow.add(message.id)
            return post(message, transfer)
          }
          // A late answer holds the ones after it: the worker answers in order.
          const held = []
          let until = 0
          const flush = () => {
            while (held.length && performance.now() >= until) {
              const [listener, event] = held.shift()
              listener.call(this, event)
            }
          }
          const late = (listener) => (event) => {
            if (slow.delete(event.data?.id)) until = performance.now() + delay
            if (!held.length && performance.now() >= until) return listener.call(this, event)
            held.push([listener, event])
            setTimeout(flush, Math.max(0, until - performance.now()) + 1)
          }
          const add = this.addEventListener.bind(this)
          this.addEventListener = (type, listener, options) => add(type, type === 'message' ? late(listener) : listener, options)
          let handler = null
          Object.defineProperty(this, 'onmessage', {
            get: () => handler,
            set: (listener) => {
              handler = listener
              add('message', late((event) => handler?.(event)))
            },
          })
        }
      }
    }, [slowQuery, slowBy])
  // The engine's pictures, counted by size: each `render` request to the engine's worker.
  if (countRenders)
    await context.addInitScript(() => {
      window.__renders = []
      const Original = window.Worker
      if (!Original) return
      window.Worker = class extends Original {
        constructor(...args) {
          super(...args)
          const post = this.postMessage.bind(this)
          this.postMessage = (message, transfer) => {
            const request = message?.request
            if (request?.method === 'render') window.__renders.push([request.width, request.height])
            return post(message, transfer)
          }
        }
      }
    })
  context.setDefaultTimeout(15000)
  const page = await context.newPage()
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  const until = async (check, tries = 80) => {
    for (let i = 0; i < tries; i += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(100)
    }
    return false
  }
  const kit = page.frameLocator('#kit')
  const frame = () => page.frames().find((f) => f !== page.mainFrame())
  const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
  const agent = (steps) => page.evaluate(async (list) => window.devHost.connection().ai.run(list, false), steps)
  const one = async (command, params = {}) => (await agent([{ command, params }])).completed[0]?.result
  const history = async () => (await one('file.history'))?.undo?.length ?? -1
  const flat = (list) => list.flatMap((item) => [item, ...flat(item.children ?? [])])
  const items = async () => (await details()).spreads.flatMap((spread) => flat(spread.items))
  const itemOf = async (id) => (await items()).find((item) => item.id === id)
  const view = async () => (await kit.getByTestId('layout-workspace').getAttribute('data-view') ?? '1 0 0').split(' ').map(Number)
  /** A point of spread `s` (points from its left page's left edge) on the screen. */
  const toScreen = async (x, y, s = 0) => {
    const d = await details()
    const placed = d.canvas.spreads[s]
    const left = Math.min(...d.spreads[s].pages.map((p) => p.bounds[0]))
    const [zoom, vx, vy] = await view()
    const box = await kit.getByTestId('layout-canvas-region').boundingBox()
    return { x: box.x + (placed.x + x - left - vx) * zoom, y: box.y + (placed.y + y - vy) * zoom, zoom }
  }
  /** Opens a menu of the menu bar and a row in it (through its submenus). */
  const menu = async (name, ...rows) => {
    await kit.getByTestId(`layout-menu-${name}`).click()
    for (const row of rows) {
      const item = row.startsWith('sub:') ? kit.getByRole('menuitem', { name: row.slice(4), exact: true }) : kit.getByTestId(row)
      if (row.startsWith('sub:')) await item.hover()
      else await item.click()
    }
  }
  const status = () => page.locator('#status').textContent()
  const tool = async () => (await details()).tool
  await page.goto(`${origin}/dev.html?modes=menus&locale=zh-CN${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1200)
  await kit.locator('.kui-app').focus()
  const region = await kit.getByTestId('layout-canvas-region').boundingBox()
  await page.mouse.click(region.x + 20, region.y + 20)
  await page.waitForTimeout(300)
  return { context, page, kit, frame, errors, until, details, agent, one, history, items, itemOf, toScreen, menu, status, tool, region }
}

/** The sections, in the order of the walkthrough's list. */
const SECTIONS = {
  /** 1. The keyboard comes back after every kind of dialog closes, and after Esc out of text. */
  async focus() {
    const w = await opened()
    const { page, kit, until, details, menu, status, tool, region } = w
    try {
      /** Makes the document dirty (a rectangle drawn by hand), with the keyboard on the canvas. */
      const dirty = async () => {
        await kit.locator('.kui-app').focus()
        await page.keyboard.press('m')
        await until(async () => (await tool()) === 'rectangle')
        const a = await w.toScreen(100, 700)
        await page.mouse.move(a.x, a.y)
        await page.mouse.down()
        await page.mouse.move(a.x + 20, a.y + 15, { steps: 4 })
        await page.mouse.up()
        await page.keyboard.press('v')
        await until(async () => (await tool()) === 'selection')
        await until(async () => /unsaved/.test(await status()))
      }
      /** After a dialog closed, with no click: a tool key and Cmd+S reach the editor. */
      const keysWork = async (label) => {
        await page.waitForTimeout(250)
        await page.keyboard.press('m')
        const toolKey = await until(async () => (await tool()) === 'rectangle', 30)
        await page.keyboard.press('v')
        await until(async () => (await tool()) === 'selection', 30)
        await page.keyboard.press(`${MOD}+s`)
        const saved = await until(async () => !/unsaved/.test(await status()), 40)
        const active = await w.frame().evaluate(() => document.activeElement?.className?.toString() ?? '')
        step(`${label}: a tool key and Cmd+S work with no click`, toolKey && saved, `tool key ${toolKey}, saved ${saved}, focus on .${active}`)
      }
      const closed = (testId) => until(async () => (await kit.getByTestId(testId).count()) === 0)
      const cancel = (testId) => kit.getByTestId(testId).getByRole('button', { name: '取消', exact: true }).click()
      const kinds = [
        ['Document Setup', 'layout-setup-dialog', () => menu('File', 'layout-menu-document-setup'), { ok: () => kit.getByTestId('layout-setup-ok').click() }],
        ['Export', 'layout-export-dialog', () => menu('File', 'layout-menu-export'), { ok: async () => { await kit.getByTestId('layout-export-format').selectOption('pdf'); await kit.getByTestId('layout-export-ok').click() } }],
        ['Margins and Columns', 'layout-dialog', () => menu('Layout', 'layout-cmd-layout.marginsAndColumns'), { ok: () => kit.getByTestId('layout-dialog-ok').click() }],
        ['Check Spelling', 'layout-spelling', () => menu('Edit', 'sub:拼写检查', 'layout-cmd-spelling.check'), { close: () => kit.getByTestId('layout-spelling-close').click() }],
        ['Preflight', 'layout-preflight', () => menu('File', 'layout-cmd-preflight.run'), { close: () => kit.getByTestId('layout-preflight-done').click() }],
      ]
      for (const [name, testId, show, ways] of kinds) {
        // Esc where a person presses it: in the dialog they work in (a modal one has the keyboard
        // when it opens; a modeless one, as Check Spelling, takes Esc from its own field or button).
        const esc = async () => {
          if (testId === 'layout-spelling') await kit.getByTestId('layout-spelling-to').click()
          else if (ways.close) await kit.getByTestId(`${testId}-done`).focus()
          await page.keyboard.press('Escape')
        }
        const closes = { ...ways, ...(ways.ok ? { cancel: () => cancel(testId) } : {}), esc }
        for (const [how, close] of Object.entries(closes)) {
          await dirty()
          await show()
          if (!(await until(async () => (await kit.getByTestId(testId).count()) === 1))) {
            step(`${name} opens`, false)
            continue
          }
          await page.waitForTimeout(300)
          await close()
          step(`${name} closed by ${how}: the dialog is gone`, await closed(testId))
          // Export's OK writes the file first.
          await until(async () => (await kit.getByTestId(testId).count()) === 0, 100)
          await keysWork(`${name} closed by ${how}`)
        }
      }
      // Esc out of text: the letter tool keys work at once.
      await kit.locator('.kui-app').focus()
      await page.keyboard.press('t')
      await until(async () => (await tool()) === 'type')
      const at = await w.toScreen(300, 187)
      await page.mouse.click(at.x, at.y)
      await until(async () => (await details()).selection.text != null)
      await page.keyboard.press('Escape')
      await until(async () => (await tool()) === 'selection')
      await page.keyboard.press('m')
      step('after Esc out of text, M picks the Rectangle tool with no click', await until(async () => (await tool()) === 'rectangle', 30), await tool())
      await page.keyboard.press('t')
      step('… and T the Type tool', await until(async () => (await tool()) === 'type', 30), await tool())
      void region
      step('focus: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * 2. Typing at the start of a long threaded story: the Pages panel's thumbnails wait for a
   * pause and only those in view are drawn (each change drew every page's before, and with
   * every key slower than their delay, all of them after every key), and the text arrives.
   */
  async typing() {
    const w = await opened({ countRenders: true })
    const { page, kit, until, details, agent, one, tool } = w
    try {
      const frame = (await agent([{ command: 'frame.add', params: { spread: 0, rect: [36, 36, 576, 756], content: 'text', caret: false } }])).completed[0].result
      await agent([{ command: 'select.set', params: { ids: [frame.id] } }, { command: 'object.primaryTextFrame', params: { ids: [frame.id], on: true } }])
      const para = 'The aurora is a natural light display in the sky, seen mostly in high-latitude regions. It forms when the solar wind disturbs the magnetosphere. '
      const long = Array.from({ length: 300 }, (_, i) => `${i + 1}. ${para}${para}`).join('\n')
      await agent([{ command: 'text.select', params: { story: frame.story, anchor: 0, focus: 0 } }, { command: 'text.insert', params: { text: long } }, { command: 'select.set', params: { ids: [] } }])
      const pages = (await details()).pageCount
      step('a long threaded story over many pages', pages >= 15, `${pages} pages`)
      await kit.getByTestId('layout-dock-tab-pages').click().catch(() => undefined)
      await page.waitForTimeout(1500)
      const thumbs = await kit.locator('.layout-thumb').count()
      // The Type tool's caret at the story's start, by a click.
      await kit.locator('.kui-app').focus()
      await page.keyboard.press('t')
      await until(async () => (await tool()) === 'type')
      const at = await w.toScreen(80, 46)
      await page.mouse.click(at.x, at.y)
      await until(async () => (await details()).selection.text?.story === frame.story)
      await page.keyboard.press(`${MOD}+ArrowUp`)
      await until(async () => (await details()).selection.text?.focus === 0)
      await page.waitForTimeout(1500)
      await w.frame().evaluate(() => (window.__renders = []))
      const typed = 'Typing at the start of a long story. '
      await page.keyboard.type(typed, { delay: 60 })
      const arrived = await until(async () => ((await one('story.get', { story: frame.story }))?.text ?? '').startsWith(typed), 200)
      step('the typed text arrives in the story', arrived)
      await page.waitForTimeout(2000)
      const renders = await w.frame().evaluate(() => window.__renders)
      const thumbRenders = renders.filter(([width, height]) => Math.max(width, height) <= 92).length
      const inView = await kit.locator('.layout-thumb').evaluateAll((list) => list.filter((canvas) => {
        const box = canvas.getBoundingClientRect()
        const panel = canvas.closest('.kui-panel-body, .layout-side, [role="tabpanel"]')?.getBoundingClientRect() ?? { top: 0, bottom: window.innerHeight }
        return box.bottom > panel.top - 64 && box.top < panel.bottom + 64 && box.height > 0
      }).length)
      step(`typing ${typed.length} characters draws only the thumbnails in view, after a pause`, thumbRenders <= Math.max(1, inView) * 2, `${thumbRenders} thumbnail pictures for ${thumbs} thumbnails, ${inView} in view`)
      step('typing: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * 4. At 1440 × 900 Layers is taller (it had 113 pixels, two rows), and an object's row dragged
   * in Layers reaches a layer out of view: held at the list's edge, the list scrolls. (Five groups
   * share the column: existing tests use Pages and Layers side by side.)
   */
  async panels() {
    const w = await opened()
    const { page, kit, until, details, itemOf, agent } = w
    try {
      // Enough objects that the Layers list scrolls.
      await agent(Array.from({ length: 16 }, (_, i) => ({ command: 'frame.add', params: { spread: 0, rect: [40 + i * 8, 600, 70 + i * 8, 630], content: 'unassigned' } })))
      await kit.getByTestId('layout-dock-tab-layers').click()
      await page.waitForTimeout(400)
      const bodies = await kit.locator('.kui-dock-group').evaluateAll((list) => list.map((group) => ({ tab: group.querySelector('[aria-selected="true"]')?.getAttribute('data-testid') ?? '', body: group.querySelector('.kui-dock-body')?.getBoundingClientRect().height ?? 0 })))
      const layers = bodies.find((group) => group.tab === 'layout-dock-tab-layers')
      step('the Layers panel is tall enough for several rows', (layers?.body ?? 0) >= 150, JSON.stringify(bodies))
      step('… and every group shown has room for its panel', bodies.every((group) => group.body >= 70), JSON.stringify(bodies))
      // A new layer (at the top of the list), then the list scrolled to its end, the layer out of view.
      const before = (await details()).layers.map((layer) => layer.id)
      await kit.getByTestId('layout-new-layer').click()
      await until(async () => (await details()).layers.length === before.length + 1)
      const added = (await details()).layers.find((layer) => !before.includes(layer.id))
      const objects = (await details()).spreads[0].items.filter((item) => item.layer !== added.id)
      const list = kit.getByTestId('layout-layer-list')
      // The scrolling list the rows are in (the list itself, or the panel around it).
      // (On the page: the frame's rectangle is inside the editor's frame.)
      const offset = (await page.locator('#kit').boundingBox())?.y ?? 0
      const scroller = async (how) => list.evaluate((element, [what, dy]) => {
        let at = element
        while (at && !(/(auto|scroll)/.test(getComputedStyle(at).overflowY) && at.scrollHeight > at.clientHeight)) at = at.parentElement
        if (!at) return null
        if (what === 'end') at.scrollTop = at.scrollHeight
        const box = at.getBoundingClientRect()
        return { top: box.top + dy, bottom: box.bottom + dy, scrollTop: at.scrollTop }
      }, [how, offset])
      const scrolled = await scroller('end')
      await page.waitForTimeout(300)
      const targetBox = async () => ((await kit.getByTestId(`layout-layer-${added.id}`).count()) ? kit.getByTestId(`layout-layer-${added.id}`).boundingBox({ timeout: 500 }).catch(() => null) : null)
      const visibleRow = async () => {
        for (const item of objects.slice().reverse()) {
          const box = (await kit.getByTestId(`layout-object-${item.id}`).count()) ? await kit.getByTestId(`layout-object-${item.id}`).boundingBox({ timeout: 500 }).catch(() => null) : null
          if (box && (!scrolled || (box.y > scrolled.top && box.y + box.height < scrolled.bottom))) return { item, box }
        }
        return null
      }
      const source = await visibleRow()
      step('an object row is in view to drag', source != null)
      if (source) {
        const target = await targetBox()
        const hidden = Boolean(scrolled) && (!target || target.y + target.height < scrolled.top || target.y > scrolled.bottom)
        const from = { x: source.box.x + source.box.width / 2, y: source.box.y + source.box.height / 2 }
        await page.mouse.move(from.x, from.y)
        await page.mouse.down()
        await page.mouse.move(from.x + 3, from.y + 6, { steps: 3 })
        if (hidden) {
          // Held at the list's top edge until the layer comes into view.
          await page.mouse.move(from.x, scrolled.top + 4, { steps: 6 })
          await until(async () => {
            const box = await targetBox()
            const now = await scroller()
            return box != null && box.y >= now.top && box.y + box.height <= now.bottom
          }, 60)
        }
        const box = await targetBox()
        await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 6 })
        await page.mouse.up()
        step(`an object row dragged onto a layer ${hidden ? 'out of view (the list scrolls) ' : ''}goes onto it`, await until(async () => (await itemOf(source.item.id))?.layer === added.id), `layer ${(await itemOf(source.item.id))?.layer}, want ${added.id}`)
      }
      step('panels: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * 5 and 6. Text by hand: after five clicks select the story, a new drag in it selects from its
   * press to its release (it began a drag-and-drop move that dropped back in itself and kept the
   * story); Table › Convert Text to Table leaves the caret in the first cell, so typing goes there.
   */
  async text() {
    const w = await opened()
    const { page, kit, until, details, agent, one, tool, menu } = w
    try {
      const storyLength = async (story) => ((await one('story.get', { story }))?.text ?? '').length
      await kit.locator('.kui-app').focus()
      await page.keyboard.press('t')
      await until(async () => (await tool()) === 'type')
      const a = await w.toScreen(150, 187)
      await page.mouse.click(a.x, a.y)
      await until(async () => (await details()).selection.text != null)
      const story = (await details()).selection.text.story
      await page.mouse.click(a.x, a.y, { clickCount: 5, delay: 0 })
      const all = await storyLength(story)
      step('five clicks select the story', await until(async () => { const t = (await details()).selection.text; return t && Math.abs(t.focus - t.anchor) === all }), JSON.stringify((await details()).selection.text))
      await page.waitForTimeout(800)
      const from = await w.toScreen(80, 187)
      const to = await w.toScreen(260, 199)
      await page.mouse.move(from.x, from.y)
      await page.mouse.down()
      await page.mouse.move(to.x, to.y, { steps: 6 })
      await page.mouse.up()
      const picked = async () => { const t = (await details()).selection.text; return t ? Math.abs(t.focus - t.anchor) : -1 }
      step('a new drag in the selected story selects from its press to its release', await until(async () => { const n = await picked(); return n > 0 && n < all }), `${await picked()} of ${all} characters`)
      step('… and moves no text', (await storyLength(story)) === all)

      // Convert Text to Table: a frame of tab-separated text, selected by hand.
      await page.keyboard.press('Escape')
      await kit.locator('.kui-app').focus()
      await page.keyboard.press('t')
      const p0 = await w.toScreen(100, 560)
      const p1 = await w.toScreen(320, 660)
      await page.mouse.move(p0.x, p0.y)
      await page.mouse.down()
      await page.mouse.move(p1.x, p1.y, { steps: 6 })
      await page.mouse.up()
      await until(async () => (await details()).selection.text != null)
      const made = (await details()).selection.text.story
      await agent([{ command: 'story.setText', params: { story: made, text: 'Ink\tCoverage\nPlum\t80%' } }])
      const inside = await w.toScreen(130, 575)
      await page.mouse.click(inside.x, inside.y)
      await page.keyboard.press(`${MOD}+a`)
      await until(async () => { const t = (await details()).selection.text; return t?.story === made && Math.abs(t.focus - t.anchor) > 10 })
      await menu('Table', 'layout-cmd-table.convertFromText')
      await until(async () => (await kit.getByTestId('layout-dialog').count()) === 1)
      await kit.getByTestId('layout-dialog-ok').click()
      step('Convert Text to Table leaves the caret in the first cell', await until(async () => (await details()).selection.text?.cell != null), JSON.stringify((await details()).selection.text))
      await page.keyboard.type('Black ', { delay: 30 })
      const cell = async () => {
        const at = (await details()).selection.text?.cell
        return at ? (await one('table.get', { table: at.table }))?.cells?.[0]?.text : null
      }
      step('… so typing goes into it', await until(async () => (await cell()) === 'Black Ink'), String(await cell()))
      step('text: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /** 8. The Pages panel's trash asks before it deletes a page with objects; an empty page goes at once. */
  async pages() {
    const w = await opened()
    const { page, kit, until, details } = w
    try {
      await kit.getByTestId('layout-dock-tab-pages').click()
      await kit.getByTestId('layout-page-0').click()
      const count = (await details()).pageCount
      await kit.getByTestId('layout-delete-page').click()
      const asked = await until(async () => (await kit.locator('[data-command="layout.pages.delete"]').count()) === 1, 30)
      step('the trash on a page with objects asks first', asked && (await details()).pageCount === count, `asked ${asked}, ${(await details()).pageCount} pages`)
      if (asked) {
        await kit.getByTestId('layout-dialog').getByRole('button', { name: '取消', exact: true }).click()
        await until(async () => (await kit.getByTestId('layout-dialog').count()) === 0)
        step('… and Cancel keeps the page', (await details()).pageCount === count)
      }
      // A new, empty page after the first: the trash deletes it at once.
      await kit.getByTestId('layout-add-page').click()
      await until(async () => (await details()).pageCount === count + 1)
      await kit.getByTestId('layout-page-1').click()
      await page.waitForTimeout(200)
      await kit.getByTestId('layout-delete-page').click()
      step('the trash deletes an empty page at once', await until(async () => (await details()).pageCount === count) && (await kit.getByTestId('layout-dialog').count()) === 0)
      step('pages: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * 7. File › New… in millimetres with two columns and 20 mm margins: A-Parent has them too (it
   * kept half an inch and one column), and the layout opens in millimetres (it fell back to points).
   */
  async newDocument() {
    const w = await opened()
    const { page, kit, until, details, menu } = w
    try {
      await menu('File', 'layout-menu-new')
      await until(async () => (await kit.getByTestId('layout-new-name').count()) === 1)
      const field = async (testId, value) => {
        const box = kit.getByTestId(testId)
        const input = (await box.evaluate((element) => element.tagName)) === 'INPUT' ? box : box.locator('input').first()
        await input.fill(String(value))
        await input.press('Tab')
      }
      await field('layout-new-name', 'Walk')
      await field('layout-new-columns', 2)
      await field('layout-new-margins-top', 20)
      await kit.getByTestId('layout-dialog-ok').click()
      const written = await until(async () => page.evaluate(() => [...window.devHost.files.keys()].some((name) => name.startsWith('Walk'))), 100)
      step('New Document writes the layout', written)
      const name = await page.evaluate(() => [...window.devHost.files.keys()].find((file) => file.startsWith('Walk')))
      const bytes = await page.evaluate((file) => Array.from(new Uint8Array(window.devHost.files.get(file).bytes)), name)
      const path = `${process.env.TMPDIR ?? '/tmp'}/fixes-${which}-${Date.now()}-${name}`
      const { writeFileSync } = await import('node:fs')
      writeFileSync(path, Buffer.from(bytes))
      await page.locator('#file').setInputFiles(path)
      await page.waitForFunction((file) => document.getElementById('status')?.textContent?.includes(file) && document.getElementById('status')?.textContent?.includes(' open in '), name.replace(/^.*\//, ''), { timeout: 60000 }).catch(() => undefined)
      await page.waitForTimeout(1500)
      const millimetres = 20 * 72 / 25.4
      const pageMargins = (await details()).spreads[0].pages[0].margins
      step('the pages have the margins', Math.abs((pageMargins?.top ?? 0) - millimetres) < 0.1, JSON.stringify(pageMargins))
      // A-Parent, as a person looks at it: a double-click on it in Pages edits the parents.
      await kit.getByTestId('layout-dock-tab-pages').click()
      await kit.getByTestId('layout-parent-A-Parent').dblclick()
      await until(async () => (await details()).editingParents === true)
      const parent = (await details()).spreads[0].pages[0]
      step('A-Parent has the margins and the columns', (await details()).editingParents === true && Math.abs((parent.margins?.top ?? 0) - millimetres) < 0.1 && parent.columns === 2, JSON.stringify({ margins: parent.margins, columns: parent.columns }))
      await kit.getByTestId('layout-parents-done').click()
      await until(async () => !(await details()).editingParents)
      await menu('Layout', 'layout-cmd-layout.marginsAndColumns')
      await until(async () => (await kit.getByTestId('layout-margins-unit').count()) === 1)
      const unit = await kit.getByTestId('layout-margins-unit').locator('select').first().inputValue().catch(async () => kit.getByTestId('layout-margins-unit').inputValue())
      step('the layout opens in millimetres', unit === 'mm', unit)
      await page.keyboard.press('Escape')
      step('newDocument: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /** 9. After Swatch Options changes a swatch's colour, its chips in the panels show the new colour. */
  async swatches() {
    const w = await opened()
    const { page, kit, until, details, agent } = w
    try {
      await agent([{ command: 'swatch.add', params: { name: 'Ink Test', color: '#336699' } }])
      const item = (await details()).spreads[0].items[0]
      await kit.getByTestId('layout-dock-tab-swatches').click()
      await kit.getByTestId('layout-swatch-Ink Test').dblclick()
      await until(async () => (await kit.getByTestId('layout-swatch-options').count()) > 0)
      const input = kit.getByTestId('layout-swatch-options-color-r')
      const field = (await input.evaluate((element) => element.tagName)) === 'INPUT' ? input : input.locator('input').first()
      await field.fill('255')
      await field.press('Tab')
      await page.waitForTimeout(300)
      await kit.getByTestId('layout-swatch-options-ok').click()
      await until(async () => (await details()).colors['Ink Test']?.hex === '#ff6699')
      const chip = () => kit.getByTestId('layout-swatch-Ink Test').locator('.layout-chip').evaluate((element) => getComputedStyle(element).backgroundColor)
      step("the swatch's chip in Swatches shows the new colour", await until(async () => (await chip()) === 'rgb(255, 102, 153)', 40), await chip())
      const listed = (await details()).swatchColors.find((swatch) => swatch.name === 'Ink Test')?.color
      step('… as the document reads it back', listed === '#ff6699', String(listed))
      void item
      step('swatches: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /** 11. Numbering styles in a generic dialog are in Chinese; View's Extras is 额外内容. */
  async words() {
    const w = await opened()
    const { page, kit, until, menu } = w
    try {
      await menu('Layout', 'layout-cmd-layout.section')
      await until(async () => (await kit.getByTestId('layout-dialog').count()) === 1)
      const choices = await kit.getByTestId('layout-dialog-param-style').locator('option').allTextContents().catch(() => [])
      step('Numbering & Section Options shows its numbering styles in the editor\'s words', choices.length >= 5 && !choices.some((text) => /arabic|Roman|Letters/.test(text)) && choices.includes('I, II, III, IV…'), choices.join(' | '))
      await page.keyboard.press('Escape')
      await until(async () => (await kit.getByTestId('layout-dialog').count()) === 0)
      await kit.getByTestId('layout-menu-View').click()
      const extras = await kit.getByRole('menuitem', { name: '额外内容', exact: true }).count()
      step('View › Extras reads 额外内容', extras === 1)
      await page.keyboard.press('Escape')
      step('words: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /** 13. A sideways trackpad swipe pans the canvas; a tool flyout chooses by press, drag and release. */
  async input() {
    const w = await opened()
    const { page, kit, until, tool, region } = w
    try {
      // Zoomed in, so there is room to pan sideways.
      await kit.locator('.kui-app').focus()
      await page.keyboard.press(`${MOD}+2`)
      await page.waitForTimeout(500)
      const view = async () => ((await kit.getByTestId('layout-workspace').getAttribute('data-view')) ?? '1 0 0').split(' ').map(Number)
      const before = await view()
      await page.mouse.move(region.x + region.width / 2, region.y + region.height / 2)
      await page.mouse.wheel(120, 0)
      step('a sideways swipe pans the canvas sideways', await until(async () => (await view())[1] > before[1] + 1, 30), `${before} → ${await view()}`)
      // The shapes' flyout: press the Rectangle tool, hold, move onto Ellipse, release.
      const button = kit.locator('.kui-tool-slot [data-kit-ui-id="tool:rectangle"]').first()
      const box = await button.boundingBox()
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
      await page.mouse.down()
      const opened = await until(async () => (await kit.locator('.kui-tool-flyout').count()) === 1, 30)
      const item = kit.locator('.kui-tool-flyout [data-kit-ui-id="tool:ellipse"]')
      const target = opened ? await item.boundingBox() : null
      if (target) await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 6 })
      await page.mouse.up()
      step('press, drag and release on a flyout tool chooses it', opened && (await until(async () => (await tool()) === 'ellipse', 30)), `flyout ${opened}, tool ${await tool()}`)
      step('input: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * Phase 10, 3a (the flaky width in test:flow). Right after another object is selected, a width
   * typed into Properties replaces the field's value even when the panel catches up with the new
   * object while the field is already focused (it went after the old value: 100 + 150, clamped to
   * 100000). The engine's measurements come late here, as from a busy engine.
   */
  async staleField() {
    const w = await opened({ slowQuery: 'transform.info', slowBy: 300 })
    const { page, kit, until, details, agent, itemOf } = w
    try {
      const made = (await agent([
        { command: 'frame.add', params: { spread: 0, rect: [60, 400, 100, 440], content: 'graphic' } },
        { command: 'frame.add', params: { spread: 0, rect: [200, 460, 300, 560], content: 'graphic' } },
      ])).completed.map((c) => c.result.id)
      const pick = async (id) => {
        const row = kit.getByTestId(`layout-object-name-${id}`)
        await row.scrollIntoViewIfNeeded()
        await row.click()
        await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([id]))
      }
      const named = (id) => until(async () => (await kit.getByTestId('layout-geometry').getAttribute('data-item').catch(() => null)) === String(id))
      await pick(made[0])
      step('the panel names the first object once it measured it', await named(made[0]))
      await pick(made[1])
      // A person clicks into W at once (its text selected) and types when the panel has caught up.
      const input = kit.getByTestId('layout-w').locator('input').first()
      await input.evaluate((element) => {
        element.focus()
        element.select()
      })
      await named(made[1])
      await page.keyboard.insertText('150')
      await page.keyboard.press('Enter')
      const width = async () => {
        const b = (await itemOf(made[1])).bounds
        return b[2] - b[0]
      }
      step('the width typed is the width set', await until(async () => Math.abs((await width()) - 150) < 0.01, 40), String(await width()))
      step('staleField: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * Phase 10, 3b (the flaky "Redo brings it again" in WebKit's test:preview). Redo pressed right
   * after Undo redoes, even before the editor has heard back from the Undo (it ran a key with Cmd
   * only when its last report said the command could run, and that report still said nothing to
   * redo). The engine's answer to Undo comes late here, as from a busy engine.
   */
  async redoAfterUndo() {
    const w = await opened({ slowQuery: 'edit.undo', slowBy: 400 })
    const { page, kit, until, details, agent, itemOf } = w
    try {
      const id = (await agent([{ command: 'frame.add', params: { spread: 0, rect: [100, 400, 300, 500], content: 'graphic' } }])).completed[0].result.id
      const row = kit.getByTestId(`layout-object-name-${id}`)
      await row.scrollIntoViewIfNeeded()
      await row.click()
      await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([id]))
      const x = async () => (await itemOf(id)).bounds[0]
      const x0 = await x()
      await kit.locator('.kui-app').focus()
      await page.keyboard.press('ArrowRight')
      await until(async () => (await x()) > x0 + 0.5)
      const moved = await x()
      await page.waitForTimeout(500)
      await kit.locator('.kui-app').focus()
      await page.keyboard.press('ControlOrMeta+z')
      await page.keyboard.press('ControlOrMeta+Shift+z')
      // Undo, then Redo: the object ends where the nudge put it.
      await page.waitForTimeout(1500)
      step('Redo right after Undo redoes', await until(async () => Math.abs((await x()) - moved) < 0.01, 40), `${x0} → ${moved}, now ${await x()}`)
      step('redoAfterUndo: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * Phase 10, 6. Check Spelling's Skip and Ignore All are remembered while the document is open:
   * closed and opened again, the check does not stop at them again; a new misspelling it does.
   */
  async spelling() {
    const w = await opened()
    const { page, kit, until, agent, menu } = w
    try {
      await agent([{ command: 'frame.add', params: { spread: 0, rect: [100, 600, 400, 640], content: 'text', text: 'Thw cat jumpd over Thw dog', caret: false } }])
      const word = () => kit.getByTestId('layout-spelling-word').textContent().catch(() => null)
      const done = async () => (await kit.getByTestId('layout-spelling-status').textContent()) === '拼写检查已完成。'
      const openCheck = async () => {
        await menu('Edit', 'sub:拼写检查', 'layout-cmd-spelling.check')
        await until(async () => (await kit.getByTestId('layout-spelling-word').count()) === 1 || (await done()))
      }
      await openCheck()
      // Ignore All on "jumpd", Skip on everything else, to the end.
      const seen = []
      for (let i = 0; i < 12 && !(await done()); i += 1) {
        const now = await word()
        seen.push(now)
        await kit.getByTestId(now === 'jumpd' ? 'layout-spelling-ignore' : 'layout-spelling-skip').click()
        await page.waitForTimeout(200)
      }
      step('the first check walks every word to the end', seen.filter((x) => x === 'Thw').length === 2 && seen.includes('jumpd') && (await done()), JSON.stringify(seen))
      await kit.getByTestId('layout-spelling-close').click()
      await openCheck()
      step('opened again, nothing skipped or ignored comes up again', await until(done, 20), String(await word()))
      await kit.getByTestId('layout-spelling-close').click()
      // A new misspelling, and another "jumpd" (ignored by spelling): only the new word comes up.
      await agent([{ command: 'frame.add', params: { spread: 0, rect: [100, 650, 400, 690], content: 'text', text: 'zorpish and jumpd', caret: false } }])
      await openCheck()
      step('… a new misspelling does', (await word()) === 'zorpish', String(await word()))
      await kit.getByTestId('layout-spelling-skip').click()
      step('… and nothing after it', await until(done, 20), String(await word()))
      await kit.getByTestId('layout-spelling-close').click()
      step('spelling: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * Phase 10, 8. New Paragraph Style ("+") makes the style at once with its name ready to type
   * over in the list: Enter keeps the name typed, Escape the one it was given; a click on its row
   * applies it as on any row; a double-click on a style opens its options.
   */
  async styleRename() {
    const w = await opened()
    const { page, kit, until, details, agent } = w
    try {
      const frame = (await agent([{ command: 'frame.add', params: { spread: 0, rect: [100, 600, 400, 660], content: 'text', text: 'A lead paragraph.', caret: false } }])).completed[0].result
      const row = kit.getByTestId(`layout-object-name-${frame.id}`)
      await row.scrollIntoViewIfNeeded()
      await row.click()
      await until(async () => JSON.stringify((await details()).selection.items) === JSON.stringify([frame.id]))
      await kit.getByTestId('layout-dock-tab-styles').click()
      const styles = async () => (await details()).paragraphStyles
      const field = kit.getByTestId('layout-paragraph-style-rename')
      const make = async () => {
        const before = (await styles()).length
        await kit.getByTestId('layout-paragraph-style-new').click()
        await until(async () => (await styles()).length === before + 1)
        return (await styles()).at(-1)
      }
      const first = await make()
      const ready = await until(async () => (await field.count()) === 1 && (await field.evaluate((input) => input === document.activeElement && input.selectionStart === 0 && input.selectionEnd === input.value.length)), 30)
      step('after New the new style\'s name is ready to type over', ready, first)
      await page.keyboard.type('Lead In')
      await page.keyboard.press('Enter')
      step('typing and Enter rename it', await until(async () => (await styles()).includes('Lead In') && !(await styles()).includes(first)), JSON.stringify(await styles()))
      const second = await make()
      await until(async () => (await field.count()) === 1, 30)
      await page.keyboard.type('Other')
      await page.keyboard.press('Escape')
      await until(async () => (await field.count()) === 0, 30)
      step('Escape keeps the name it was given', (await styles()).includes(second) && !(await styles()).includes('Other'), JSON.stringify(await styles()))
      // As a person who makes a style and clicks its row: the row applies it.
      const third = await make()
      await kit.getByTestId(`layout-paragraph-style-${third}`).click()
      const applied = async () => (await agent([{ command: 'text.selectionAttrs', params: {} }])).completed[0].result?.paragraphStyle
      step('a click on the new style\'s row applies it', await until(async () => (await applied()) === third), String(await applied()))
      step('… and leaves the name as it was', (await field.count()) === 0 && (await styles()).includes(third))
      await kit.getByTestId('layout-paragraph-style-Lead In').dblclick()
      step('a double-click on a style opens its options', await until(async () => (await kit.getByTestId('layout-style-dialog').count()) === 1), '')
      step('styleRename: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },

  /**
   * Phase 10, 2. At 1440 × 900 Layers, Swatches and Styles each show at least six rows with no
   * resizing: the groups keep room for their panels and the column scrolls; a group collapses from
   * its tab bar, and the dock remembers it after a reload.
   */
  async dock() {
    const w = await opened()
    const { page, kit, until, details, agent } = w
    try {
      await agent(Array.from({ length: 20 }, (_, i) => ({ command: 'frame.add', params: { spread: 0, rect: [40 + i * 8, 600, 70 + i * 8, 630], content: 'unassigned' } })))
      await agent(Array.from({ length: 8 }, (_, i) => ({ command: 'style.paragraph.add', params: { name: `Body ${i + 1}` } })))
      await until(async () => (await details()).paragraphStyles.length >= 10)
      /** Rows of the group showing `tab` wholly in view (inside its body and the dock), the group brought into view as a person scrolls the column. */
      const rowsInView = async (tab) => {
        await kit.getByTestId(`layout-dock-tab-${tab}`).click()
        await page.waitForTimeout(300)
        return kit.getByTestId(`layout-dock-tab-${tab}`).evaluate((button) => {
          const group = button.closest('.kui-dock-group')
          group.scrollIntoView({ block: 'nearest' })
          const body = group.querySelector('.kui-dock-body')?.getBoundingClientRect()
          const dock = group.closest('.kui-dock').getBoundingClientRect()
          if (!body) return { rows: 0, body: 0 }
          const top = Math.max(body.top, dock.top) - 1
          const bottom = Math.min(body.bottom, dock.bottom) + 1
          const rows = [...group.querySelectorAll('.kui-dock-body .layout-row, .kui-dock-body .layout-list-item')].filter((row) => {
            const box = row.getBoundingClientRect()
            return box.height > 0 && box.top >= top && box.bottom <= bottom
          })
          return { rows: rows.length, body: Math.round(body.height) }
        })
      }
      for (const tab of ['layers', 'swatches', 'styles']) {
        const seen = await rowsInView(tab)
        step(`at 1440 × 900 ${tab} shows at least six rows without resizing`, seen.rows >= 6, JSON.stringify(seen))
      }
      // Pages and Layers stay side by side, each in its own group.
      const groups = await kit.locator('.kui-dock-group').evaluateAll((list) => list.map((group) => ({ tab: group.querySelector('[aria-selected="true"]')?.getAttribute('data-testid') ?? '', body: Math.round(group.querySelector('.kui-dock-body')?.getBoundingClientRect().height ?? 0) })))
      step('five groups, Pages and Layers each in its own', groups.length === 5 && groups.some((g) => g.tab === 'layout-dock-tab-pages') && groups.some((g) => g.tab === 'layout-dock-tab-layers'), JSON.stringify(groups))
      step('no group is squeezed to a sliver (each body at least 140 pixels)', groups.every((g) => g.body >= 140), JSON.stringify(groups))
      // A click on the empty part of Swatches' tab bar collapses it; the dock remembers it.
      const strip = kit.getByTestId('layout-dock-group-swatches-group').locator('.kui-dock-strip')
      await strip.scrollIntoViewIfNeeded()
      const box = await strip.boundingBox()
      await page.mouse.click(box.x + box.width - 48, box.y + box.height / 2)
      const collapsed = () => kit.getByTestId('layout-dock-group-swatches-group').getAttribute('data-collapsed')
      step('a click on the empty part of a tab bar collapses the group', await until(async () => (await collapsed()) === 'true'), await collapsed())
      await page.reload()
      await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
      step('… and it stays collapsed after a reload', await until(async () => (await collapsed()) === 'true'), await collapsed())
      await kit.getByTestId('layout-dock-tab-swatches').dblclick()
      step('a double-click on its tab expands it again', await until(async () => (await collapsed()) === 'false'), await collapsed())
      step('dock: no page errors', w.errors.length === 0, w.errors.join(' | '))
    } finally {
      await w.context.close()
    }
  },
}

try {
  for (const [name, run] of Object.entries(SECTIONS)) {
    if (only.size && !only.has(name)) continue
    try {
      await run()
    } catch (error) {
      failures += 1
      console.log(`FAIL ${which} ${name} threw - ${error?.stack ?? error}`)
    }
  }
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
