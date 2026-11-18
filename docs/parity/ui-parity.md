# UI and interaction parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe InDesign 2026 (21.6, Medium Dark, macOS)

Tools, handles, snapping, nudging, modifiers, panels, menus and feel, against InDesign 2026.
InDesign's behaviour comes from `plan/indesign/02-ui-ux.md`, `05-tools.md`, `06-panels.md`,
`07-dialogs.md` and `09-shortcuts.md` (local). Overall numbers:
[target-app-parity.md](target-app-parity.md).

## The editor

The kit's own editor is `web/`, a React app on kition-sdk's `@kition/kit-editor` that a host shows
in an iframe. It has:

- menu tabs (File, Edit, Layout, Type, Object, Table, View, Window, Help) built from the engine's
  commands and their menu paths, plus a Home tab;
- a tool rail with the engine's tools, which run in the engine (`crates/engine/tools`); the Hand
  and Zoom tools are the host's;
- rulers along the canvas (drag from a ruler to add a guide);
- a side panel with Pages, Layers, Swatches, Styles, Properties, Links and Sizes sections;
- Document Setup, Export and Templates dialogs;
- typed text through kit-editor's hidden text field, in which input methods compose.

The canvas suppresses the browser's context menu; there are no canvas context menus. The rows
below are the engine's tool behaviour, which the editor and the headless kit share.

## Checklist

**✓** like InDesign · **~** present, differs or unreliable · **✗** missing.

| Area | InDesign 2026 | Kition Layout | Status | Evidence |
|---|---|---|---|---|
| Selection tool: click, marquee, move, Alt-duplicate, resize handles, rotate from corners | ✓ | ✓ | ✓ | |
| Direct Selection, Pen family, Pencil, Scissors | ✓ | ✓ | ~ | moving reference point "0" fails |
| Smart guides, snapping to guides/grid/baseline | ✓ | ✓ (2,049-line `snap.rs`) | ~ | no vertical smart spacing between two rectangles |
| Nudging with arrows, Shift ×10 | ✓ | ✓ | ✓ | |
| Gridify while drawing, Gap tool, Live Distribute | ✓ | ✓ | ✓ | |
| Type tool: caret, selection, double/triple click, drag-and-drop text | ✓ | ✓ | ~ | formatting set at a caret is discarded |
| Threading: click out port, load text cursor, click frame | ✓ | ✓ out/in port click, loaded cursor (click a frame, drag one, click empty space; Alt keeps it loaded; Escape), double-click a port to unthread, `frame.thread` / `frame.unthread` | ~ | Shift-click autoflow and View › Extras › Show Text Threads; unthreading by clicking the previous or next frame with the loaded cursor is missing |
| Undo granularity (a drag = one step) | ✓ | ✓ (Begin/Preview/Commit) | ✓ | |

Not in the engine's tools: live corners and the content grabber.

## Remaining effort

| Work | Opus 5.5 h |
|---|---|
| Copy/paste, context menus, threading clicks, caret formatting on every platform, with regression tests | 30–50 |
| Canvas IME (port from Kition Vector) | 10–20 |
| Missing menu items (~150) and panels (Tabs, Navigator, Trap Presets) | 40–70 |
| Handle/tool feel fixes (reference point, smart spacing), font search focus | 10–20 |
| Dialog depth pass against 07-dialogs.md | 30–40 |
| **Total** | **120–180** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
