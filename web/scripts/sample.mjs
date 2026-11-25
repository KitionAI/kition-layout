// Makes public/sample/Sample.klayout with the engine itself: two pages, a heading and body
// text frame, a colored frame, and a second layer.
import fs from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engine = await import(pathToFileURL(`${web}public/engine/kition_layout_kit.js`).href)
await engine.default({ module_or_path: await fs.readFile(`${web}public/engine/kition_layout_kit_bg.wasm`) })
const doc = engine.LayoutDocument.create('Sample.klayout', 612, 792)
const run = (command, params = {}) => JSON.parse(doc.execute(command, JSON.stringify(params)) || 'null')
const frame = run('frame.add', { rect: [54, 72, 558, 160], content: 'text', caret: true })
run('text.insert', { text: 'Northern Lights Field Guide' })
run('text.exitToFrame')
run('frame.add', { rect: [54, 180, 558, 420], content: 'text', caret: true })
run('text.insert', { text: 'Auroras appear when charged particles from the Sun meet the upper atmosphere. Green comes from oxygen near 100 km; red from oxygen higher up.' })
run('text.exitToFrame')
run('frame.add', { rect: [54, 450, 558, 720], shape: 'rectangle', content: 'graphic' })
run('object.fill', { swatch: 'C=100 M=90 Y=10 K=0' })
run('layout.pages.insert', { count: 1 })
run('layer.new', { name: 'Notes' })
run('edit.deselectAll')
await fs.writeFile(`${web}public/sample/Sample.klayout`, doc.save())
console.log('wrote public/sample/Sample.klayout', frame ? '' : '')
