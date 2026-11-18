// Two people co-editing a layout through a real Kition Server (kition-server, in memory), each
// with the Layout engine's own replica, as the editor's runtime relays it (`@kition/sync`'s relay,
// the one @kition/embed runs): node tests/collab.mjs
//
// Cases (Kition One's managed co-editing ids): two editors converge (MC-17/18), a viewer's push is
// refused and never reaches the room (MC-02), a page offline for 31 s and edited on both sides
// converges when it reconnects (MC-16/17).
//
// Needs the engine (`pnpm engine`) and a kition-server binary: KITION_SERVER_BIN, else
// ../../kition-server/target/{release,debug}/kition-server (`cargo build -p kition-server`
// in kition-server). KITION_COLLAB_OFFLINE_MS shortens the offline spell (default 31000).
// kition-server is a private tool (not open source; docs/quality.md, Private tools). Without it the
// script runs no case: it prints `SKIP: <reason>` and a `skipped` report naming the missing tool,
// and exits 77, the skip status of the Automake and Meson test harnesses, never 0, so nothing
// counts the skip as a pass. `pnpm test:collab` (tests/private-skip.mjs) reports that as skipped.
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { createServer } from 'vite'

const web = fileURLToPath(new URL('..', import.meta.url))
const engineDir = path.join(web, 'public/engine')
const OFFLINE_MS = Number(process.env.KITION_COLLAB_OFFLINE_MS ?? 31_000)
const NAME = 'Flyer.klayout'
/** The exit status of a run that tested nothing because kition-server is missing. */
const SKIPPED = 77

function serverBinary() {
  const candidates = [process.env.KITION_SERVER_BIN, ...['release', 'debug'].map((profile) => path.join(web, '../../kition-server/target', profile, 'kition-server'))]
  return candidates.find((candidate) => candidate && existsSync(candidate)) ?? null
}

const binary = serverBinary()
if (!binary) {
  const reason = 'the private kition-server is missing (set KITION_SERVER_BIN or build it in ../../kition-server)'
  console.log(`SKIP: ${reason}`)
  console.log(JSON.stringify({ status: 'skipped', kit: 'kit.layout', missing: 'kition-server', reason, cases: [] }, null, 2))
  process.exit(SKIPPED)
}
if (!existsSync(path.join(engineDir, 'kition_layout_kit_bg.wasm'))) {
  console.error('collab: no engine in public/engine (run `pnpm engine`)')
  process.exit(1)
}

// --- Kition Server, in memory, with development tokens ---------------------------------------
const CONFIG = `listen = "127.0.0.1:0"
public_url = "http://kition-server.test"

[auth]
mode = "dev"

${['ann', 'bob', 'cat'].map((user) => `[[auth.users]]\ntoken = "dev-${user}"\nid = "dev:${user}"\nname = "${user}"\npro = true\n`).join('\n')}
[storage]
mode = "memory"
`

async function startServer() {
  const dir = mkdtempSync(path.join(tmpdir(), 'layout-collab-'))
  const config = path.join(dir, 'kition-server.toml')
  writeFileSync(config, CONFIG)
  const child = spawn(binary, ['--config', config, '--memory'], { env: { ...process.env, RUST_LOG: 'info', NO_COLOR: '1' }, stdio: ['ignore', 'pipe', 'pipe'] })
  const port = await new Promise((resolve, reject) => {
    let output = ''
    const timer = setTimeout(() => reject(new Error(`kition-server did not start:\n${output}`)), 30_000)
    const read = (chunk) => {
      output += chunk.toString()
      const match = output.replace(/\u001b\[[0-9;]*m/g, '').match(/addr=127\.0\.0\.1:(\d+)/)
      if (match) {
        clearTimeout(timer)
        resolve(Number(match[1]))
      }
    }
    child.stdout.on('data', read)
    child.stderr.on('data', read)
    child.on('exit', (code) => reject(new Error(`kition-server exited (${code}):\n${output}`)))
  })
  return {
    url: `ws://127.0.0.1:${port}/v1/sync`,
    stop: () => {
      child.kill('SIGTERM')
      rmSync(dir, { recursive: true, force: true })
    },
  }
}

// --- The engine and the sync client ------------------------------------------------------------
const vite = await createServer({ root: web, configFile: false, logLevel: 'error', server: { middlewareMode: true, hmr: false }, appType: 'custom' })
const { SyncClient, startRelay, SyncRpcError } = await vite.ssrLoadModule('@kition/sync')
const { LORO_REPLICA_VERSIONS: versions } = await vite.ssrLoadModule('@kition/sync/replicaVersions')
const engine = await import(path.join(engineDir, 'kition_layout_kit.js'))
await engine.default({ module_or_path: readFileSync(path.join(engineDir, 'kition_layout_kit_bg.wasm')) })
const { LayoutDocument } = engine

/** One person's page: a connection (whose network a test can cut), and the layout's replica. */
class Person {
  constructor(name, server) {
    this.name = name
    this.online = true
    this.sockets = new Set()
    this.listeners = new Set()
    this.client = new SyncClient({
      createTransport: (url) => {
        if (!this.online) return refused()
        const socket = new WebSocket(url)
        socket.binaryType = 'arraybuffer'
        this.sockets.add(socket)
        socket.addEventListener('close', () => this.sockets.delete(socket))
        return socket
      },
    })
    this.ready = this.client.connect(server.url, `dev-${name}`)
  }

  /** Opens the file in this person's engine, as their editor does. */
  open(bytes) {
    this.doc = LayoutDocument.open(NAME, bytes)
  }

  /** The editor runtime's replica, as `replica/*` answers it. */
  replica() {
    const doc = () => this.doc
    return {
      version: async () => doc().version().slice().buffer,
      exportChanges: async (since) => ({ changes: doc().exportChanges(new Uint8Array(since)).slice().buffer, blobs: doc().blobsSince(new Uint8Array(since)).slice().buffer }),
      importChanges: async (changes, blobs) => {
        if (blobs.byteLength) doc().addBlobs(new Uint8Array(blobs))
        return doc().importChanges(new Uint8Array(changes))
      },
      onChanged: (listener) => {
        this.listeners.add(listener)
        return () => this.listeners.delete(listener)
      },
    }
  }

  relay(docId, lastSent) {
    this.joined = new Promise((resolve) => {
      this.relayed = startRelay({ docId, replica: this.replica(), sync: this.client, versions, lastSent, onJoined: resolve, onError: (error) => console.error(`[${this.name}]`, error) })
    })
    return this.joined
  }

  /** Runs a command as the person's editor does: the engine edits, then the replica says it moved. */
  run(command, params) {
    const answer = JSON.parse(this.doc.execute(command, JSON.stringify(params)) || 'null')
    const version = this.doc.version().slice().buffer
    for (const listener of this.listeners) listener(version)
    return answer
  }

  /** What the person sees: pages, objects and stories, without the ids each replica mints. */
  view() {
    const inspected = JSON.parse(this.doc.inspect())
    const strip = (value) =>
      Array.isArray(value) ? value.map(strip) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).filter(([key]) => !['id', 'story', 'frames', 'layer', 'asset'].includes(key)).map(([key, inner]) => [key, strip(inner)])) : value
    return JSON.stringify({ spreads: strip(inspected.spreads), stories: strip(inspected.stories).map((story) => JSON.stringify(story)).sort() })
  }

  version() {
    return this.doc.version()
  }

  cut() {
    this.online = false
    for (const socket of this.sockets) socket.close()
  }

  restore() {
    this.online = true
  }

  close() {
    this.relayed?.stop()
    this.client.close()
  }
}

/** A transport that never connects, as a page without a network sees it. */
function refused() {
  const transport = { onopen: null, onclose: null, onerror: null, onmessage: null, send() {}, close() {} }
  setTimeout(() => transport.onclose?.(), 0)
  return transport
}

/** Waits until every page holds every change of the others, and they see one layout. */
async function converged(people, ms = 20_000) {
  const until = Date.now() + ms
  for (;;) {
    const held = people.every((a) => people.every((b) => versions.includes(a.version(), b.version())))
    const views = new Set(people.map((person) => person.view()))
    if (held && views.size === 1) return
    if (Date.now() > until) assert.fail(`the pages did not converge (${people.map((person) => person.name).join(', ')}): held ${held}, ${views.size} views`)
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

const frames = (person) => JSON.parse(person.doc.inspect()).spreads.reduce((count, spread) => count + spread.items.length, 0)
const texts = (person) => JSON.parse(person.doc.inspect()).stories.map((story) => story.preview)

// --- The cases ------------------------------------------------------------------------------
const server = await startServer()
const people = []
const passed = []
try {
  const file = (() => {
    const doc = LayoutDocument.create(NAME, 612, 792)
    doc.execute('frame.create', JSON.stringify({ rect: [72, 72, 540, 200], text: 'Spring concert' }))
    const bytes = doc.save()
    doc.free()
    return bytes
  })()
  const ann = new Person('ann', server)
  const bob = new Person('bob', server)
  const cat = new Person('cat', server)
  people.push(ann, bob, cat)
  await Promise.all(people.map((person) => person.ready))

  // Ann opens the file and makes its room, from her replica, as the embed's first editor does.
  ann.open(file)
  const docId = ann.doc.docId
  assert.ok(docId, 'the layout has its document id')
  const created = await ann.client.createRoom(docId, ann.doc.exportChanges(new Uint8Array()), NAME, 'kit.layout')
  await ann.relay(docId, ann.version())
  const { shareLink: viewerLink } = await ann.client.createLink(docId, 'viewer')

  // Bob opens the same file through the room's link: the file's own document, one room.
  const opened = await bob.client.openRoom(created.shareLink)
  assert.equal(opened.docId ?? docId, docId)
  bob.open(file)
  assert.equal(bob.doc.docId, docId)
  await bob.relay(docId)

  // MC-17/18: both edit at once (Kition One's edit: a text frame each), and converge.
  const before = frames(ann)
  ann.run('frame.create', { rect: [20, 20, 120, 80], text: 'ann' })
  bob.run('frame.create', { rect: [140, 20, 240, 80], text: 'bob' })
  await converged([ann, bob])
  assert.equal(frames(ann), before + 2)
  assert.ok(texts(bob).includes('ann') && texts(ann).includes('bob'))
  passed.push('two editors edit at the same time and converge (MC-17, MC-18)')

  // MC-02: a viewer follows the room, and the room refuses what a viewer pushes.
  await cat.client.openRoom(viewerLink)
  cat.open(file)
  const joined = await cat.relay(docId)
  assert.equal(joined.role, 'viewer')
  await converged([ann, bob, cat])
  const held = ann.version()
  cat.doc.execute('frame.create', JSON.stringify({ rect: [300, 300, 340, 340], text: 'cat' }))
  const refused = await cat.client.push(docId, cat.doc.exportChanges(held)).then(
    () => null,
    (error) => error,
  )
  assert.ok(refused instanceof SyncRpcError, "the room refuses a viewer's push")
  await new Promise((resolve) => setTimeout(resolve, 1000))
  assert.ok(versions.includes(held, ann.version()) && !texts(ann).includes('cat'), "a viewer's edit never reaches an editor")
  passed.push(`a viewer's edit never reaches the room (MC-02): ${refused.message}`)
  cat.close()
  people.splice(people.indexOf(cat), 1)

  // MC-16/17: Bob is offline for 31 s while both edit; he reconnects and both converge.
  bob.cut()
  ann.run('frame.create', { rect: [20, 300, 120, 360], text: 'ann while bob is away' })
  bob.run('layout.pages.insert', { count: 1 })
  bob.run('frame.create', { rect: [140, 300, 240, 360], text: 'bob offline' })
  await new Promise((resolve) => setTimeout(resolve, OFFLINE_MS))
  assert.ok(!texts(bob).includes('ann while bob is away'), 'offline, Bob has not heard of it')
  bob.restore()
  await converged([ann, bob], 90_000)
  for (const text of ['ann', 'bob', 'ann while bob is away', 'bob offline']) assert.ok(texts(ann).includes(text), `${text} is kept`)
  assert.equal(JSON.parse(ann.doc.inspect()).pageCount, 2)
  passed.push(`a page offline for ${Math.round(OFFLINE_MS / 1000)} s, edited on both sides, converges when it reconnects (MC-16, MC-17)`)
} finally {
  for (const person of people) person.close()
  server.stop()
  await vite.close()
}
console.log(JSON.stringify({ status: 'passed', kit: 'kit.layout', cases: passed }, null, 2))
process.exit(0)
