# Target-app parity

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

Each feature in `features.yaml` records its status against what the feature promises and, where it
has one, the target application it is compared with (InDesign 2026 (21.6)).
The difference column is the feature's recorded gap.

| Feature | Priority | Status | Target | Difference |
| --- | --- | --- | --- | --- |
| `layout.agent.act` An agent edits text in its frame | P0 | has | — | — |
| `layout.agent.observe` An agent reads the layout and reuses what it found | P0 | has | — | — |
| `layout.agent.premises` An agent's edit holds the facts it observed across an outside file rewrite | P0 | partial | — | All 395 journaled commands have blind projections (C8-layout-read-3). Over all 395: 165 commands differ, 1 holding less (file.importXml, agent-blocked: a list-field premise holds its members at SDK 34dff06) and 165 more (79 hold probed-free fields, 120 declare an editor axis, mostly mode, that the reading does not name). Selections made by an earlier step of the same run are held as of the call start (waiting_for_sdk pin; kition-doc-style step tracking not adopted). Conditional paths and translation review remain; the SDK pin still misses a list insertion after deletion before its gap. |
| `layout.agent.recover` An agent fixes what the check reports | P0 | has | — | — |
| `layout.layout.pages-guides` Insert pages, ruler guides, guide grids and page backgrounds | P0 | has | InDesign 2026 (21.6) | — |
| `layout.object.fill-stroke` Paths with fills, strokes, swatches, opacity, drop shadow and gradients | P0 | has | InDesign 2026 (21.6) | — |
| `layout.object.layers-groups` Layers, groups, stacking order and transforms | P0 | has | InDesign 2026 (21.6) | — |
| `layout.security.workspace` The kit reaches files only in its workspace, through its host, and starts no program | P0 | has | — | — |
| `layout.table.cells` Insert tables, type in cells and merge cells | P0 | has | InDesign 2026 (21.6) | — |
| `layout.text.threaded-frames` Thread a story through text frames with frame columns | P0 | has | InDesign 2026 (21.6) | — |
| `layout.type.styles` Paragraph and character styles and numbered lists | P0 | has | InDesign 2026 (21.6) | — |
| `layout.agent.converse` An agent follows up on its own edit over a conversation | P1 | has | — | — |
| `layout.agent.draft` An agent works in a draft of the layout, opened from its replica with its images | P1 | has | — | — |
| `layout.agent.render` An agent previews the layout, a page, a spread or an object | P1 | has | — | — |
| `layout.book.manage` Books: add documents, synchronize styles, paginate and export | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.collab.drafts` Drafts merge in and earlier versions restore as one undo step | P1 | partial | — | Engine tests merge a draft, compare it first, restore a version, name reply ids, answer receipts and the object model; test:contract checks the built engine exports every member; no case drives a draft or a restore in the editor yet. |
| `layout.collab.coedit` Real-time co-editing: editors converge, viewers cannot write, an offline page catches up | P1 | partial | — | Kition Desktop is for one person since the plain document model (kition-sdk 91cf510, plan 2026-11-24): the KDM no longer merges concurrent edits, so the convergence suite, the kition-server co-editing script and the layout's merge tests are gone; co-editing comes back with Kition One's server-ordered changes. What stays: every edit is a KDM change with stable ids, a viewer's editor writes nothing to the replica, and a page on the editor's line of changes follows its edits in order. Not yet: a server-ordered change stream, presence. |
| `layout.color.swatches` Swatches, colour settings and inks | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.data.merge` Data merge and text variables | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.edit.history` Undo, redo, select, cut, copy, paste, duplicate and step and repeat | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.editor.display-modes` The editor in each display mode: Classic gives way before stepping down, Single line does not repeat the rail's tools, Focus shows the canvas without rulers, and Escape cancels first in Focus | P1 | has | — | — |
| `layout.file.documents` Create, open, save, save as, save a copy, revert and close documents; templates and document setup history | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.file.export` Export PDF, EPUB, HTML, text, images, XML and IDML; open IDML; print, booklets and package | P1 | partial | InDesign 2026 (21.6) | File › Export writes PDF, PNG, JPEG, HTML, IDML and EPUB through the host (cases below); print, booklets, text, XML and package have no case in the editor yet. |
| `layout.file.place` Place files and styles, links, snippets, libraries and the content conveyor | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.file.recovery` Recovery data | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.interactive.elements` Buttons, forms, object states, media and articles | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.layer.manage` Layers | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.layout.pages` Pages, spreads, parents, sections, margins and columns, alternate layouts, guides and liquid layout | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.notes.footnotes` Footnotes and endnotes | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.object.arrange` Arrange, group, lock, hide, align and distribute objects | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.object.effects` Fill, stroke, opacity, gradients and transparency effects | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.object.options` Fitting, text wrap, frame options, labels, alt text, captions, QR codes and export options | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.object.paths` Paths, lines, compound paths, Pathfinder, convert shape, corners, clipping paths and stroke styles | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.reference.navigation` Index, table of contents, cross-references, lists, anchors, bookmarks and hyperlinks | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.review.text` Track changes, notes, conditional text, spelling and find and change | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.select.tools` Selection and tools | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.style.text` Paragraph, character and object styles | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.table.edit` Tables, cells, table and cell styles | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.text.frames` Text frames, threading, stories, text editing, anchored objects and math | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.transform.objects` Move, scale, rotate, shear, flip and transform again | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.type.format` Character and paragraph formats, fonts, composite fonts, CJK and hyphenation | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.xml.structure` XML tags, structure and DTDs | P1 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `layout.app.preferences` Preferences, scripts, preflight and view | P2 | partial | InDesign 2026 (21.6) | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |

| Priority | has | partial | missing |
| --- | ---: | ---: | ---: |
| P0 | 10 | 1 | 0 |
| P1 | 5 | 25 | 0 |
| P2 | 0 | 1 | 0 |
