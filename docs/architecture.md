# Kition Layout architecture

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe InDesign 2026

How Kition Layout is built today. The design notes behind it are in `plan/architecture.md`
(local, gitignored); this file describes what the code on `main` does. Agents: AGENTS.md has the
rules (never crash, everything is a command, layering, clean-room).

## Workspace

A Cargo workspace: the engine crates in `crates/`, the WebAssembly binding in `bindings/wasm`,
the headless kit app in `bindings/headless`, and `xtask`. The editor in `web/` is a separate
pnpm project. Pure Rust, no C dependencies by default, `unsafe_code = "forbid"` workspace-wide.

| Layer | Crate | What it does |
|---|---|---|
| L0 | `geom` | kurbo re-export, units and measurement parsing (`1p6`, `12mm`, `3in+2p`), corner shapes, polygons, transforms, hit testing |
| L0 | `color` | CMYK/RGB/Gray (Lab in CMS), swatches, tints, gradients, blend modes, ASE, ICC colour management (`cms/`), colour harmony |
| L1 | `doc` | the document model: spreads, pages, parents, layers, items, stories (text + run lists), tables, notes, styles of five kinds, swatches, sections, variables, CJK and Arabic attributes |
| L1 | `fonts` | font database (bundled OFL fonts, system fonts, document fonts, optional kition-fonts build input), metrics, outlines (skrifa), shaping (harfrust), variable instances, font classification for menus |
| L2 | `compose` | the text engine: style resolution, shaping, Knuth–Plass and single-line breaking, Liang hyphenation, justification, tabs, keeps, columns, span/split, baseline grid, vertical justification, wrap, threading, overset, anchored objects, footnotes, tables, bidi, CJK (ruby, warichu, kinsoku), caret and hit testing |
| L2 | `images` | decoding placed graphics (PNG, JPEG, GIF, WebP, TIFF incl. CMYK, BMP, PSD composite, EPS preview) |
| L3 | `render` | document to pixels with vello_cpu (SIMD, multithreaded): items, composed text, images with mips, gradients, effects, damage regions, placed PDF via hayro |
| L3 | `pdf` | PDF export with krilla: real text with subset fonts, CMYK and spot, bleed and marks, tagged PDF, links, forms, transitions, PDF/X-4 and PDF/A-2b |
| L3 | `idml` | IDML import and export (designmap, spreads, parents, stories, styles, graphics, preferences; CJK and Arabic attributes) |
| L3 | `epub` | EPUB 3 reflowable and fixed-layout export |
| L3 | `textimport` | Word (.docx), RTF, Excel (.xlsx), Tagged Text import; text export |
| L3 | `format` | the `.klayout` container (a ZIP holding the document's content, not its history: `content.bin`, the replica's sealed content snapshot; `blobs/` the state references; `meta.json` with format, formatVersion, kit, kdm, schemaVersion and docId only) and the engine's own document snapshot (`document.json`) for the clipboard, libraries and recovery |
| L4 | `tools` | tool state machines: pointer events and modifiers to commands and overlays; snapping and smart guides |
| L4 | `kdm` | `kition-layout-kdm`: the `kit.layout` KDM schema, the bridge between the engine's `Document` and a replica, the replica session (undo, change exchange, repair, the seal a save writes) and the `.klayout` file as the document's content: opened alone or taken into the host's stored replica, which keeps the history |
| L5 | `engine` | `Session`: documents and their replicas (undo is the replica's, `replica.rs`), selection, the command registry (`cmd/`), compose cache, preflight, find/change, scripts, recovery, links |
| L6 | `kition-layout-kit` | Kition's in-page Layout engine: the engine and its tools behind the Kition SDK's `KitDocument` contract, compiled to WebAssembly for the in-page editor and linked into `bindings/headless` for the headless agent executable |

`cargo xtask layers` enforces the layering: a crate depends only on lower layers (with the
intra-layer orders geom → color and doc → fonts); L0–L6 compile for `wasm32-unknown-unknown`
(`cargo xtask wasm`).

## Data model

Coordinates are points (1/72 in), y down; each spread has its own coordinate system with its
origin at the top left of its first page. A `Document` holds settings, spreads and parent
spreads (items under `Arc`, so edits are copy-on-write and undo is cheap), layers, stories,
styles (paragraph, character, object, table, cell; based-on chains and groups), swatches,
sections and text variables. A story is one `String` plus paragraph and character run lists of
`{len, style, overrides}`; effective attributes are default ← based-on chain ← style ← overrides.
Special characters (page number, footnote reference, anchored object, table) are placeholder
code points in the text. Threading is the story's ordered frame list.

## Commands, the only way to change a document

Every user-visible action is a `CommandSpec` (`id`, `label`, menu path, shortcut, parameter
doc, `enabled`, `run`, journaled, undoable) in `crates/engine/engine/src/cmd/*`. The kit's editor and the
headless kit app both call `Session::execute(id, params)`.
Tool gestures emit Begin/Preview/Commit commands so a drag is one undo step. Every journaled
command is replayable as a script. Escaped panics during a command or an import/export become an
error (`engine/src/guard.rs`), never a lost document. Outside a hosted session,
`file.recovery.save` writes unsaved documents to a recovery folder and `file.recovery.open`
reopens them; a hosted session (the editor, the headless kit serving Kition) refuses both.

## Composition and rendering

The compose cache is keyed by story revision, frame geometry, style revision and font database
revision; stories compose in parallel (rayon) on native targets. Composition produces per-frame
lines with positioned glyph runs, decorations, anchors and overset. Rendering is `render`'s
vello_cpu rasterizer (SIMD, multithreaded) into a pixel buffer. `render`'s `damage` module
reports which regions of the spreads changed between two versions of a document, so a view can
repaint just those; nothing in `bindings/` calls it today. The in-page editor draws that buffer to a `<canvas>`; the headless kit app renders the
same way, with no window or GPU presentation step.

## File I/O

Native document zip (JSON + assets); IDML both ways; PDF, EPUB, HTML, PNG/JPEG, text, XML export;
placed graphics through `images` (PDF/AI pages through hayro); Word/RTF/Excel/Tagged Text through
`textimport`. INDD is not readable. Details: [file-format-parity.md](parity/file-format-parity.md).

## Agent control

Kition's agent drives the engine through `bindings/headless`, the headless kit executable that
serves `kition-layout-kit`'s `KitDocument` over the Kit Protocol (`kition-headless`); the in-page
editor drives the same engine, compiled to WebAssembly, over the Kit UI protocol. Neither opens a
network port.

## Quality gates

`cargo xtask ci`: rustfmt, clippy `-D warnings`, tests, `assets` (every asset attributed in
[attribution.md](attribution.md)), `layers`, `wasm`. Clean crates deny `unwrap`/`expect`/`panic!`/`todo!`/
`unimplemented!`/`unreachable!`.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
