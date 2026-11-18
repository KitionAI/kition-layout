# Gaps

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

Every known shortfall of Kition Layout, from the feature catalogue: features that are not yet `has`,
then the differences recorded on features that work. Update the feature in `features.yaml` when you
land work, then this list. The comparison with the target application is in
[parity/target-app-parity.md](parity/target-app-parity.md); the order of work is in [roadmap.md](roadmap.md).

## Features not done

- `layout.agent.premises` (P0, partial): An agent's edit holds the facts it observed across an outside file rewrite. All 395 journaled commands have blind projections (C8-layout-read-3). Over all 395: 165 commands differ, 1 holding less (file.importXml, agent-blocked: a list-field premise holds its members at SDK 34dff06) and 165 more (79 hold probed-free fields, 120 declare an editor axis, mostly mode, that the reading does not name). Selections made by an earlier step of the same run are held as of the call start (waiting_for_sdk pin; kition-doc-style step tracking not adopted). Conditional paths and translation review remain; the SDK pin still misses a list insertion after deletion before its gap.
- `layout.book.manage` (P1, partial): Books: add documents, synchronize styles, paginate and export. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.color.swatches` (P1, partial): Swatches, colour settings and inks. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.data.merge` (P1, partial): Data merge and text variables. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.edit.history` (P1, partial): Undo, redo, select, cut, copy, paste, duplicate and step and repeat. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.file.documents` (P1, partial): Create, open, save, save as, save a copy, revert and close documents; templates and document setup history. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.file.export` (P1, partial): Export PDF, EPUB, HTML, text, images, XML and IDML; open IDML; print, booklets and package. File › Export writes PDF, PNG, JPEG, HTML, IDML and EPUB through the host (cases below); print, booklets, text, XML and package have no case in the editor yet.
- `layout.file.place` (P1, partial): Place files and styles, links, snippets, libraries and the content conveyor. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.file.recovery` (P1, partial): Recovery data. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.interactive.elements` (P1, partial): Buttons, forms, object states, media and articles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.layer.manage` (P1, partial): Layers. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.layout.pages` (P1, partial): Pages, spreads, parents, sections, margins and columns, alternate layouts, guides and liquid layout. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.notes.footnotes` (P1, partial): Footnotes and endnotes. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.object.arrange` (P1, partial): Arrange, group, lock, hide, align and distribute objects. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.object.effects` (P1, partial): Fill, stroke, opacity, gradients and transparency effects. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.object.options` (P1, partial): Fitting, text wrap, frame options, labels, alt text, captions, QR codes and export options. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.object.paths` (P1, partial): Paths, lines, compound paths, Pathfinder, convert shape, corners, clipping paths and stroke styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.reference.navigation` (P1, partial): Index, table of contents, cross-references, lists, anchors, bookmarks and hyperlinks. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.review.text` (P1, partial): Track changes, notes, conditional text, spelling and find and change. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.select.tools` (P1, partial): Selection and tools. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.style.text` (P1, partial): Paragraph, character and object styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.table.edit` (P1, partial): Tables, cells, table and cell styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.text.frames` (P1, partial): Text frames, threading, stories, text editing, anchored objects and math. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.transform.objects` (P1, partial): Move, scale, rotate, shear, flip and transform again. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.type.format` (P1, partial): Character and paragraph formats, fonts, composite fonts, CJK and hyphenation. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.xml.structure` (P1, partial): XML tags, structure and DTDs. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `layout.app.preferences` (P2, partial): Preferences, scripts, preflight and view. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.

## Catalogue

The catalogue has 42 features: 15 `has`, 27 not yet.
