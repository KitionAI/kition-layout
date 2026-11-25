import { useState } from 'react'
import { Button, ColorPicker, NumberField, Popover, PopoverContent, PopoverTrigger, Segmented, useKitEditor } from '@kition/sdk/editor'
import { pickedRgb, rgbToCmyk, rgbToHex, type Cmyk, type PickedColor, type Rgb } from './colors'

/**
 * The colour picker everywhere a colour is chosen (plan 2026-11-25, stage 4): kit-editor's
 * `ColorPicker` (saturation and brightness, hue, hex) with the colour's parts to type, in RGB or
 * in CMYK as a printer reads them. A colour defined in CMYK stays CMYK. `onChange` reports every
 * move (`final` false mid-drag) and the last value (`final` true).
 */
export function ColorEditor({ value, onChange, testId }: { value: PickedColor; onChange: (color: PickedColor, final: boolean) => void; testId: string }) {
  const { t } = useKitEditor()
  const [model, setModel] = useState<'rgb' | 'cmyk'>(value.model)
  const rgb = pickedRgb(value)
  const cmyk: Cmyk = value.model === 'cmyk' ? value.cmyk : rgbToCmyk(rgb)
  const fromPicker = (next: Rgb, final: boolean) => onChange(model === 'cmyk' ? { model: 'cmyk', cmyk: rgbToCmyk(next) } : { model: 'rgb', rgb: next }, final)
  const setRgb = (index: number) => (part: number, final: boolean) => {
    const next: [number, number, number] = [rgb[0], rgb[1], rgb[2]]
    next[index] = part
    onChange({ model: 'rgb', rgb: next }, final)
  }
  const setCmyk = (index: number) => (part: number, final: boolean) => {
    const next: [number, number, number, number] = [cmyk[0], cmyk[1], cmyk[2], cmyk[3]]
    next[index] = part
    onChange({ model: 'cmyk', cmyk: next }, final)
  }
  return (
    <div className="layout-color-editor" data-testid={testId}>
      <ColorPicker value={rgb} onChange={fromPicker} testId={`${testId}-picker`} />
      <Segmented
        label={t('colorModel')}
        value={model}
        segments={[
          { value: 'rgb', label: 'RGB' },
          { value: 'cmyk', label: 'CMYK' },
        ]}
        onChange={setModel}
        testId={`${testId}-model`}
      />
      <div className="layout-color-parts" data-model={model}>
        {model === 'rgb'
          ? (['R', 'G', 'B'] as const).map((part, index) => <NumberField key={part} label={part} value={rgb[index]} min={0} max={255} step={1} onChange={setRgb(index)} testId={`${testId}-${part.toLowerCase()}`} />)
          : (['C', 'M', 'Y', 'K'] as const).map((part, index) => <NumberField key={part} label={part} value={cmyk[index]} min={0} max={100} step={1} unit="%" onChange={setCmyk(index)} testId={`${testId}-${part.toLowerCase()}`} />)}
      </div>
    </div>
  )
}

/**
 * A colour well: the chip of the current colour (empty for none) that opens the colour picker, and
 * under it "Add to Swatches" when `onAddSwatch` is given. The picker keeps the person's colour
 * while it is open (the engine's answer comes a moment later).
 */
export function ColorWell({ label, color, onChange, onAddSwatch, testId, disabled }: { label: string; color: PickedColor | null; onChange: (color: PickedColor, final: boolean) => void; onAddSwatch?: (color: PickedColor) => void; testId: string; disabled?: boolean }) {
  const { t } = useKitEditor()
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState<PickedColor | null>(null)
  const shown = draft ?? color ?? { model: 'rgb', rgb: [0, 0, 0] }
  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) setDraft(null)
      }}
    >
      <PopoverTrigger asChild>
        <button type="button" className="layout-well" aria-label={label} title={label} disabled={disabled} data-none={color ? undefined : true} style={color ? { background: rgbToHex(pickedRgb(color)) } : undefined} data-testid={testId} />
      </PopoverTrigger>
      <PopoverContent align="start" className="layout-color-popover">
        <ColorEditor
          value={shown}
          onChange={(next, final) => {
            setDraft(next)
            onChange(next, final)
          }}
          testId={`${testId}-editor`}
        />
        {onAddSwatch ? (
          <Button onClick={() => onAddSwatch(shown)} data-testid={`${testId}-add`}>
            {t('addToSwatches')}
          </Button>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}
