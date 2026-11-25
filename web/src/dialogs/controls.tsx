import { useState, type ReactNode } from 'react'
import { Checkbox, Choice } from '@kition/sdk/editor'
import { formatField, parseLength, unitSuffix, type Unit } from '../units'

/**
 * The dialogs' controls, on kit-editor's: a drop-down, a check box, a colour well, a number, a
 * text field and a labelled row around any field. Each reports every change at once.
 */

export function ChoiceRow<V extends string>({ label, value, choices, onChange, disabled, testId }: { label: string; value: V; choices: readonly (readonly [V, string])[]; onChange: (value: V) => void; disabled?: boolean; testId?: string }) {
  return (
    <div className="layout-dialog-row">
      <Choice label={label} value={value} choices={choices} onChange={onChange} disabled={disabled} testId={testId} />
    </div>
  )
}

export function CheckRow({ label, checked, onChange, disabled, testId }: { label: string; checked: boolean; onChange: (checked: boolean) => void; disabled?: boolean; testId?: string }) {
  return (
    <div className="layout-dialog-row">
      <Checkbox label={label} checked={checked} onChange={onChange} disabled={disabled} testId={testId} />
    </div>
  )
}

/** A labelled row around any field. */
export function FieldRow({ label, children, htmlFor }: { label: string; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="layout-dialog-row">
      <label className="layout-dialog-label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
    </div>
  )
}

export function ColorRow({ label, value, onChange, testId }: { label: string; value: string; onChange: (value: string) => void; testId?: string }) {
  return (
    <FieldRow label={label}>
      <input type="color" className="layout-dialog-color" value={value} aria-label={label} onChange={(event) => onChange(event.target.value)} data-testid={testId} />
    </FieldRow>
  )
}

/** A number to type, or empty (`null`: the engine's default when `optional`). */
export function NumberRow({ label, value, min, max, decimals, unit, onChange, testId, placeholder }: { label: string; value: number | null; min: number; max: number; decimals: number; unit?: string; onChange: (value: number | null) => void; testId?: string; placeholder?: string }) {
  const id = testId ? `${testId}-input` : undefined
  // The text being typed, kept while the field has focus ("2." is a number on its way).
  const [typing, setTyping] = useState<string | null>(null)
  return (
    <FieldRow label={label} htmlFor={id}>
      <span className="layout-dialog-number">
        <input
          id={id}
          className="kui-field tabular-nums"
          type="number"
          value={typing ?? (value ?? '')}
          min={min}
          max={max}
          step={10 ** -decimals}
          placeholder={placeholder}
          onBlur={() => setTyping(null)}
          onChange={(event) => {
            const text = event.target.value
            setTyping(text)
            if (text.trim() === '') return onChange(null)
            const next = Number(text)
            // Kept as typed (a first digit may be under the minimum); OK checks the range.
            if (Number.isFinite(next)) onChange(next)
          }}
          data-testid={testId}
        />
        {unit ? <span className="layout-dialog-unit">{unit}</span> : null}
      </span>
    </FieldRow>
  )
}

export function TextRow({ label, value, placeholder, onChange, testId, autoFocus }: { label: string; value: string; placeholder?: string; onChange: (value: string) => void; testId?: string; autoFocus?: boolean }) {
  const id = testId ? `${testId}-input` : undefined
  return (
    <FieldRow label={label} htmlFor={id}>
      <input id={id} className="kui-field" type="text" value={value} placeholder={placeholder} autoFocus={autoFocus} onChange={(event) => onChange(event.target.value)} data-testid={testId} />
    </FieldRow>
  )
}

/**
 * A length to type (plan 2026-11-25, stage 3): shown in `unit`, typed in it or with a unit of its
 * own (`10mm`, `3p6`, `0.5in`, `1in+6pt`); `value` and `onChange` are in points (`null`: empty,
 * the engine's default when the field is optional). What is typed stays as typed while the field
 * has focus.
 */
export function LengthRow({ label, value, unit, onChange, testId, placeholder }: { label: string; value: number | null; min?: number; max?: number; unit: Unit; onChange: (points: number | null) => void; testId?: string; placeholder?: string }) {
  const id = testId ? `${testId}-input` : undefined
  const [typing, setTyping] = useState<string | null>(null)
  const [wrong, setWrong] = useState(false)
  return (
    <FieldRow label={label} htmlFor={id}>
      <span className="layout-dialog-number">
        <input
          id={id}
          className="kui-field tabular-nums"
          type="text"
          inputMode="decimal"
          value={typing ?? (value === null ? '' : formatField(value, unit))}
          placeholder={placeholder}
          aria-invalid={wrong || undefined}
          data-unit={unit}
          onBlur={() => {
            setTyping(null)
            setWrong(false)
          }}
          onChange={(event) => {
            const text = event.target.value
            setTyping(text)
            if (text.trim() === '') {
              setWrong(false)
              return onChange(null)
            }
            const points = parseLength(text, unit)
            setWrong(points === null)
            // Kept as typed (a first digit may be under the minimum); OK checks the range.
            if (points !== null) onChange(points)
          }}
          data-testid={testId}
        />
        {unit === 'pc' ? null : <span className="layout-dialog-unit">{unitSuffix(unit)}</span>}
      </span>
    </FieldRow>
  )
}

/**
 * Four sides (top, bottom, inside or left, outside or right) with the chain that makes them one:
 * linked, a change to any side sets all four. In points, shown in `unit`.
 */
export type Sides = { top: number; bottom: number; inside: number; outside: number }

export function SidesRows({ label, sides, linked, labels, linkLabel, unit, onChange, onLink, testId }: { label: string; sides: Sides; linked: boolean; labels: Record<keyof Sides, string>; linkLabel: string; unit: Unit; onChange: (sides: Sides) => void; onLink: (linked: boolean) => void; testId: string }) {
  const set = (side: keyof Sides) => (value: number | null) => {
    if (value === null || !(value >= 0)) return
    onChange(linked ? { top: value, bottom: value, inside: value, outside: value } : { ...sides, [side]: value })
  }
  return (
    <fieldset className="layout-dialog-group" data-testid={testId}>
      <legend>{label}</legend>
      {(['top', 'bottom', 'inside', 'outside'] as const).map((side) => (
        <LengthRow key={side} label={labels[side]} value={sides[side]} min={0} max={100000} unit={unit} onChange={set(side)} testId={`${testId}-${side}`} />
      ))}
      <CheckRow label={linkLabel} checked={linked} onChange={onLink} testId={`${testId}-link`} />
    </fieldset>
  )
}
