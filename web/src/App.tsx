import { useMemo, useRef, useState, type ReactNode } from 'react'
import {
  EditorFrame,
  EngineToolRail,
  HAND_TOOL,
  KitDocumentCanvas,
  KitEditorProvider,
  ZOOM_TOOL,
  commandMenuTabs,
  createKitStrings,
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
} from '@kition/kit-editor'
import { engineManifest } from './engine'
import { CATALOGS } from './strings'
import { LAYOUT_PRIMARY, HIDDEN_TOOLS, MENU_ORDER, describe } from './layout'
import { HomeTab } from './HomeTab'
import { SidePanel } from './SidePanel'
import { RULER, Ruler, rulerOrigin } from './Rulers'
import { DocumentSetupDialog } from './dialogs/DocumentSetup'
import { ExportDialog } from './dialogs/Export'
import { TemplatesDialog } from './dialogs/Templates'

/** The view tools are the host's; the engine's other tools run in the engine. */
const HOST_TOOLS = { hand: HAND_TOOL, zoom: ZOOM_TOOL }
const HIDDEN = new Set(HIDDEN_TOOLS)
const HOST_FILE_COMMANDS = ['file.new', 'book.new', 'library.new', 'file.open', 'file.close', 'file.save', 'file.saveAs', 'file.revert']
/** Commands the editor's own dialogs stand for in the menus (agents still run them). */
const OWN_DIALOGS = ['document.applyTemplate', 'layout.documentSetup', 'file.exportPdf', 'file.exportEpub', 'file.exportFixedEpub', 'file.exportHtml']
/** Menus kept in a read-only document. */
const VIEW_MENUS = ['View', 'Window', 'Help']

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  const session = useKitSession(host, manifest, describe)
  const editor = useKitEditorState(session, host, t, 'layout', phrase)
  const { doc, change, state } = session
  const tools = useEngineTools(session.tool, doc != null, change, { initial: 'selection', hostTools: HOST_TOOLS, hidden: HIDDEN })
  const canvas = useRef<KitCanvasHandle>(null)
  const viewMemory = useRef<RasterView | null>(null)
  const [view, setView] = useState<KitCanvasView | null>(null)
  const [dialog, setDialog] = useState<'setup' | 'export' | 'templates' | null>(null)
  const zoom = view?.zoom ?? 1
  // The spread and page in the middle of the view: the rulers count from it, Layers lists its objects.
  const origin = state && view ? rulerOrigin(state, view) : null
  const spread = origin?.spread ?? state?.spreads[0] ?? null
  const pageInView = origin ? (origin.spread.pages.find((page) => origin.spread.area.x0 + page.bounds[0] - origin.spread.left === origin.x)?.index ?? 0) : 0
  const { commands } = editor
  // A guide dragged from a ruler is an edit, which a read-only layout (a viewer) does not make.
  const addGuide = (orientation: 'horizontal' | 'vertical', position: number, index: number) => {
    if (!host.readOnly) void editor.run('guide.add', { orientation, position, spread: index })
  }
  const tabs = useMemo<FrameTab[]>(
    () => [
      // Home first (its own ribbon; a drop-down panel in Menus), then the engine's menus.
      { id: 'home', label: t('homeTab'), ribbon: <HomeTab onDialog={setDialog} /> },
      ...commandMenuTabs(commands, {
        order: MENU_ORDER,
        exclude: [...HOST_FILE_COMMANDS, ...OWN_DIALOGS],
        phrase,
        view: VIEW_MENUS,
        primary: LAYOUT_PRIMARY,
        // The editor's own dialogs stand for these commands, at the top of File.
        before: {
          File: [
            { label: t('templatesMenu'), onSelect: () => setDialog('templates'), testId: 'layout-menu-templates', primary: true },
            { label: t('documentSetupMenu'), onSelect: () => setDialog('setup'), testId: 'layout-menu-document-setup', primary: true },
            { label: t('exportMenu'), onSelect: () => setDialog('export'), testId: 'layout-menu-export', primary: true },
          ],
        },
      }),
    ],
    [commands, phrase, t],
  )

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <EditorFrame
        tabs={tabs}
        defaultTab="home"
        canvas={canvas}
        zoom={zoom}
        onKeyDown={(event) => {
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
        panels={state ? <SidePanel info={state} spread={spread} onShow={(next) => canvas.current?.show(next.area)} onSetup={() => setDialog('setup')} /> : null}
      >
        <div className="kui-row" data-testid="layout-workspace" data-view={view ? `${view.zoom} ${view.x} ${view.y}` : undefined}>
          <CanvasFrame rulers={(
            <>
              <span className="layout-ruler-corner" aria-hidden="true" />
              {state ? <Ruler axis="x" info={state} view={view} onGuide={addGuide} /> : <span />}
              {state ? <Ruler axis="y" info={state} view={view} onGuide={addGuide} /> : <span />}
            </>
          )}>
            <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} initialArea={state?.spreads[0]?.area ?? null} framed={false} onView={setView} />
          </CanvasFrame>
        </div>
        {state && dialog === 'setup' ? <DocumentSetupDialog info={state} onClose={() => setDialog(null)} /> : null}
        {state && dialog === 'export' ? <ExportDialog info={state} page={pageInView} onClose={() => setDialog(null)} /> : null}
        {state && dialog === 'templates' ? <TemplatesDialog onClose={() => setDialog(null)} /> : null}
      </EditorFrame>
    </KitEditorProvider>
  )
}

/**
 * The canvas with its rulers along the top and left. Focus shows the canvas alone: the rulers are
 * kit chrome, so they and their grid tracks are left out there.
 */
function CanvasFrame({ rulers, children }: { rulers: ReactNode; children: ReactNode }) {
  const focus = useFrameMode() === 'focus'
  const track = focus ? 'minmax(0, 1fr)' : `${RULER}px minmax(0, 1fr)`
  return (
    <div className="layout-canvas-frame" style={{ gridTemplateColumns: track, gridTemplateRows: track }}>
      {focus ? null : rulers}
      {children}
    </div>
  )
}
