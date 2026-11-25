import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from 'react'
import {
  EditorFrame,
  EngineToolRail,
  HAND_TOOL,
  KitDocumentCanvas,
  KitEditorProvider,
  createKitStrings,
  shortcutMatches,
  useEngineTools,
  useKitEditorState,
  useKitSession,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type KitCanvasView,
  type Phrases,
  type RasterView,
  useFrameMode,
} from '@kition/sdk/editor'
import { engineManifest } from './engine'
import { useEngineBackdrop } from './backdrop'
import { CATALOGS } from './strings'
import { HIDDEN_TOOLS, describe } from './layout'
import { HomeTab } from './HomeTab'
import { LayoutDock } from './panels/dock'
import { DialogsProvider } from './panels/dialogsContext'
import { defaultFillStroke, layerToActivate, swapFillStroke } from './panels/panelLogic'
import { StatusInfo } from './StatusInfo'
import { CanvasScrollbars, createViewStore } from './CanvasScrollbars'
import { RULER, Ruler, rulerOrigin } from './Rulers'
import { LayoutDialogs, type Dialogs, type OwnDialog } from './dialogs/DialogHost'
import { MenuHints, layoutMenuTabs, menuEnabled, menuKey } from './menus/layoutMenus'
import { LayoutContextMenu, MenuOpenerProvider, useCanvasContextMenu, type MenuOpener, type OpenMenu } from './menus/ContextMenus'
import { LAYOUT_ZOOM_TOOL, useBlinkingCaret, useCanvasWheel, useEngineModifiers, useFocusOnOpen, usePanelEnterReturnsFocus, useViewToolDoubleClicks } from './canvasInput'
import { useFileDrop, useSystemClipboard } from './clipboard'
import { VIEW_ACTIONS, VIEW_KEY_ALIASES, viewMove, type ViewAction } from './view'
import { ZoomEntry } from './ZoomEntry'
import { PlaceCursorBadge } from './PlaceCursorBadge'
import { escapeAction } from './images'
import { drawnCursor } from './cursors'
import { useInputMethod } from './ime'
import { LIST_ROWS, nextListType, type ListKind } from './lists'
import { findItem, type DocumentInfo, type Item } from './layout'
import { GuideOverlay, guidesOf, useGuideGestures } from './GuideLayer'
import { EditorViewProvider, type EditorView } from './viewContext'
import { browserStorage, engineView, loadView, saveView, viewChange, viewChecked, type GuideRef, type ViewState } from './viewOptions'

/** The view tools are the host's (the Zoom tool's drag zooms into its area); the engine's other tools run in the engine. */
const HOST_TOOLS = { hand: HAND_TOOL, zoom: LAYOUT_ZOOM_TOOL }
const ARROWS = new Set(['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'])
/** The view actions' keys, with their aliases. */
const VIEW_KEYS: ReadonlyArray<readonly [string, ViewAction]> = [...VIEW_ACTIONS.map(([action, , shortcut]) => [shortcut, action] as const), ...VIEW_KEY_ALIASES]
const HIDDEN = new Set(HIDDEN_TOOLS)
/** The tools a double click on their button sets up (the number of sides). */
const POLYGON_TOOLS = ['tool:polygon', 'tool:polygonFrame']

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  const session = useKitSession(host, manifest, describe)
  const own = useKitEditorState(session, host, t, 'layout', phrase)
  const { doc, change, state } = session
  // A row greys out where the selection is not one it works on (Group with one object), and a
  // variant row (Align Left) as its engine command does.
  const editor = useMemo(() => ({ ...own, enabled: menuEnabled(own.enabled, state) }), [own, state])
  // Every person's run goes through the dialog host (plan 2026-11-25, stage 1).
  const dialogs = useRef<Dialogs | null>(null)
  // The text caret blinks (an engine overlay the editor hides every other moment).
  const drawOverlay = useBlinkingCaret(change.revision)
  const tools = useEngineTools(session.tool, doc != null, change, { initial: 'selection', hostTools: HOST_TOOLS, hidden: HIDDEN, onDialog: (name, params) => dialogs.current?.engine(name, params), drawOverlay })
  const canvas = useRef<KitCanvasHandle>(null)
  // The engine's tools with their drawn cursors (rotate, the hollow arrow, the eyedropper, the loaded cursors).
  const { canvasTool } = tools
  const engineTool = useMemo(() => {
    const cursor = drawnCursor(tools.active, canvasTool.cursor)
    return cursor === canvasTool.cursor ? canvasTool : { ...canvasTool, cursor }
  }, [canvasTool, tools.active])
  // Space and Ctrl reach the engine's tools, a modifier pressed mid-drag acts at once, a pause ends typing's undo step.
  useEngineModifiers(session.doc)
  const typing = useRef<() => boolean>(() => false)
  const runQuietRef = useRef<(command: string, params: Record<string, unknown>) => Promise<string | null>>(async () => null)
  // New objects go on the layer of what the person selected, and not on an empty layer a
  // document opened on (plan 2026-11-25, 0b).
  useActiveLayer(state, session.doc, runQuietRef)
  typing.current = () => tools.wantsText
  useCanvasWheel(canvas, () => typing.current())
  useFocusOnOpen(doc != null)
  usePanelEnterReturnsFocus(() => typing.current())
  // The view options (plan stage 3): the editor's, kept for the person, told to the engine.
  const [viewOptions, setViewOptions] = useState<ViewState>(() => loadView(browserStorage()))
  const viewShown = useRef(viewOptions)
  viewShown.current = viewOptions
  useEffect(() => saveView(browserStorage(), viewOptions), [viewOptions])
  const [selectedGuide, setSelectedGuide] = useState<GuideRef | null>(null)
  const [movingGuide, setMovingGuide] = useState<GuideRef | null>(null)
  const rulers = viewOptions.rulers
  const workspace = useRef<HTMLDivElement>(null)
  const [menu, setMenu] = useState<OpenMenu | null>(null)
  const openMenu = useMemo<MenuOpener>(() => (spot, x, y) => setMenu({ spot, x, y }), [])
  useEngineBackdrop(doc, canvas)
  // Draws the page and the engine's overlays again after the engine changed what it draws (a view
  // option, a dialog's preview) without a change the canvas follows.
  const refreshTools = tools.refresh
  const repaint = useCallback(() => {
    void canvas.current?.retryRender()
    refreshTools()
  }, [refreshTools])
  /**
   * After an editor method that changed the document or the session (`runSteps`, the place
   * cursor's `placeCycle`): the session's info and everything drawn from it follow, as after a
   * tool's gesture (a settled tool call brings the info up to date and tells the editor).
   */
  const settle = useCallback(async () => {
    await session.tool('toolState', [0, 0, '{}', canvas.current?.view()?.zoom ?? 1], true)
    repaint()
  }, [session, repaint])
  const shownMode = useRef<ViewState['screenMode'] | null>(null)
  useEffect(() => {
    if (!doc) return
    let live = true
    const json = JSON.stringify(engineView(viewOptions, { selected: selectedGuide, moving: movingGuide }))
    void doc
      .invoke('setView', json)
      .then(() => {
        if (!live) return
        // The page itself changes only with the screen mode; the overlays with every option.
        if (shownMode.current !== null && shownMode.current !== viewOptions.screenMode) void canvas.current?.retryRender()
        shownMode.current = viewOptions.screenMode
        refreshTools()
      })
      .catch(() => undefined)
    return () => {
      live = false
    }
  }, [doc, viewOptions, selectedGuide, movingGuide, refreshTools])
  // Input methods: marked text in place at the caret, the candidate window next to it (stage 5).
  useInputMethod({ doc, canvas, typing: () => typing.current(), repaint, revision: change.revision })
  const editorView = useMemo<EditorView>(() => ({ view: viewOptions, setView: (change) => setViewOptions(change), repaint }), [viewOptions, repaint])
  const runQuiet = useCallback((command: string, params: Record<string, unknown>) => editor.session.run(command, params), [editor.session])
  runQuietRef.current = runQuiet
  const guides = useMemo(() => guidesOf(state), [state])
  const guideGestures = useGuideGestures({ run: runQuiet, info: state, guides, view: viewOptions, canvas, tool: tools.active, readOnly: Boolean(host.readOnly), selected: selectedGuide, onSelect: setSelectedGuide, onMoving: setMovingGuide })
  const viewMemory = useRef<RasterView | null>(null)
  const [view, setView] = useState<KitCanvasView | null>(null)
  const views = useMemo(createViewStore, [])
  const zoom = view?.zoom ?? 1
  // Scrolling stops at the pasteboard's edge: past it the engine has nothing to draw.
  const width = state?.canvas.x1 ?? 0
  const height = state?.canvas.y1 ?? 0
  const panBounds = useMemo(() => (width > 0 && height > 0 ? { x0: 0, y0: 0, x1: width, y1: height } : null), [width, height])
  // The spread and page in the middle of the view: the rulers count from it, Layers lists its objects.
  const origin = state && view ? rulerOrigin(state, view) : null
  const spread = origin?.spread ?? state?.spreads[0] ?? null
  const pageInView = origin ? (origin.spread.pages.find((page) => origin.spread.area.x0 + page.bounds[0] - origin.spread.left === origin.x)?.index ?? 0) : 0
  const { commands } = editor
  const showDialog = (name: OwnDialog) => dialogs.current?.show(name)
  const invoke = (command: string) => dialogs.current?.invoke(command)
  /** Moves the view as a View row asks (`view.ts`): fit an area, set a zoom, or fit the pasteboard. */
  const viewAction = (action: ViewAction) => {
    const handle = canvas.current
    const move = state && handle ? viewMove(action, state, pageInView) : null
    if (!handle || !move) return
    if ('area' in move) handle.show(move.area)
    else if ('zoom' in move) handle.zoomTo(move.zoom)
    else handle.fit()
  }
  const viewActions = useRef(viewAction)
  viewActions.current = viewAction
  // The frame's Fit (the status bar's button, Cmd+0) fits the spread in view, not the whole pasteboard.
  const frameCanvas = useMemo<RefObject<KitCanvasHandle | null>>(
    () => ({
      get current() {
        const handle = canvas.current
        return handle ? { ...handle, fit: () => viewActions.current('fitSpread') } : null
      },
    }),
    [],
  )
  useViewToolDoubleClicks(() => viewActions.current('fitSpread'), () => viewActions.current('actualSize'))
  /**
   * Bullets or numbering on the selected paragraphs (the Type menu's switch, its key, the Home
   * tab's button): on, or off when the paragraph at the selection already has that list.
   */
  const toggleList = async (kind: ListKind) => {
    const open = session.doc
    const text = state?.editingText || state?.selection.some((id) => findItem(state, id)?.item.story != null)
    if (!open || !text || host.readOnly) return
    const attrs = JSON.parse(await open.execute('text.selectionAttrs', {}).catch(() => 'null')) as { para?: { listType?: string } } | null
    await editor.run('text.para', { attrs: { listType: nextListType(attrs?.para?.listType, kind) } })
  }
  // The menus are made once: their handlers reach the latest selection through this.
  const lists = useRef(toggleList)
  lists.current = toggleList
  const runKit = (command: string) => {
    const option = viewChange(command, viewShown.current)
    const list = LIST_ROWS.find(([kind]) => command === `kit:text.${kind}`)
    if (list) void lists.current(list[0])
    else if (option) setViewOptions(option)
    else if (command === 'kit:file.place') dialogs.current?.place()
    else if (command === 'kit:layout.goToPage') showDialog('goToPage')
    else if (command === 'kit:edit.preferences') showDialog('preferences')
    else if (command.startsWith('kit:view.')) viewActions.current(command.slice('kit:view.'.length) as ViewAction)
  }
  /**
   * The keys the frame does not give the engine's tools while they are idle (plan stage 2): an
   * arrow nudges the selection (Shift ten times, Alt a copy), Cmd+arrow moves the caret to a line's
   * or story's end while typing; the view's keys; Escape puts the place cursor down.
   */
  const canvasKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey
    const view = VIEW_KEYS.find(([shortcut]) => shortcutMatches(shortcut, event, undefined, true))
    if (view) {
      viewActions.current(view[1])
      return true
    }
    // W switches between Normal and Preview, as InDesign's (not while typing or in a field).
    if (event.key.toLowerCase() === 'w' && !command && !event.altKey && !event.shiftKey && !tools.wantsText && !inField(event.target)) {
      setViewOptions((now) => ({ ...now, screenMode: now.screenMode === 'normal' ? 'preview' : 'normal' }))
      return true
    }
    // X swaps the selection's fill and stroke, D gives it the default ones (InDesign's keys; the
    // engine has no command for either: a fill and a stroke command, run as one undo step).
    const letter = event.key.toLowerCase()
    if ((letter === 'x' || (letter === 'd' && !event.shiftKey)) && !command && !event.altKey && !tools.wantsText && !inField(event.target) && state?.selected && !host.readOnly) {
      const items = state.selection.flatMap((id) => {
        const found = findItem(state, id)?.item
        return found ? [found as Item & { fillTint?: number; strokeDetail?: { tint?: number } }] : []
      })
      const steps = letter === 'x' ? swapFillStroke(items) : defaultFillStroke(state.selection)
      const open = session.doc
      if (!open) return true
      void (async () => {
        try {
          await open.invoke('runSteps', phrase(letter === 'x' ? 'Swap Fill and Stroke' : 'Default Fill and Stroke'), JSON.stringify(steps))
        } catch (error) {
          editor.report(t('commandFailed'), error)
        }
        await settle()
      })()
      return true
    }
    // A selected ruler guide takes the arrows, Delete and Escape.
    if (!tools.wantsText && !inField(event.target) && guideGestures.guideKey(event)) return true
    // Several graphics in the place cursor: the arrows bring the next (or the previous) to the front.
    if (ARROWS.has(event.key) && tools.active === 'placeGun' && !command && session.doc) {
      const open = session.doc
      const forward = event.key === 'ArrowRight' || event.key === 'ArrowDown'
      void open
        .invoke('placeCycle', forward)
        .then(settle)
        .catch(() => undefined)
      return true
    }
    if (ARROWS.has(event.key) && tools.active !== 'hand' && tools.active !== 'zoom') {
      const zoomNow = canvas.current?.view()?.zoom ?? zoom
      const mods = (cmd: boolean) => JSON.stringify({ shift: event.shiftKey, alt: event.altKey, command: cmd, control: false, space: false })
      if (tools.wantsText) {
        if (!command) return false
        void session.tool('toolKey', [event.key, mods(true), zoomNow], true)
        return true
      }
      if (command || !state?.selected || host.readOnly) return false
      void session.tool('toolKey', [event.key, mods(false), zoomNow], true)
      return true
    }
    // Escape drops the place cursor's front graphic (the last: the Selection tool comes back),
    // and from a frame's selected graphic goes back to the frame.
    const escape = event.key === 'Escape' && !command && !event.altKey && !event.shiftKey && !tools.wantsText ? escapeAction(state, tools.active) : null
    if (escape === 'placeSkip') {
      const open = session.doc
      void (async () => {
        const left = open ? await open.invoke<number>('placeSkip').catch(() => 0) : 0
        if (!left) tools.select('selection')
        await settle()
      })()
      return true
    }
    if (escape === 'frame' && state) {
      void editor.run('select.set', { ids: state.selection })
      return true
    }
    return false
  }
  // The page in view (0-based) and its spread: what page commands act on.
  const place = { page: pageInView, spread: spread?.index ?? 0 }
  const showPage = (index: number) => {
    const at = state?.spreads.find((item) => item.pages.some((page) => page.index === index))
    if (at) canvas.current?.show(at.area)
  }
  const tabs = useMemo<FrameTab[]>(
    () => [
      // Home first (its own ribbon; a drop-down panel in Menus), then the engine's menus.
      { id: 'home', label: t('homeTab'), ribbon: <HomeTab onDialog={showDialog} onCommand={invoke} onList={(kind) => void lists.current(kind)} /> },
      ...layoutMenuTabs({
        commands,
        phrase,
        onCommand: invoke,
        onKitCommand: runKit,
        checked: (command) => viewChecked(command, viewShown.current),
        // The editor's own dialogs stand for these commands, at the top of File.
        before: {
          File: [
            { label: t('newDocumentMenu'), onSelect: () => showDialog('newDocument'), testId: 'layout-menu-new' },
            { label: t('templatesMenu'), onSelect: () => showDialog('templates'), testId: 'layout-menu-templates', primary: true },
            { label: t('documentSetupMenu'), onSelect: () => showDialog('documentSetup'), testId: 'layout-menu-document-setup', primary: true },
            { label: t('exportMenu'), onSelect: () => showDialog('export'), testId: 'layout-menu-export', primary: true },
          ],
        },
      }),
    ],
    // The handlers read `dialogs` when they run.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [commands, phrase, t],
  )

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <EditorViewProvider value={editorView}>
      <MenuOpenerProvider value={openMenu}>
      <DialogsProvider value={dialogs}>
      <EditorFrame
        tabs={tabs}
        defaultTab="home"
        canvas={frameCanvas}
        zoom={zoom}
        onKeyDown={(event) => {
          // The key of a row that asks first opens its dialog; a `kit:` row's key runs it.
          if (dialogs.current && menuKey(event, { commands, enabled: editor.enabled, dialogs: dialogs.current, onKitCommand: runKit })) return true
          if (canvasKey(event)) return true
          // Escape a busy tool did not take clears the selection; taken (default prevented) only
          // then, so in Focus an Escape with nothing to cancel still leaves Focus (KitPointer 4.4).
          if (event.key !== 'Escape' || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || !state?.selected) return false
          void editor.run('select.set', { ids: [] })
          return true
        }}
        status={
          <>
            <span data-testid="layout-status">{tools.status ? phrase(tools.status) : state ? t(state.pageCount === 1 ? 'summaryOne' : 'summary', { pages: state.pageCount }) : ''}</span>
            {state?.selected ? <span>{t('selectedCount', { count: state.selected })}</span> : null}
            {state ? <StatusInfo info={state} views={views} unit={viewOptions.unit} /> : null}
          </>
        }
        rail={<EngineToolRail hidden={HIDDEN_TOOLS} />}
        panels={state ? <LayoutDock info={state} spread={spread} onShow={(next) => canvas.current?.show(next.area)} onSetup={() => showDialog('documentSetup')} /> : null}
      >
        <div ref={workspace} className="kui-row" data-testid="layout-workspace" data-view={view ? `${view.zoom} ${view.x} ${view.y}` : undefined}>
          <CanvasFrame showRulers={rulers} rulers={(
            <>
              <span className="layout-ruler-corner" aria-hidden="true" />
              {state ? <Ruler axis="x" info={state} view={view} unit={viewOptions.unit} onGuide={guideGestures.fromRuler} /> : <span />}
              {state ? <Ruler axis="y" info={state} view={view} unit={viewOptions.unit} onGuide={guideGestures.fromRuler} /> : <span />}
            </>
          )}>
            <KitDocumentCanvas canvas={canvas} tool={engineTool} viewMemory={viewMemory} initialArea={state?.spreads[0]?.area ?? null} panBounds={panBounds} framed={false} animated={Boolean(state?.editingText)} onView={(next) => {
              setView(next)
              views.set(next)
            }}>
              {/* The scrollbars: there only while the pasteboard is longer than the view. */}
              <CanvasScrollbars views={views} content={() => panBounds} canvas={canvas} />
              <GuideOverlay drag={guideGestures.drag} info={state} view={view} unit={viewOptions.unit} />
            </KitDocumentCanvas>
          </CanvasFrame>
        </div>
        <LayoutDialogs api={dialogs} info={state} place={place} onShowPage={showPage} />
        <MenuHints info={state} />
        <PolygonDoubleClick onOpen={() => showDialog('polygon')} />
        <CanvasShims info={state} canvas={canvas} workspace={workspace} typing={() => typing.current()} onOpen={openMenu} spread={() => spread?.index ?? 0} />
        <LayoutContextMenu menu={menu} info={state} onClose={() => setMenu(null)} dialogs={dialogs} onView={(action) => viewActions.current(action)} view={viewOptions} onViewOption={runKit} />
        <ZoomEntry canvas={canvas} zoom={zoom} />
        <PlaceCursorBadge queue={state?.placeQueue ?? []} tool={tools.active} />
      </EditorFrame>
      </DialogsProvider>
      </MenuOpenerProvider>
      </EditorViewProvider>
    </KitEditorProvider>
  )
}

/**
 * The active layer follows the person (plan 2026-11-25, 0b): selecting objects on one layer makes
 * it active, as InDesign does, and in a document that opened on an empty layer while another
 * shown, unlocked layer has objects, the person's first press on the canvas (before anything they
 * draw there exists) makes the topmost of those active (the sample's "Notes"). Not at the moment
 * the document opens: the editor runs nothing of its own while the host is still opening it. The
 * active layer is editor state (`layer.activate`, no undo step), and new objects go on it.
 */
function useActiveLayer(info: DocumentInfo | null, doc: unknown, run: RefObject<(command: string, params: Record<string, unknown>) => Promise<string | null>>) {
  const latest = useRef(info)
  latest.current = info
  const selection = info?.selection.join(',') ?? ''
  const activate = (selected: number[]) => {
    const now = latest.current
    if (!now) return
    const counts = new Map<number, number>()
    for (const spread of now.spreads) for (const item of spread.items) counts.set(item.layer, (counts.get(item.layer) ?? 0) + 1)
    const next = layerToActivate(now.layers, counts, now.activeLayer, selected)
    if (next != null) void run.current?.('layer.activate', { id: next }).catch(() => null)
  }
  // The selection's layer.
  useEffect(() => {
    const now = latest.current
    if (!now || !doc) return
    const selected = [...new Set(now.selection.flatMap((id) => findItem(now, id)?.item.layer ?? []))]
    if (selected.length) activate(selected)
    // Only when the person's selection changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc, selection])
  // A document's first press on the canvas: off an empty layer it opened on.
  useEffect(() => {
    if (!doc) return
    const onPress = (event: PointerEvent) => {
      if (!(event.target instanceof Element) || !event.target.closest('.kui-canvas-region')) return
      window.removeEventListener('pointerdown', onPress, true)
      if (!latest.current?.selection.length) activate([])
    }
    window.addEventListener('pointerdown', onPress, true)
    return () => window.removeEventListener('pointerdown', onPress, true)
    // Once per document.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc])
}

/** Whether a key went to a field of its own (a panel's number, a text box). */
function inField(target: EventTarget | null) {
  return target instanceof HTMLElement && (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || target.isContentEditable)
}

/** A double click on a polygon tool's button (in the rail or its flyout) opens the polygon settings. */
function PolygonDoubleClick({ onOpen }: { onOpen: () => void }) {
  const latest = useRef(onOpen)
  latest.current = onOpen
  useEffect(() => {
    const onDoubleClick = (event: MouseEvent) => {
      const button = event.target instanceof Element ? event.target.closest('[data-kit-ui-id]') : null
      if (button && POLYGON_TOOLS.includes(button.getAttribute('data-kit-ui-id') ?? '')) latest.current()
    }
    document.addEventListener('dblclick', onDoubleClick)
    return () => document.removeEventListener('dblclick', onDoubleClick)
  }, [])
  return null
}

/**
 * What the editor adds to the canvas inside the editor's provider (plan stage 2): the right-click
 * menus, the system clipboard, and files dropped on the canvas.
 */
function CanvasShims({ info, canvas, workspace, typing, onOpen, spread }: { info: DocumentInfo | null; canvas: RefObject<KitCanvasHandle | null>; workspace: RefObject<HTMLDivElement | null>; typing: () => boolean; onOpen: MenuOpener; spread: () => number }) {
  useCanvasContextMenu({ info, canvas, onOpen, spread })
  useSystemClipboard({ info, typing, canvas })
  useFileDrop(workspace, info, canvas)
  return null
}

/**
 * The canvas with its rulers along the top and left. Focus shows the canvas alone: the rulers are
 * kit chrome, so they and their grid tracks are left out there, as when the person hides them.
 */
function CanvasFrame({ rulers, showRulers, children }: { rulers: ReactNode; showRulers: boolean; children: ReactNode }) {
  const hidden = useFrameMode() === 'focus' || !showRulers
  const track = hidden ? 'minmax(0, 1fr)' : `${RULER}px minmax(0, 1fr)`
  return (
    <div className="layout-canvas-frame" style={{ gridTemplateColumns: track, gridTemplateRows: track }}>
      {hidden ? null : rulers}
      {children}
    </div>
  )
}
