import { useMemo, useState, type MouseEvent } from 'react'
import { ArrowDownIcon as ArrowDown, ArrowUpIcon as ArrowUp, ChevronDownIcon as ChevronDown, ChevronRightIcon as ChevronRight, CopyIcon as Copy, EyeIcon as Eye, EyeOffIcon as EyeOff, GroupIcon as Group, LockIcon as Lock, LockOpenIcon as LockOpen, PlusIcon as Plus, TrashIcon as Trash, UngroupIcon as Ungroup } from '@kition/tessera/icons/react'
import { IconButton, PanelFooter, PanelSection, RenameField, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { flatten, type DocumentInfo, type Item, type Layer, type Spread } from '../layout'
import { useMenuOpener } from '../menus/ContextMenus'

/** Every row is this tall, so the list draws only the rows in view (thousands of objects). */
export const ROW = 26
const VIEW_ROWS = 14
const OVERSCAN = 6

type Row = { kind: 'layer'; layer: Layer; count: number } | { kind: 'item'; item: Item; depth: number }

/**
 * InDesign's Layers panel: the layers, front first, each opening to the objects of the spread in
 * view on it (groups indented under them). Rows are virtualised. A row's eye and lock hide and
 * lock; a click selects (Shift adds), a double-click renames. Under the list: new layer, move the
 * active layer, group, ungroup, duplicate and delete the selection. Read-only (a viewer), only
 * selecting and opening layers is left.
 */
export function LayersSection({ info, spread }: { info: DocumentInfo; spread: Spread | null }) {
  const { run, phrase: p, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [closed, setClosed] = useState<ReadonlySet<number>>(new Set())
  const [scroll, setScroll] = useState(0)
  const [renaming, setRenaming] = useState<{ id: number; layer: boolean; text: string } | null>(null)
  const rows = useMemo(() => {
    const objects = flatten(spread?.items ?? [])
    const out: Row[] = []
    for (const layer of info.layers) {
      const top = objects.filter(({ item, depth }) => depth === 0 && item.layer === layer.id).length
      out.push({ kind: 'layer', layer, count: top })
      if (closed.has(layer.id)) continue
      // A group's children follow it, whatever their own layer says.
      let on = false
      for (const entry of objects) {
        if (entry.depth === 0) on = entry.item.layer === layer.id
        if (on) out.push({ kind: 'item', item: entry.item, depth: entry.depth })
      }
    }
    return out
  }, [closed, info.layers, spread])
  const selected = new Set(info.selection)
  const first = Math.max(0, Math.floor(scroll / ROW) - OVERSCAN)
  const last = Math.min(rows.length, Math.ceil((scroll + VIEW_ROWS * ROW) / ROW) + OVERSCAN)
  const activeIndex = info.layers.findIndex((layer) => layer.id === info.activeLayer)
  const openMenu = useMenuOpener()
  /** A right-click on an object's row selects it (unless it is selected) and opens the object menu. */
  const objectMenu = (item: Item) => (event: MouseEvent) => {
    if (!openMenu) return
    event.preventDefault()
    const chosen = selected.has(item.id)
    if (!chosen) void run('select.set', { ids: [item.id] })
    openMenu({ kind: 'object', textFrame: item.story != null, image: item.asset != null, group: item.kind === '<group>', count: chosen ? info.selection.length : 1 }, event.clientX, event.clientY)
  }
  const layerMenu = (layer: Layer) => (event: MouseEvent) => {
    if (!openMenu) return
    event.preventDefault()
    openMenu({ kind: 'layer', layer: layer.id }, event.clientX, event.clientY)
  }
  const pick = (event: MouseEvent, id: number) => void run(event.shiftKey ? 'select.toggle' : 'select.set', event.shiftKey ? { id } : { ids: [id] })
  const finishRename = (commit: boolean) => {
    const current = renaming
    setRenaming(null)
    if (!commit || !current) return
    if (current.layer) void run('layer.set', { id: current.id, name: current.text })
    else void run('object.rename', { id: current.id, name: current.text })
  }

  const row = (entry: Row, index: number) => {
    const top = index * ROW
    if (entry.kind === 'layer') {
      const { layer } = entry
      const open = !closed.has(layer.id)
      return (
        <div key={`layer-${layer.id}`} className="layout-row layout-layer-row" style={{ top }} data-active={layer.id === info.activeLayer} onContextMenu={layerMenu(layer)} data-testid={`layout-layer-${layer.id}`}>
          <IconButton icon={layer.visible ? Eye : EyeOff} label={layer.visible ? t('hideLayer') : t('showLayer')} pressed={!layer.visible} disabled={readOnly} onClick={() => void run('layer.set', { id: layer.id, visible: !layer.visible })} data-testid={`layout-layer-visibility-${layer.id}`} />
          <IconButton icon={layer.locked ? Lock : LockOpen} label={layer.locked ? t('unlockLayer') : t('lockLayer')} pressed={layer.locked} disabled={readOnly} onClick={() => void run('layer.set', { id: layer.id, locked: !layer.locked })} data-testid={`layout-layer-lock-${layer.id}`} />
          <button
            type="button"
            className="layout-disclosure"
            aria-expanded={open}
            aria-label={open ? t('collapse') : t('expand')}
            onClick={() => setClosed((now) => {
              const next = new Set(now)
              if (open) next.add(layer.id)
              else next.delete(layer.id)
              return next
            })}
          >
            {open ? <ChevronDown size={13} aria-hidden="true" /> : <ChevronRight size={13} aria-hidden="true" />}
          </button>
          <span className="layout-layer-color" style={{ background: `rgb(${layer.color.join(',')})` }} aria-hidden="true" />
          {renaming?.layer && renaming.id === layer.id ? (
            <RenameField value={renaming.text} label={t('renameLayer')} onChange={(text) => setRenaming({ ...renaming, text })} onDone={finishRename} testId="layout-rename" />
          ) : (
            <button type="button" className="layout-row-name" onClick={() => void run('layer.activate', { id: layer.id })} onDoubleClick={() => !readOnly && setRenaming({ id: layer.id, layer: true, text: layer.name })}>
              {layer.name}
            </button>
          )}
          <span className="kui-muted layout-count">{entry.count}</span>
        </div>
      )
    }
    const { item, depth } = entry
    const name = item.name || p(item.kind)
    return (
      <div key={`item-${item.id}`} className="layout-row layout-item-row" style={{ top }} data-selected={selected.has(item.id)} onContextMenu={objectMenu(item)} data-testid={`layout-object-${item.id}`}>
        <IconButton icon={item.hidden ? EyeOff : Eye} label={item.hidden ? t('showObject') : t('hideObject')} pressed={item.hidden} disabled={readOnly} onClick={() => void run('object.setFlags', { ids: [item.id], hidden: !item.hidden })} data-testid={`layout-object-visibility-${item.id}`} />
        <IconButton icon={item.locked ? Lock : LockOpen} label={item.locked ? t('unlockObject') : t('lockObject')} pressed={item.locked} disabled={readOnly} onClick={() => void run('object.setFlags', { ids: [item.id], locked: !item.locked })} data-testid={`layout-object-lock-${item.id}`} />
        <span style={{ width: 14 + depth * 12, flex: 'none' }} aria-hidden="true" />
        {renaming && !renaming.layer && renaming.id === item.id ? (
          <RenameField value={renaming.text} label={t('renameObject')} onChange={(text) => setRenaming({ ...renaming, text })} onDone={finishRename} testId="layout-rename" />
        ) : (
          <button type="button" className="layout-row-name" onClick={(event) => pick(event, item.id)} onDoubleClick={() => !readOnly && setRenaming({ id: item.id, layer: false, text: name })} data-testid={`layout-object-name-${item.id}`}>
            {name}
          </button>
        )}
      </div>
    )
  }

  return (
    <PanelSection title={p('Layers')} actions={<IconButton icon={Plus} label={t('newLayer')} disabled={readOnly} onClick={() => void run('layer.new', {})} data-testid="layout-new-layer" />} testId="layout-layers">
      <div className="layout-virtual" style={{ height: Math.min(rows.length, VIEW_ROWS) * ROW }} onScroll={(event) => setScroll(event.currentTarget.scrollTop)} data-testid="layout-layer-list" data-rows={rows.length}>
        <div style={{ height: rows.length * ROW, position: 'relative' }}>{rows.slice(first, last).map((entry, offset) => row(entry, first + offset))}</div>
      </div>
      <PanelFooter
        start={
          <>
            <IconButton icon={ArrowUp} label={t('layerUp')} disabled={readOnly || activeIndex <= 0} onClick={() => info.activeLayer != null && void run('layer.move', { id: info.activeLayer, to: activeIndex - 1 })} data-testid="layout-layer-up" />
            <IconButton icon={ArrowDown} label={t('layerDown')} disabled={readOnly || activeIndex < 0 || activeIndex + 1 >= info.layers.length} onClick={() => info.activeLayer != null && void run('layer.move', { id: info.activeLayer, to: activeIndex + 1 })} data-testid="layout-layer-down" />
          </>
        }
        end={
          <>
            <IconButton icon={Group} label={p('Group')} disabled={readOnly || info.selection.length < 2} onClick={() => void run('arrange.group', { ids: info.selection })} data-testid="layout-group" />
            <IconButton icon={Ungroup} label={p('Ungroup')} disabled={readOnly || !info.selection.length} onClick={() => void run('arrange.ungroup', { ids: info.selection })} data-testid="layout-ungroup" />
            <IconButton icon={Copy} label={p('Duplicate')} disabled={readOnly || !info.selection.length} onClick={() => void run('edit.duplicate', {})} data-testid="layout-duplicate" />
            <IconButton icon={Trash} label={p('Clear')} disabled={readOnly || !info.selection.length} onClick={() => void run('edit.delete', {})} data-testid="layout-delete" />
          </>
        }
      />
    </PanelSection>
  )
}
