import { useMemo, type MouseEvent, type ReactNode, type RefObject } from 'react'
import { DockArea, useKitEditor, useMediaQuery, type DockAreaHandle, type DockGroup } from '@kition/sdk/editor'
import type { DocumentInfo, Spread } from '../layout'
import { LayersSection } from './Layers'
import { LinksSection } from './Links'
import { PagesSection } from './Pages'
import { PropertiesPanel } from './Properties'
import { SizesSection } from './Sizes'
import { StylesSection } from './Styles'
import { ObjectStylesSection } from './ObjectStyles'
import { SwatchesSection } from './Swatches'

type DockSpec = ReadonlyArray<{ group: string; tabs: readonly { id: string; label: string }[]; defaultHeight: number; compactHeight: number; minHeight: number; preferredFill?: number }>

/**
 * The panels beside the spreads as tabbed groups (plan 2026-11-25, stage 4; kit-editor's
 * `DockArea`): each group shows one tab at a time, collapses to its strip (its chevron, a click on
 * the empty part of its tab bar or a double-click on its tab), and is resized by its bottom edge;
 * the last one takes the rest of the column. Properties, Pages (with Sizes) and Layers each have a
 * group of their own, as a person uses them together (select in Layers, set in Properties, add a
 * page); Swatches shares one with Links, and the two style lists share the last. Every group keeps
 * room for its panel (Layers, Swatches and Styles at least six rows, phase 10) instead of giving
 * way to a sliver: where the column is too short for all of them it scrolls. The dock remembers
 * its tabs, heights and collapsed groups per kit in the browser (view state, never the file).
 */
export const LAYOUT_DOCK: DockSpec = [
  { group: 'properties', tabs: [{ id: 'properties', label: 'Properties' }], defaultHeight: 330, compactHeight: 230, minHeight: 230 },
  { group: 'pages', tabs: [{ id: 'pages', label: 'Pages' }], defaultHeight: 190, compactHeight: 170, minHeight: 170 },
  { group: 'layers', tabs: [{ id: 'layers', label: 'Layers' }], defaultHeight: 260, compactHeight: 260, minHeight: 260 },
  {
    group: 'swatches',
    tabs: [
      { id: 'swatches', label: 'Swatches' },
      { id: 'links', label: 'Links' },
    ],
    defaultHeight: 250,
    compactHeight: 250,
    minHeight: 250,
  },
  { group: 'styles', tabs: [{ id: 'styles', label: 'Styles' }], defaultHeight: 260, compactHeight: 260, minHeight: 260, preferredFill: 260 },
]

/**
 * A short window (1024 × 700): three groups, so each tab shown has room for its panel; Pages and
 * Layers share one, Swatches, Links and Styles the other.
 */
export const SHORT_DOCK: DockSpec = [
  { group: 'properties', tabs: [{ id: 'properties', label: 'Properties' }], defaultHeight: 300, compactHeight: 220, minHeight: 150 },
  {
    group: 'pages-layers',
    tabs: [
      { id: 'layers', label: 'Layers' },
      { id: 'pages', label: 'Pages' },
    ],
    defaultHeight: 160,
    compactHeight: 130,
    minHeight: 110,
  },
  {
    group: 'swatches-styles',
    tabs: [
      { id: 'swatches', label: 'Swatches' },
      { id: 'styles', label: 'Styles' },
      { id: 'links', label: 'Links' },
    ],
    defaultHeight: 160,
    compactHeight: 130,
    minHeight: 110,
    preferredFill: 140,
  },
]

/** Below this window height the dock has three groups (`SHORT_DOCK`). */
const SHORT_WINDOW = '(max-height: 799px)'

/**
 * A click on the empty part of a group's tab bar, or a second click on a tab (a double-click),
 * collapses or expands the group as its chevron does (and is remembered the same way).
 */
export function toggleFromTabBar(event: MouseEvent) {
  const target = event.target instanceof Element ? event.target : null
  const strip = target?.closest('.kui-dock-strip')
  if (!target || !strip) return
  const empty = target === strip || target.classList.contains('kui-dock-tabs')
  const again = event.detail === 2 && target.closest('.kui-dock-tab') != null
  if (!empty && !again) return
  strip.querySelector<HTMLButtonElement>('[data-testid^="layout-dock-collapse-"]')?.click()
}

/** The dock beside the canvas; `spread` is the one in view (Layers lists its objects). */
export function LayoutDock({ info, spread, onShow, onSetup, dock }: { info: DocumentInfo; spread: Spread | null; onShow: (spread: Spread) => void; onSetup: () => void; dock?: RefObject<DockAreaHandle | null> }) {
  const { phrase } = useKitEditor()
  const spec = useMediaQuery(SHORT_WINDOW) ? SHORT_DOCK : LAYOUT_DOCK
  const content: Record<string, () => ReactNode> = {
    properties: () => <PropertiesPanel info={info} />,
    pages: () => (
      <>
        <PagesSection info={info} onShow={onShow} onSetup={onSetup} />
        <SizesSection info={info} onShow={onShow} />
      </>
    ),
    layers: () => <LayersSection info={info} spread={spread} />,
    swatches: () => <SwatchesSection info={info} />,
    links: () => <LinksSection info={info} />,
    styles: () => (
      <>
        <StylesSection info={info} kind="paragraph" />
        <StylesSection info={info} kind="character" />
        <ObjectStylesSection info={info} />
      </>
    ),
  }
  const groups: DockGroup[] = spec.map(({ group, tabs, defaultHeight, compactHeight, minHeight, preferredFill }) => ({
    id: `${group}-group`,
    defaultHeight,
    compactHeight,
    minHeight,
    preferredFill,
    tabs: tabs.map((tab) => ({ id: tab.id, label: phrase(tab.label), render: content[tab.id] ?? (() => null) })),
  }))
  const shown = useMemo(() => new Set(spec.map(({ group }) => `${group}-group`)), [spec])
  return (
    <aside className="kui-panel layout-side" data-side="right" data-testid="layout-side" onClick={toggleFromTabBar}>
      <DockArea kit="kit.layout" groups={groups} shown={shown} handle={dock} testId="layout-dock" />
    </aside>
  )
}
