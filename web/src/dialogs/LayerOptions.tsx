import { useMemo, useState } from 'react'
import { Checkbox, Choice, useKitEditor } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { DialogFrame, useRunInDialog } from './DialogFrame'
import { useCommandPreview } from './preview'
import { LAYER_COLORS, layerEdit, type LayerDraft } from '../panels/reorder'

/**
 * Layer Options (plan 2026-11-25, stage 7): the layer's name, its colour (the frame edges and
 * selections of its objects), and whether it shows, is locked and prints. The changes show at
 * once (`layer.set`'s preview); OK keeps them as one undo step.
 */
export function LayerOptionsDialog({ info, id, onClose }: { info: DocumentInfo; id: number; onClose: () => void }) {
  const { t } = useKitEditor()
  const run = useRunInDialog()
  const layer = info.layers.find((entry) => entry.id === id)
  const opened = useMemo<LayerDraft>(
    () => ({ name: layer?.name ?? '', color: layer?.color ?? [0, 0, 0], visible: layer?.visible ?? true, locked: layer?.locked ?? false, printable: layer?.printable ?? true }),
    // The values it opens with stay fixed while the preview changes the layer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  )
  const [draft, setDraft] = useState(opened)
  const params = layer ? layerEdit(id, opened, draft) : null
  const preview = useCommandPreview('layer.set', params)
  const colorKey = draft.color.join(',')
  const named = LAYER_COLORS.find(([, rgb]) => rgb.join(',') === colorKey)
  const colors = [...(named ? [] : [[colorKey, t('layer.customColor')] as const]), ...LAYER_COLORS.map(([name, rgb]) => [rgb.join(','), t(`layerColor.${name}`)] as const)]
  return (
    <DialogFrame
      command="layer.set"
      title={t('layer.optionsTitle', { name: opened.name })}
      testId="layout-layer-options"
      preview={preview}
      onOk={async () => {
        if (!params) return
        if (preview && (await preview.keep(params))) return
        await run('layer.set', params)
      }}
      onClose={onClose}
    >
      <label className="layout-field">
        <span>{t('layer.name')}</span>
        <input className="kui-field" value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} data-testid="layout-layer-options-name" />
      </label>
      <span className="layout-color">
        <Choice label={t('layer.color')} value={colorKey} choices={colors} onChange={(key) => setDraft({ ...draft, color: key.split(',').map(Number) })} testId="layout-layer-options-color" />
        <span className="layout-layer-color" style={{ background: `rgb(${colorKey})` }} aria-hidden="true" />
      </span>
      <Checkbox label={t('layer.show')} checked={draft.visible} onChange={(visible) => setDraft({ ...draft, visible })} testId="layout-layer-options-visible" />
      <Checkbox label={t('layer.lock')} checked={draft.locked} onChange={(locked) => setDraft({ ...draft, locked })} testId="layout-layer-options-locked" />
      <Checkbox label={t('layer.print')} checked={draft.printable} onChange={(printable) => setDraft({ ...draft, printable })} testId="layout-layer-options-printable" />
    </DialogFrame>
  )
}
