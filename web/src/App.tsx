import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from 'react'
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
import { SidePanel } from './SidePanel'
import { CanvasScrollbars, createViewStore } from './CanvasScrollbars'
import { RULER, Ruler, rulerOrigin } from './Rulers'
import { LayoutDialogs, type Dialogs, type OwnDialog } from './dialogs/DialogHost'
import { MenuHints, layoutMenuTabs, menuEnabled, menuKey } from './menus/layoutMenus'
import { LayoutContextMenu, MenuOpenerProvider, useCanvasContextMenu, type MenuOpener, type OpenMenu } from './menus/ContextMenus'
import { LAYOUT_ZOOM_TOOL, useBlinkingCaret, useCanvasWheel, useEngineModifiers, useFocusOnOpen, usePanelEnterReturnsFocus, useViewToolDoubleClicks } from './canvasInput'
import { useFileDrop, useSystemClipboard } from './clipboard'
import { VIEW_ACTIONS, VIEW_KEY_ALIASES, viewMove, type ViewAction } from './view'
import { ZoomEntry } from './ZoomEntry'
import { drawnCursor } from './cursors'
import type { DocumentInfo } from './layout'

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
  typing.current = () => tools.wantsText
  useCanvasWheel(canvas, () => typing.current())
  useFocusOnOpen(doc != null)
  usePanelEnterReturnsFocus(() => typing.current())
  const [rulers, setRulers] = useState(true)
  const rulersShown = useRef(rulers)
  rulersShown.current = rulers
  const workspace = useRef<HTMLDivElement>(null)
  const [menu, setMenu] = useState<OpenMenu | null>(null)
  const openMenu = useMemo<MenuOpener>(() => (spot, x, y) => setMenu({ spot, x, y }), [])
  useEngineBackdrop(doc, canvas)
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
  const runKit = (command: string) => {
    if (command === 'kit:file.place') dialogs.current?.place()
    else if (command === 'kit:layout.goToPage') showDialog('goToPage')
    else if (command === 'kit:view.rulers') setRulers((shown) => !shown)
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
    if (event.key === 'Escape' && tools.active === 'placeGun') {
      tools.select('selection')
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
  // A guide dragged from a ruler is an edit, which a read-only layout (a viewer) does not make.
  const addGuide = (orientation: 'horizontal' | 'vertical', position: number, index: number) => {
    if (!host.readOnly) void editor.run('guide.add', { orientation, position, spread: index })
  }
  const tabs = useMemo<FrameTab[]>(
    () => [
      // Home first (its own ribbon; a drop-down panel in Menus), then the engine's menus.
      { id: 'home', label: t('homeTab'), ribbon: <HomeTab onDialog={showDialog} onCommand={invoke} /> },
      ...layoutMenuTabs({
        commands,
        phrase,
        onCommand: invoke,
        onKitCommand: runKit,
        checked: (command) => (command === 'kit:view.rulers' ? rulersShown.current : undefined),
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
      <MenuOpenerProvider value={openMenu}>
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
          void editor.run('selection.set', { ids: [] })
          return true
        }}
        status={
          <>
            <span data-testid="layout-status">{tools.status ? phrase(tools.status) : state ? t(state.pageCount === 1 ? 'summaryOne' : 'summary', { pages: state.pageCount }) : ''}</span>
            {state?.selected ? <span>{t('selectedCount', { count: state.selected })}</span> : null}
          </>
        }
        rail={<EngineToolRail hidden={HIDDEN_TOOLS} />}
        panels={state ? <SidePanel info={state} spread={spread} onShow={(next) => canvas.current?.show(next.area)} onSetup={() => showDialog('documentSetup')} /> : null}
      >
        <div ref={workspace} className="kui-row" data-testid="layout-workspace" data-view={view ? `${view.zoom} ${view.x} ${view.y}` : undefined}>
          <CanvasFrame showRulers={rulers} rulers={(
            <>
              <span className="layout-ruler-corner" aria-hidden="true" />
              {state ? <Ruler axis="x" info={state} view={view} onGuide={addGuide} /> : <span />}
              {state ? <Ruler axis="y" info={state} view={view} onGuide={addGuide} /> : <span />}
            </>
          )}>
            <KitDocumentCanvas canvas={canvas} tool={engineTool} viewMemory={viewMemory} initialArea={state?.spreads[0]?.area ?? null} panBounds={panBounds} framed={false} animated={Boolean(state?.editingText)} onView={(next) => {
              setView(next)
              views.set(next)
            }}>
              {/* The scrollbars: there only while the pasteboard is longer than the view. */}
              <CanvasScrollbars views={views} content={() => panBounds} canvas={canvas} />
            </KitDocumentCanvas>
          </CanvasFrame>
        </div>
        <LayoutDialogs api={dialogs} info={state} place={place} onShowPage={showPage} />
        <MenuHints info={state} />
        <PolygonDoubleClick onOpen={() => showDialog('polygon')} />
        <CanvasShims info={state} canvas={canvas} workspace={workspace} typing={() => typing.current()} onOpen={openMenu} spread={() => spread?.index ?? 0} />
        <LayoutContextMenu menu={menu} info={state} onClose={() => setMenu(null)} dialogs={dialogs} onView={(action) => viewActions.current(action)} rulers={rulers} onRulers={() => setRulers((shown) => !shown)} />
        <ZoomEntry canvas={canvas} zoom={zoom} />
      </EditorFrame>
      </MenuOpenerProvider>
    </KitEditorProvider>
  )
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
