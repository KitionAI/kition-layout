import { useRef, useState, type KeyboardEvent, type MouseEvent, type PointerEvent, type ReactNode } from 'react'
import { ChevronDownIcon as ChevronDown, ChevronRightIcon as ChevronRight } from '@kition/tessera/icons/react'
import { IconButton, KitElement, readKitStore, useKitEditor, useMediaQuery, useUi, writeKitStore } from '@kition/sdk/editor'
import type { DocumentInfo, Spread } from '../layout'
import { LayersSection } from './Layers'
import { LinksSection } from './Links'
import { PagesSection } from './Pages'
import { PropertiesPanel } from './Properties'
import { SizesSection } from './Sizes'
import { StylesSection } from './Styles'
import { ObjectStylesSection } from './ObjectStyles'
import { SwatchesSection } from './Swatches'

/**
 * One group of the dock: its tabs (one shown at a time) and how it shares its column. An expanded
 * group starts at `basis` pixels (its body; the tab strip comes on top), never goes below `min`
 * (where the column is too short for every group's minimum, the column scrolls), and takes a
 * `grow` share of the room the column has over the groups' bases (a taller window, a collapsed
 * neighbour).
 */
export type DockGroupSpec = { group: string; tabs: readonly { id: string; label: string }[]; basis: number; min: number; grow: number }
/** The dock's columns, left to right (the last beside the window's edge), each top to bottom. */
export type DockSpec = { columns: ReadonlyArray<{ width: number; groups: readonly DockGroupSpec[] }> }

/**
 * A wide window (1440 × 900 and up, phase 12): two columns, so every panel a person works with has
 * room on first view without scrolling the dock. Properties, with the styles it applies under it,
 * beside the canvas; Pages, Layers and Swatches (with Links) at the window's edge. Five groups:
 * Pages and Layers each have their own, as do Swatches and Styles (existing tests use them side
 * by side).
 */
export const WIDE_DOCK: DockSpec = {
  columns: [
    {
      width: 272,
      groups: [
        { group: 'properties', tabs: [{ id: 'properties', label: 'Properties' }], basis: 470, min: 200, grow: 3 },
        { group: 'styles', tabs: [{ id: 'styles', label: 'Styles' }], basis: 250, min: 190, grow: 1 },
      ],
    },
    {
      width: 264,
      groups: [
        { group: 'pages', tabs: [{ id: 'pages', label: 'Pages' }], basis: 250, min: 170, grow: 1 },
        { group: 'layers', tabs: [{ id: 'layers', label: 'Layers' }], basis: 225, min: 180, grow: 2 },
        {
          group: 'swatches',
          tabs: [
            { id: 'swatches', label: 'Swatches' },
            { id: 'links', label: 'Links' },
          ],
          basis: 225,
          min: 190,
          grow: 1,
        },
      ],
    },
  ],
}

/**
 * A window too small for two columns (1024 × 700, 1280 × 900): one column of three groups, so each
 * tab shown has room for its panel; Pages and Layers share one, Swatches, Links and Styles the
 * other.
 */
export const NARROW_DOCK: DockSpec = {
  columns: [
    {
      width: 288,
      groups: [
        { group: 'properties', tabs: [{ id: 'properties', label: 'Properties' }], basis: 160, min: 130, grow: 1 },
        {
          group: 'pages-layers',
          tabs: [
            { id: 'layers', label: 'Layers' },
            { id: 'pages', label: 'Pages' },
          ],
          basis: 190,
          min: 140,
          grow: 1,
        },
        {
          group: 'swatches-styles',
          tabs: [
            { id: 'swatches', label: 'Swatches' },
            { id: 'styles', label: 'Styles' },
            { id: 'links', label: 'Links' },
          ],
          basis: 190,
          min: 140,
          grow: 1,
        },
      ],
    },
  ],
}

/** From this window size the dock has two columns (`WIDE_DOCK`), below it one (`NARROW_DOCK`). */
const WIDE_WINDOW = '(min-width: 1360px) and (min-height: 800px)'

/** A group's place in the dock as the person left it (view state, kept per kit in the browser). */
type Kept = { tab?: string; collapsed?: boolean; size?: number }
type Layout = Record<string, Kept>
const KIT = 'kit.layout'
const STORE = 'panels'
/** The height of a group's tab strip (its border included). */
const STRIP = 29
const STEP = 16

function loadLayout(): Layout {
  return readKitStore(KIT, STORE, (parsed) => (parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? (parsed as Layout) : undefined)) ?? {}
}

/**
 * How an expanded group sits in its column (CSS flex): it starts at its basis (the size the
 * person dragged it to, else the spec's), shrinks in proportion to it, grows by its share, and
 * keeps its minimum. A collapsed group is its strip; the room it gives goes to the others by
 * their shares.
 */
export function groupFlex(spec: DockGroupSpec, kept: Kept | undefined) {
  if (kept?.collapsed) return { flex: '0 0 auto' }
  const size = typeof kept?.size === 'number' && Number.isFinite(kept.size) ? Math.max(spec.min, kept.size) : spec.basis
  return { flex: `${spec.grow} 1 ${size + STRIP}px`, minHeight: spec.min + STRIP }
}

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

/**
 * The panels beside the spreads as tabbed groups (plan 2026-11-25, stage 4; phase 12): each group
 * shows one tab at a time and collapses to its strip (its chevron, a click on the empty part of
 * its tab bar or a double-click on its tab). The groups of a column share its height: each keeps
 * room for its panel, a collapsed group's room goes to the others, and the divider under a group
 * (drag it, or Up/Down on it) moves room between it and the group below. A wide window has two
 * columns of five groups (`WIDE_DOCK`), a smaller one one column of three (`NARROW_DOCK`).
 * The dock remembers its tabs, sizes and collapsed groups per kit in the browser (view state,
 * never the file).
 */
export function LayoutDock({ info, spread, onShow, onSetup }: { info: DocumentInfo; spread: Spread | null; onShow: (spread: Spread) => void; onSetup: () => void }) {
  const { phrase, t } = useKitEditor()
  const spec = useMediaQuery(WIDE_WINDOW) ? WIDE_DOCK : NARROW_DOCK
  const content: Record<string, () => ReactNode> = {
    properties: () => <PropertiesPanel info={info} />,
    pages: () => <PagesSection info={info} onShow={onShow} onSetup={onSetup} sizes={<SizesSection info={info} onShow={onShow} />} />,
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
  const [layout, setLayout] = useState<Layout>(loadLayout)
  const change = (next: Layout, keep: boolean) => {
    setLayout(next)
    if (keep) writeKitStore(KIT, STORE, next)
  }
  const patch = (group: string, kept: Kept, keep = true) => change({ ...layout, [group]: { ...layout[group], ...kept } }, keep)
  return (
    <aside className="kui-panel layout-side" data-side="right" data-testid="layout-side" data-columns={spec.columns.length} onClick={toggleFromTabBar}>
      {spec.columns.map((column, index) => (
        <DockColumn key={spec.columns.length * 10 + index} groups={column.groups} width={column.width} layout={layout} patch={patch} change={change} content={content} label={phrase} toggle={(collapsed) => t(collapsed ? 'expand' : 'collapse')} />
      ))}
    </aside>
  )
}

type ColumnProps = {
  groups: readonly DockGroupSpec[]
  width: number
  layout: Layout
  patch: (group: string, kept: Kept, keep?: boolean) => void
  change: (next: Layout, keep: boolean) => void
  content: Record<string, () => ReactNode>
  label: (text: string) => string
  toggle: (collapsed: boolean) => string
}

/** One column of the dock: its groups, top to bottom, sharing its height. */
function DockColumn({ groups, width, layout, patch, change, content, label, toggle }: ColumnProps) {
  const ui = useUi()
  const column = useRef<HTMLDivElement>(null)
  const drag = useRef<{ pointer: number; y: number; above: string; below: string; sizes: Map<string, number>; base: Layout; last: Layout } | null>(null)
  const visible = groups.map((group) => ({ group, tabs: group.tabs.filter((tab) => !ui.state(`panel:${tab.id}`).hidden) })).filter(({ tabs }) => tabs.length > 0)
  const expanded = visible.filter(({ group }) => !layout[`${group.group}-group`]?.collapsed)
  /** The bodies' heights now, as the sizes every expanded group of the column keeps from here on. */
  const measured = () => {
    const sizes = new Map<string, number>()
    for (const { group } of expanded) {
      const element = column.current?.querySelector<HTMLElement>(`[data-testid="layout-dock-group-${group.group}-group"]`)
      if (element) sizes.set(group.group, Math.max(group.min, Math.round(element.getBoundingClientRect().height - STRIP)))
    }
    return sizes
  }
  /** Moves `delta` pixels from the group below the divider to the one above it (or back), each kept at its minimum. */
  const resized = (base: Layout, sizes: Map<string, number>, above: DockGroupSpec, below: DockGroupSpec, delta: number) => {
    const a = sizes.get(above.group) ?? above.basis
    const b = sizes.get(below.group) ?? below.basis
    const moved = Math.max(above.min - a, Math.min(b - below.min, delta))
    const next: Layout = { ...base }
    for (const [group, size] of sizes) next[`${group}-group`] = { ...next[`${group}-group`], size }
    next[`${above.group}-group`] = { ...next[`${above.group}-group`], size: a + moved }
    next[`${below.group}-group`] = { ...next[`${below.group}-group`], size: b - moved }
    return next
  }
  const belowOf = (group: DockGroupSpec) => {
    const at = expanded.findIndex((item) => item.group === group)
    return at >= 0 ? (expanded[at + 1]?.group ?? null) : null
  }
  const onDividerDown = (event: PointerEvent<HTMLDivElement>, group: DockGroupSpec) => {
    const below = belowOf(group)
    if (!below || event.button !== 0) return
    event.preventDefault()
    event.currentTarget.setPointerCapture?.(event.pointerId)
    drag.current = { pointer: event.pointerId, y: event.clientY, above: group.group, below: below.group, sizes: measured(), base: layout, last: layout }
  }
  const onDividerMove = (event: PointerEvent<HTMLDivElement>) => {
    const now = drag.current
    if (!now || now.pointer !== event.pointerId) return
    const above = groups.find((item) => item.group === now.above)
    const below = groups.find((item) => item.group === now.below)
    if (!above || !below) return
    now.last = resized(now.base, now.sizes, above, below, Math.round(event.clientY - now.y))
    change(now.last, false)
  }
  const onDividerUp = (event: PointerEvent<HTMLDivElement>) => {
    const now = drag.current
    if (!now || now.pointer !== event.pointerId) return
    event.currentTarget.releasePointerCapture?.(event.pointerId)
    drag.current = null
    change(now.last, true)
  }
  const onDividerKey = (event: KeyboardEvent<HTMLDivElement>, group: DockGroupSpec) => {
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return
    const below = belowOf(group)
    if (!below) return
    event.preventDefault()
    const step = (event.shiftKey ? STEP * 4 : STEP) * (event.key === 'ArrowDown' ? 1 : -1)
    change(resized(layout, measured(), group, below, step), true)
  }
  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>, id: string, tabs: readonly { id: string }[], shown: string) => {
    if ((event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') || tabs.length < 2) return
    event.preventDefault()
    const at = Math.max(0, tabs.findIndex((tab) => tab.id === shown))
    const next = tabs[(at + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length]
    patch(id, { tab: next.id })
    column.current?.querySelector<HTMLButtonElement>(`[data-testid="layout-dock-tab-${next.id}"]`)?.focus()
  }
  return (
    <div ref={column} className="kui-dock layout-dock-column" data-testid="layout-dock" style={{ width }}>
      {visible.map(({ group, tabs }) => {
        const id = `${group.group}-group`
        const kept = layout[id]
        const collapsed = Boolean(kept?.collapsed)
        const tab = tabs.find((item) => item.id === kept?.tab) ?? tabs[0]
        const below = collapsed ? null : belowOf(group)
        return (
          <div key={id} className="kui-dock-group" data-testid={`layout-dock-group-${id}`} data-collapsed={collapsed} style={groupFlex(group, kept)}>
            <div className="kui-dock-strip" data-kit-ui-id={`panel:${tab.id}`}>
              <div className="kui-dock-tabs" role="tablist">
                {tabs.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    role="tab"
                    aria-selected={item.id === tab.id}
                    aria-controls={`layout-dock-panel-${id}`}
                    tabIndex={item.id === tab.id ? 0 : -1}
                    className="kui-dock-tab"
                    data-testid={`layout-dock-tab-${item.id}`}
                    data-kit-ui-id={`panel:${item.id}`}
                    onClick={() => patch(id, { tab: item.id })}
                    onKeyDown={(event) => onTabKey(event, id, tabs, tab.id)}
                  >
                    {label(item.label)}
                  </button>
                ))}
              </div>
              <IconButton
                icon={collapsed ? ChevronRight : ChevronDown}
                label={toggle(collapsed)}
                aria-expanded={!collapsed}
                onClick={() => patch(id, { collapsed: !collapsed })}
                data-testid={`layout-dock-collapse-${id}`}
              />
            </div>
            {collapsed ? null : (
              <div className="kui-dock-body" role="tabpanel" id={`layout-dock-panel-${id}`}>
                <KitElement id={`panel:${tab.id}`}>{content[tab.id]?.()}</KitElement>
              </div>
            )}
            {below ? (
              <div
                className="kui-dock-handle"
                role="separator"
                aria-orientation="horizontal"
                tabIndex={0}
                data-testid={`layout-dock-handle-${id}`}
                onPointerDown={(event) => onDividerDown(event, group)}
                onPointerMove={onDividerMove}
                onPointerUp={onDividerUp}
                onPointerCancel={onDividerUp}
                onKeyDown={(event) => onDividerKey(event, group)}
              />
            ) : null}
          </div>
        )
      })}
    </div>
  )
}
