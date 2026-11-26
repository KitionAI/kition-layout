// Paths and output by hand (plan 2026-11-25, stage 8), the parts with no browser: engine errors
// worded by their kind and refusal code (never the English), preflight issues and spelling in the
// person's language, the PDF options the export sends, the new document's primary text frames,
// the Pen's drawn cursors and the Links panel's thumbnails. The engine half loads the built web
// engine and is skipped until `pnpm --dir web engine` has built it. Run with `pnpm --dir web
// test:unit` (node --test).
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'
import assert from 'node:assert/strict'
import { errorText, parseEngineError, reasonOf } from './errors.ts'
import { REASONS, refusalReason } from './menus/context.ts'
import { PREFLIGHT_KINDS, changeAllSteps, indexAfterRemoving, issueText, shownWords } from './output.ts'
import { DEFAULT_PDF, bytesRequest, pdfParams } from './exporting.ts'
import { addPrimaryTextFrames, marginBox } from './dialogs/pages.ts'
import { DRAWN_CURSORS, drawnCursor } from './cursors.ts'
import { thumbnailType } from './images.ts'
import { CATALOGS } from './strings.ts'

const engineDir = new URL('../public/engine/', import.meta.url)
const sdk = existsSync(new URL('kition_layout_kit_bg.wasm', engineDir))
  ? await (async () => {
      const module = await import(new URL('kition_layout_kit.js', engineDir).href)
      module.initSync({ module: readFileSync(new URL('kition_layout_kit_bg.wasm', engineDir)) })
      return module
    })()
  : null
const skip = sdk ? false : 'the web engine is not built'

/** The editor's strings in `locale`, with `{values}` filled in. */
const words = (locale) => (key, values = {}) => {
  const text = CATALOGS[locale][key]
  assert.ok(typeof text === 'string', `${locale} has ${key}`)
  return text.replace(/\{(\w+)\}/g, (_, name) => String(values[name] ?? `{${name}}`))
}
const zh = words('zh-CN')
const zhWords = { t: zh, title: (command) => ({ 'file.exportPdf': '导出 PDF', 'arrange.group': '编组', 'object.lock': '锁定', 'frame.add': '创建框架', 'edit.paste': '粘贴' })[command] ?? command, reasons: REASONS }
/** Latin letters in a run of three or more: English words. */
const english = /[A-Za-z]{3,}/

test('engine errors are read by their kind, command and refusal code', () => {
  assert.deepEqual(parseEngineError(new Error('invalid parameters for `object.pathfinder`: the shapes don\'t overlap: nothing is left [refusal:noOverlap]')), { kind: 'badParams', command: 'object.pathfinder', code: 'noOverlap' })
  assert.deepEqual(parseEngineError(new Error('command `table.merge` is not available right now: not in a table cell [refusal:needTable]')), { kind: 'disabled', command: 'table.merge', code: 'needTable' })
  assert.deepEqual(parseEngineError('unknown command `no.such`'), { kind: 'unknown', command: 'no.such', code: null })
  assert.deepEqual(parseEngineError(new Error('no active document')), { kind: 'noDocument', command: null, code: null })
  assert.deepEqual(parseEngineError(new Error('internal error in `x.y` (the document was kept as it was): boom')), { kind: 'internal', command: 'x.y', code: null })
  assert.deepEqual(parseEngineError(new Error('`frame.add` has no parameter `rekt`; did you mean `rect`?')), { kind: 'badParams', command: 'frame.add', code: null })
  assert.deepEqual(parseEngineError(new Error('something else')), { kind: 'other', command: null, code: null })
  // The code names the reason; an unknown code is no reason.
  assert.equal(reasonOf(new Error('x [refusal:needTwo]'), REASONS), 'needTwo')
  assert.equal(reasonOf(new Error('x [refusal:noSuchReason]'), REASONS), null)
  assert.equal(refusalReason(new Error('invalid parameters for `object.distribute`: select at least three objects [refusal:needThree]')), 'needThree')
})

test('no engine error reaches the person in English', () => {
  const errors = [
    'invalid parameters for `file.exportPdf`: bad `pages`: 9-12 [refusal:badPageRange]',
    'invalid parameters for `file.exportPdf`: unknown standard `x9` (none, x4, a2b)',
    'invalid parameters for `arrange.group`: select at least two objects [refusal:needTwo]',
    'command `edit.paste` is not available right now: clipboard is empty [refusal:needClipboard]',
    'command `object.lock` is not available right now: select something',
    'unknown command `no.such`',
    'no active document',
    'internal error in `file.exportPdf` (the document was kept as it was): index out of bounds',
    'PDF writer error: Pdf(VersionMismatch(Pdf17))',
    '`frame.add` has no parameter `rekt`',
  ]
  for (const message of errors) {
    const text = errorText(new Error(message), zhWords)
    assert.ok(text && !english.test(text.replace(/PDF|ppi|RGB/g, '')), `${message} → ${text}`)
  }
  assert.equal(errorText(new Error(errors[0]), zhWords), zh('hint.badPageRange'))
  assert.equal(errorText(new Error(errors[1]), zhWords), '“导出 PDF”没有执行：设置不被接受。')
  assert.equal(errorText(new Error(errors[4]), zhWords), '“锁定”现在不能用。')
})

test('every preflight issue reads in both languages from its details', () => {
  const issues = [
    { severity: 'error', kind: 'overset', message: 'Overset text: 12 characters', item: 5, page: 0, details: { chars: 12 } },
    { severity: 'error', kind: 'missingFont', message: 'Missing font: Futura', details: { font: 'Futura' } },
    { severity: 'error', kind: 'missingGlyphs', message: 'Missing glyphs', details: { font: 'Inter Regular', missing: ['你', '好'], more: 0 } },
    { severity: 'error', kind: 'missingGlyphs', message: 'Missing glyphs', details: { font: 'Inter Regular', missing: ['你'], more: 3 } },
    { severity: 'error', kind: 'missingLink', message: 'Missing graphic', item: 3, page: 1 },
    { severity: 'error', kind: 'missingLink', message: 'Missing link: a.png', item: 3, details: { name: 'a.png' } },
    { severity: 'warning', kind: 'lowResolution', message: 'a.png: effective 72 ppi (< 150)', item: 3, details: { name: 'a.png', ppi: 72, minPpi: 150 } },
    { severity: 'warning', kind: 'emptyText', message: 'Empty text frame', item: 4 },
    { severity: 'warning', kind: 'rgbColor', message: 'RGB swatch in a print document: Sky', details: { swatch: 'Sky' } },
    { severity: 'warning', kind: 'unsupportedTypography', message: 'Mojikumi `x` is preserved' },
    { severity: 'warning', kind: 'futureKind', message: 'Something new' },
  ]
  for (const issue of issues) {
    for (const locale of ['zh-CN', 'en-US']) {
      const text = issueText(issue, words(locale))
      assert.ok(text && !text.includes('{'), `${locale} ${issue.kind}: ${text}`)
      if (locale === 'zh-CN') assert.ok(!english.test(text.replace(/Futura|Inter Regular|a\.png|Sky|ppi|RGB/g, '')), `${issue.kind}: ${text}`)
    }
  }
  assert.equal(issueText(issues[0], zh), '溢流文本：12 个字符没有排下')
  assert.equal(issueText(issues[6], zh), 'a.png：有效分辨率 72 ppi，低于 150 ppi')
  assert.ok(PREFLIGHT_KINDS.every((kind) => issues.some((issue) => issue.kind === kind)))
})

test('Check Spelling walks on past what it changed, ignored or added; Change All is each story from its end', () => {
  const words = [
    { story: 1, start: 0, end: 5, word: 'teh', suggestions: ['the'], frame: 9 },
    { story: 1, start: 10, end: 13, word: 'wrod', suggestions: ['word'], frame: 9 },
    { story: 1, start: 20, end: 23, word: 'teh', suggestions: ['the'], frame: 9 },
    { story: 2, start: 4, end: 7, word: 'teh', suggestions: ['the'], frame: 11 },
  ]
  assert.deepEqual(shownWords(words, new Set(['teh'])).map((w) => w.word), ['wrod'])
  // At "wrod" (index 1) after every "teh" went: one word before it stays.
  assert.equal(indexAfterRemoving(words, 1, 'TEH'), 0)
  assert.equal(indexAfterRemoving(words, 3, 'wrod'), 2)
  assert.deepEqual(changeAllSteps(words, 'teh', 'the'), [
    ['spelling.change', { story: 1, start: 20, end: 23, to: 'the' }],
    ['spelling.change', { story: 1, start: 0, end: 5, to: 'the' }],
    ['spelling.change', { story: 2, start: 4, end: 7, to: 'the' }],
  ])
})

test('the PDF options send only what the exporter reads, and nothing by default', () => {
  assert.deepEqual(pdfParams(DEFAULT_PDF), {})
  assert.deepEqual(pdfParams({ ...DEFAULT_PDF, spreads: true, cropMarks: true, pageInfo: true, standard: 'x4', compressImages: true, tagged: true }), {
    spreads: true,
    marks: { crop: true, bleed: false, pageInfo: true },
    standard: 'x4',
    compressImages: true,
    tagged: true,
  })
  assert.deepEqual(bytesRequest('pdf', { pages: '1', bleed: true, pdf: { ...DEFAULT_PDF, bleedMarks: true } }).params, { format: 'pdf', pages: '1', bleed: true, marks: { crop: false, bleed: true, pageInfo: false } })
  assert.deepEqual(bytesRequest('png', { bleed: true, pdf: { ...DEFAULT_PDF, spreads: true } }).params, { format: 'png' }, 'only PDF takes them')
})

test('a primary text frame fills the margins, the inside one by the spine', () => {
  const margins = { top: 10, bottom: 20, inside: 30, outside: 40 }
  assert.deepEqual(marginBox({ bounds: [0, 0, 600, 800], side: 'right', margins }), [30, 10, 560, 780])
  assert.deepEqual(marginBox({ bounds: [0, 0, 600, 800], side: 'left', margins }), [40, 10, 570, 780])
  assert.deepEqual(marginBox({ bounds: [600, 0, 1200, 800], side: 'right', margins }), [630, 10, 1160, 780])
})

test("New Document's primary text frame: one story in the margins that adds pages as text flows", { skip }, async () => {
  const { doc, run } = layout()
  try {
    run('layout.documentSetup', { width: 595, height: 842, pages: 1, facingPages: true, bleed: 8 })
    run('layout.marginsAndColumns', { margins: { top: 36, bottom: 36, inside: 54, outside: 36 }, columns: 2, gutter: 12 })
    await addPrimaryTextFrames(async (command, params) => doc.execute(command, JSON.stringify(params)), 2, 12)
    let info = JSON.parse(doc.inspect())
    const frame = info.spreads[0].items[0]
    assert.deepEqual([frame.kind, frame.bounds, frame.textFrame.columns], ['<text frame>', [54, 36, 559, 806], 2])
    assert.deepEqual(info.selection.items, [], 'nothing left selected')
    run('text.select', { story: frame.story, anchor: 0, focus: 0 })
    run('text.insert', { text: 'Lorem ipsum dolor sit amet. '.repeat(800) })
    info = JSON.parse(doc.inspect())
    assert.ok(info.pageCount > 1, `the primary story adds pages: ${info.pageCount}`)
    assert.equal(info.stories.find((story) => story.id === frame.story).frames.length, info.pageCount, 'a threaded frame on each page')
  } finally {
    doc.free()
  }
})

test('the Pen draws what a click does, and Links draws the types a browser shows', () => {
  assert.equal(drawnCursor('pen', 'crosshair'), DRAWN_CURSORS.pen)
  assert.equal(drawnCursor('pen', 'copy'), DRAWN_CURSORS.penAdd)
  assert.equal(drawnCursor('pen', 'no-drop'), DRAWN_CURSORS.penDelete)
  assert.equal(drawnCursor('pen', 'alias'), DRAWN_CURSORS.penClose)
  assert.equal(drawnCursor('pen', 'cell'), DRAWN_CURSORS.penContinue)
  assert.equal(drawnCursor('addAnchor', 'copy'), DRAWN_CURSORS.penAdd)
  assert.equal(drawnCursor('deleteAnchor', 'no-drop'), DRAWN_CURSORS.penDelete)
  // Other tools keep theirs: the frame tools' crosshair, the loaded text cursor's link.
  assert.equal(drawnCursor('rectangleFrame', 'crosshair'), 'crosshair')
  assert.equal(drawnCursor('loadedText', 'alias'), DRAWN_CURSORS.threadLink)
  assert.equal(thumbnailType('Photo.JPG'), 'image/jpeg')
  assert.equal(thumbnailType('logo.svg'), 'image/svg+xml')
  assert.equal(thumbnailType('scan.tif'), null)
})

/** A new layout in the web engine, with `run`. */
function layout() {
  const doc = sdk.LayoutDocument.create('Output.klayout', 612, 792)
  const run = (command, params = {}) => JSON.parse(doc.execute(command, JSON.stringify(params)) || 'null')
  return { doc, run }
}

test('the engine gives refusals codes, preflight its details and spelling its frames', { skip }, () => {
  const { doc, run } = layout()
  try {
    const frame = run('frame.add', { rect: [72, 72, 172, 92], content: 'text', text: 'Teh quick wrod. '.repeat(20), caret: false })
    const refusal = (command, params) => {
      try {
        run(command, params)
        return null
      } catch (error) {
        return error
      }
    }
    assert.equal(refusalReason(refusal('file.exportBytes', { format: 'pdf', pages: '9-12' })), 'badPageRange')
    run('select.set', { ids: [frame.id] })
    assert.equal(refusalReason(refusal('arrange.group', {})), 'needTwo')
    assert.equal(refusalReason(refusal('object.distribute', { axis: 'horizontal' })), 'needThree')
    // The PDF options the dialog sends are taken.
    const pdf = run('file.exportBytes', { format: 'pdf', spreads: true, marks: { crop: true, bleed: true, pageInfo: true }, standard: 'x4', compressImages: true, tagged: true })
    assert.ok(atob(pdf.data).startsWith('%PDF-1.6'), 'PDF/X-4 is PDF 1.6')
    const check = run('preflight.run')
    const overset = check.issues.find((issue) => issue.kind === 'overset')
    assert.equal(overset.item, frame.id)
    assert.equal(overset.page, 0)
    assert.ok(overset.details.chars > 0)
    const spelling = run('spelling.check')
    const first = spelling.find((word) => word.word === 'wrod')
    assert.equal(first.frame, frame.id, 'the frame showing it')
    assert.ok(spelling.some((word) => word.frame === null), 'an overset word has no frame')
  } finally {
    doc.free()
  }
})

test('select.set takes anchor points and the path commands continue and close one subpath', { skip }, () => {
  const { doc, run } = layout()
  try {
    const path = run('path.add', { anchors: [{ p: [100, 100] }, { p: [200, 100] }] })
    run('path.appendAnchor', { id: path.id, subpath: 0, at: 'start', anchor: { p: [50, 50] } })
    run('select.set', { ids: [path.id], anchors: [[path.id, 0, 2]] })
    assert.deepEqual(JSON.parse(doc.inspect()).selection.anchors, [[path.id, 0, 2]])
    run('path.close', { id: path.id, subpath: 0 })
  } finally {
    doc.free()
  }
})
