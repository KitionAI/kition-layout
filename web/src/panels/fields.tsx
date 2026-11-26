import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import { IconButton, useKitEditor, type IconComponent } from '@kition/sdk/editor'
import { formatField, parseLength, unitSuffix, UNITS, type Unit } from '../units'
import { VARIANT_LABELS } from '../menus/catalog'
import { useDialogs } from './dialogsContext'

/**
 * A length in the panel (plan 2026-11-25, stage 4): shown in the ruler unit, typed in it or with
 * a unit of its own (`10mm`, `3p6`, `0.5in`, `1in+6pt`), as `dialogs/controls.tsx`'s `LengthRow`.
 * Enter or leaving the field sets it; Escape puts the shown value back; Up and Down step by about
 * a point (Shift ten times). `value` and `onCommit` are in points; `null` shows an empty field (a
 * value the engine does not report, or several that differ). The test id is on the label around
 * the input.
 */
export function LengthField({ label, name, value, unit, onCommit, min = -100000, max = 100000, testId, disabled }: { label: string; name?: string; value: number | null; unit: Unit; onCommit: (points: number) => void; min?: number; max?: number; testId?: string; disabled?: boolean }) {
  const [typing, setTyping] = useState<string | null>(null)
  const [wrong, setWrong] = useState(false)
  const shown = value === null || !Number.isFinite(value) ? '' : formatField(value, unit)
  const input = useRef<HTMLInputElement>(null)
  // The value changed under a focused field nobody has typed in yet (the panel caught up with a new
  // selection): its text is selected again, so what is typed next replaces it instead of going
  // after it (plan 2026-11-25, phase 10).
  useLayoutEffect(() => {
    const element = input.current
    if (element && typing === null && element.ownerDocument.activeElement === element) element.select()
  }, [shown])
  const commit = (text: string) => {
    setTyping(null)
    if (text.trim() === '' || text === shown) {
      setWrong(false)
      return
    }
    const points = parseLength(text, unit)
    if (points === null) {
      setWrong(true)
      return
    }
    setWrong(false)
    onCommit(Math.max(min, Math.min(max, points)))
  }
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') commit(event.currentTarget.value)
    else if (event.key === 'Escape') {
      setTyping(null)
      setWrong(false)
    } else if ((event.key === 'ArrowUp' || event.key === 'ArrowDown') && value !== null) {
      event.preventDefault()
      const step = lengthStep(unit) * UNITS[unit] * (event.shiftKey ? 10 : 1)
      const next = Math.max(min, Math.min(max, value + (event.key === 'ArrowUp' ? step : -step)))
      setTyping(null)
      onCommit(next)
    }
  }
  return (
    <label className="layout-length" data-testid={testId} title={name ?? label}>
      <span className="layout-length-label">{label}</span>
      <input
        ref={input}
        className="kui-field tabular-nums"
        type="text"
        inputMode="decimal"
        aria-label={name ?? label}
        aria-invalid={wrong || undefined}
        disabled={disabled}
        data-unit={unit}
        value={typing ?? shown}
        onChange={(event) => setTyping(event.target.value)}
        onBlur={(event) => commit(event.currentTarget.value)}
        onKeyDown={onKeyDown}
      />
      {unit === 'pc' ? null : <span className="layout-length-unit">{unitSuffix(unit)}</span>}
    </label>
  )
}

/** A step of about a point, a round one in the unit. */
export function lengthStep(unit: Unit) {
  return UNITS[unit] >= 72 ? 0.125 : UNITS[unit] >= 12 ? 0.25 : UNITS[unit] >= 2 ? 0.5 : 1
}

/**
 * A panel's heading inside a dock tab: its title (when the tab holds more than one panel) and its
 * buttons. Not a toggle: a tab's panels are always open.
 */
export function PanelHeading({ title, actions, testId }: { title?: string; actions?: ReactNode; testId?: string }) {
  return (
    <div className="layout-panel-heading" data-testid={testId}>
      <span className="layout-panel-heading-title">{title ?? ''}</span>
      {actions ? <span className="layout-buttons">{actions}</span> : null}
    </div>
  )
}

/** A titled group of the Properties panel (Transform, Character…): a heading and its rows. */
export function PropertyGroup({ title, children, testId }: { title: string; children: ReactNode; testId?: string }) {
  return (
    <section className="layout-property-group" data-testid={testId}>
      <h3 className="layout-property-title">{title}</h3>
      {children}
    </section>
  )
}

/**
 * A row of the menus' fixed-value buttons (Align Left, Wrap around Bounding Box…): each runs its
 * catalog row through the dialog host, as a click on the menu row does, named by the row's label.
 */
export function VariantButtons({ rows, icons, params, active, testId, disabled }: { rows: readonly string[]; icons: Readonly<Record<string, IconComponent>>; params?: Record<string, unknown>; active?: string; testId: string; disabled?: boolean }) {
  const { phrase } = useKitEditor()
  const dialogs = useDialogs()
  return (
    <div className="layout-variant-row" role="group" data-testid={testId}>
      {rows.map((row) => {
        const label = phrase(VARIANT_LABELS.get(row) ?? row)
        const variant = row.slice(row.indexOf('|') + 1)
        return (
          <IconButton
            key={row}
            icon={icons[variant] ?? icons.default}
            label={label}
            pressed={active === variant}
            disabled={disabled}
            onClick={() => dialogs?.current?.invoke(row, params ? { params } : undefined)}
            data-testid={`${testId}-${variant}`}
          />
        )
      })}
    </div>
  )
}
