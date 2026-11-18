# Changelog

Changes to Kition Layout, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

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
