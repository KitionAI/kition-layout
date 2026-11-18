// Export's file handling: node --test src/exporting.test.mjs (Node strips the module's types).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { EXTENSION, FORMATS, bytesOf, bytesRequest, exportFolder, exportPath, exportStem, writeExports } from './exporting.ts'

const manifest = JSON.parse(readFileSync(new URL('../../kit.json', import.meta.url), 'utf8'))

test('kit.json declares every format the dialog writes, so the host accepts the new file', () => {
  const declared = new Map(manifest.exports.map((entry) => [entry.extension, entry]))
  for (const format of FORMATS) assert.ok(declared.has(EXTENSION[format]), `${format} is declared`)
  assert.equal(declared.get('idml').mime, 'application/vnd.adobe.indesign-idml-package')
  assert.equal(declared.get('epub').mime, 'application/epub+zip')
  assert.deepEqual(manifest.agent.export, { command: 'file.exportBytes', data: 'data', params: {} })
})

test('IDML, EPUB, PDF and HTML ask the engine for the bytes, never a path', () => {
  assert.deepEqual(bytesRequest('idml', { bleed: true }), { command: 'file.exportBytes', params: { format: 'idml' } })
  assert.deepEqual(bytesRequest('epub', { pages: '1', bleed: true }), { command: 'file.exportBytes', params: { format: 'epub' } })
  assert.deepEqual(bytesRequest('pdf', { pages: '2-3', bleed: true }), { command: 'file.exportBytes', params: { format: 'pdf', pages: '2-3', bleed: true } })
  assert.deepEqual(bytesRequest('html', { bleed: false }), { command: 'file.exportBytes', params: { format: 'html' } })
})

test('names the file after the document, next to it', () => {
  assert.equal(exportStem('Poster.klayout'), 'Poster')
  assert.equal(exportStem('.klayout'), 'Untitled')
  assert.equal(exportFolder('work/2026/Poster.klayout'), 'work/2026/')
  assert.equal(exportFolder('Poster.klayout'), '')
  const doc = { name: 'Poster.klayout', path: 'work/Poster.klayout' }
  assert.equal(exportPath(doc, 'idml'), 'work/Poster.idml')
  assert.equal(exportPath(doc, 'epub'), 'work/Poster.epub')
  assert.equal(exportPath(doc, 'jpeg', '-2'), 'work/Poster-2.jpg')
})

test('reads the file from data, base64 or text', () => {
  assert.equal(new TextDecoder().decode(bytesOf({ data: 'UEsDBA==' })), 'PK\x03\x04')
  assert.equal(new TextDecoder().decode(bytesOf({ base64: 'JVBERi0=' })), '%PDF-')
  assert.equal(new TextDecoder().decode(bytesOf({ text: '<!doctype html>' })), '<!doctype html>')
  assert.equal(bytesOf({ bytes: 4 }), null)
  assert.equal(bytesOf(null), null)
})

test('answers the paths the host wrote, and stops at its refusal', async () => {
  const files = [{ path: 'Poster.idml', bytes: new Uint8Array([80, 75]) }, { path: 'Poster.epub', bytes: new Uint8Array([80, 75]) }]
  const asked = []
  const written = await writeExports(files, async (path) => {
    asked.push(path)
    return path.replace('Poster', 'Poster 2')
  })
  assert.deepEqual(written, ['Poster 2.idml', 'Poster 2.epub'], 'the host may choose an unused name')
  await assert.rejects(
    writeExports(files, async () => {
      throw new Error('no kit edits `Poster.idml`')
    }),
    /no kit edits/,
  )
  await assert.rejects(
    writeExports(files, async () => undefined),
    /did not write Poster.idml/,
    'no success without the host answering a path',
  )
  assert.deepEqual(asked, ['Poster.idml', 'Poster.epub'])
})
