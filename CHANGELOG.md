# Changelog

Changes to Kition Layout, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## Unreleased

- The plain document model (kition-sdk 91cf510, plan 2026-11-24): the KDM is no longer a CRDT. A `.klayout` holds `content.json` (DEFLATE, level 6), the KDM content encoding written by `Replica::content_snapshot`: the state, no version, lineage, actor or origin, so an unchanged layout saves to the same bytes without a seal. `prepareSave` and the seal are gone. A replica takes changes in order on one line; concurrent co-editing, the convergence suite and the kition-server co-editing script go until Kition One orders changes on a server. The committed samples, fixtures and templates are converted with their document and object ids kept.
- A `.klayout` holds the layout's content, not its history (kition-sdk data model 7.3, rule M16b): `content.bin` is the replica's sealed content snapshot (the state, and of the history only one `system` seal), with the blobs the state references; no `snapshot.bin`, no change log, no compaction, no deleted or overwritten content, no person's or agent's origin. `meta.json` names format, formatVersion, kit, kdm, schemaVersion and docId only (no title copy, no writer version). Files of the old shape do not open. The history stays with the host's history store: `prepareSave` seals the live replica before a save, and a file opens on the host's stored replica through `take_file` (its changes, or its state by object id). Until the kit editor's packages call `prepareSave`, a save nobody sealed for writes the content of a sealed copy and `markSaved` takes the seal in. The fixtures, case inputs, sample and templates are rewritten as content files with their docIds and ids kept. kition-sdk 82a3224.

## 0.1.4

- The same kit as 0.1.3, built on kition-sdk 6672663 (the replica's undo passes over an undo group whose changes came to nothing): the 0.1.3 package was built on kition-sdk 6b4d471.

## 0.1.3

- A host tells a layout file's document id (`document.identity`) and makes a copied layout a new document (`document.fork`): the fork keeps the pages, frames, stories, styles and their ids, and carries a new id and none of the source's history (Kit Process Protocol section 5.6, kition-sdk 44d500b). Two copies of the same `.klayout` were one document until now.
- The headless program speaks Kit Process Protocol version 1 (`headless.protocol` is `kition-kit/1`), as hosts on kition-sdk from bb1b07b require.
- On kition-sdk 6672663: undo and redo pass over a step whose changes came to nothing, so one undo always takes back a visible edit.

## 0.1.2

- `kit.json` declares the engine's editor state as `engine.editorState`, the same list as the engine's `EDITOR_STATE`, so a host that does not run the engine reads `inspect()` without it.
- Lint fix in path hole orientation (clippy 1.100), no change in behaviour.
- The README connects an agent with `npm install -g @kition/cli` and `kition agent add`.

## 0.1.1

- The pasteboard around the spreads follows the host's theme, live: in a dark host it is dark, and it repaints when the host's theme changes (before, it kept its built-in light grey).

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: pages, spreads, parent pages, guides and layers; text threaded through frames
  with columns; paragraph and character styles with tabs, leaders and numbered lists; swatches,
  gradients and drop shadows; groups, transforms and tables. It opens IDML and exports PDF, PNG,
  JPEG, HTML, IDML and EPUB.
- The kit engine (`kition-layout-kit`), native and WebAssembly: the layout is a KDM replica,
  replicas merge concurrent edits, and undo is per person and per agent. A live preflight check
  reports what a layout cannot print as intended, with the command that fixes it.
- The headless kit (`kition-layout`) for agents over the Kit Protocol: it reads a layout's pages
  and stories, edits them with the editor's own commands, previews a page, a spread or an object,
  and works in drafts. A write names the revision it read and is refused when someone else changed
  what it rests on. It reads nothing outside its workspace and starts no program.
- The kit's editor (`web/`): the menu bar, the tool rail with flyouts, the spread canvas and the
  Pages, Layers, Swatches and Styles panels, in kit-editor's frame.
- NOTICE and docs/attribution.md list the upstream work, the fonts and every asset, and Loro
  (MIT), bundled by the editor and linked by the engine.
