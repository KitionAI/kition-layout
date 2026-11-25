import { useMemo, type ReactNode, type RefObject } from 'react'
import { DockArea, useKitEditor, useMediaQuery, type DockAreaHandle, type DockGroup } from '@kition/sdk/editor'
import type { DocumentInfo, Spread } from '../layout'
import { LayersSection } from './Layers'
import { LinksSection } from './Links'
import { PagesSection } from './Pages'
import { PropertiesPanel } from './Properties'
import { SizesSection } from './Sizes'
import { StylesSection } from './Styles'
import { SwatchesSection } from './Swatches'

type DockSpec = ReadonlyArray<{ group: string; tabs: readonly { id: string; label: string }[]; defaultHeight: number; compactHeight: number; minHeight: number; preferredFill?: number }>

/**
 * The panels beside the spreads as tabbed groups (plan 2026-11-25, stage 4; kit-editor's
 * `DockArea`): each group shows one tab at a time, collapses to its strip, and is resized by its
 * bottom edge; the last one takes the rest of the column, and the others give way in a short
 * window down to their minimums (then the dock scrolls). Properties, Pages (with Sizes) and Layers
 * each have a group of their own, as a person uses them together (select in Layers, set in
 * Properties, add a page); Swatches shares one with Links, and the two style lists share the last.
 * The dock remembers its tabs and heights per kit in the browser.
 */
export const LAYOUT_DOCK: DockSpec = [
  { group: 'properties', tabs: [{ id: 'properties', label: 'Properties' }], defaultHeight: 400, compactHeight: 280, minHeight: 150 },
  { group: 'pages', tabs: [{ id: 'pages', label: 'Pages' }], defaultHeight: 150, compactHeight: 104, minHeight: 80 },
  { group: 'layers', tabs: [{ id: 'layers', label: 'Layers' }], defaultHeight: 160, compactHeight: 112, minHeight: 90 },
  {
    group: 'swatches',
    tabs: [
      { id: 'swatches', label: 'Swatches' },
      { id: 'links', label: 'Links' },
    ],
    defaultHeight: 140,
    compactHeight: 100,
    minHeight: 80,
  },
  { group: 'styles', tabs: [{ id: 'styles', label: 'Styles' }], defaultHeight: 140, compactHeight: 100, minHeight: 80, preferredFill: 100 },
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
    <aside className="kui-panel layout-side" data-side="right" data-testid="layout-side">
      <DockArea kit="kit.layout" groups={groups} shown={shown} handle={dock} testId="layout-dock" />
    </aside>
  )
}
