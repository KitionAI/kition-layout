import { useMemo, useState } from 'react'
import { useEngineQuery, useKitEditor, type KitCommand } from '@kition/sdk/editor'
import type { DocumentInfo } from '../layout'
import { DialogFrame, useRunInDialog } from './DialogFrame'
import { CheckRow, ChoiceRow, ColorRow, NumberRow, TextRow } from './controls'
import { dialogTitle, optionLabel, paramLabel } from './labels'
import { initialValues, paramFields, paramsOf, requiredKeys, valuesValid, type ParamField, type ParamValues } from './params'

/** The values each command's dialog was last OK'd with, for the session (the next one opens with them). */
const remembered = new Map<string, ParamValues>()

/** Choices a command takes from the document rather than its schema: a parent page, a paragraph style, a text variable. */
function documentChoices(command: string, key: string, info: DocumentInfo, variables: readonly string[]): readonly string[] | null {
  if (key === 'parent') return ['', ...info.parents.map((parent) => parent.label ?? '').filter(Boolean)]
  if (key === 'style' && info.paragraphStyles.length) return ['', ...info.paragraphStyles]
  if (command === 'variable.insert' && key === 'name') return variables
  return null
}

/**
 * The generic parameter dialog (plan 2026-11-25, stage 1): a control per parameter a person sets,
 * labelled in the editor's language. It opens with the values given (where the person is: the
 * page in view) over the ones last OK'd (the engine's defaults the first time); Alt+Cancel resets
 * them. `fixed` values go with the command unasked (the spread in view).
 */
export function ParamsDialog({ command, info, values: given, fixed, onClose }: { command: KitCommand; info: DocumentInfo; values?: Record<string, unknown>; fixed?: Record<string, unknown>; onClose: () => void }) {
  const words = useKitEditor()
  const run = useRunInDialog()
  // Insert Variable offers the document's text variables.
  const listed = useEngineQuery<Array<{ name: string }>>(command.id === 'variable.insert' ? 'variable.list' : 'style.list', {}, command.id)
  const variables = useMemo(() => (command.id === 'variable.insert' && Array.isArray(listed) ? listed.map((variable) => variable.name) : []), [command.id, listed])
  const fields = useMemo<ParamField[]>(
    () =>
      paramFields(command).map((field) => {
        const options = field.kind === 'text' ? documentChoices(command.id, field.key, info, variables) : null
        return options ? { key: field.key, kind: 'choice', options, initial: options[0] ?? '' } : field
      }),
    [command, info, variables],
  )
  const defaults = useMemo(() => initialValues(fields), [fields])
  const required = useMemo(() => requiredKeys(command), [command])
  const [values, setValues] = useState<ParamValues>(() => initialValues(fields, { ...remembered.get(command.id), ...given }))
  const set = (key: string, value: ParamValues[string]) => setValues((current) => ({ ...current, [key]: value }))
  const label = (key: string) => paramLabel(words, key)
  const testId = (key: string) => `layout-dialog-param-${key}`
  const params = paramsOf(fields, values)
  // A choice of the document's (a parent, a style) left at "none" sends null, which clears it.
  for (const field of fields) if (field.kind === 'choice' && values[field.key] === '' && field.key === 'parent') params.parent = null
  return (
    <DialogFrame
      command={command.id}
      title={dialogTitle(words, command.title)}
      okDisabled={!valuesValid(fields, values, required)}
      onOk={async () => {
        await run(command.id, { ...fixed, ...params })
        remembered.set(command.id, values)
      }}
      onReset={() => setValues(defaults)}
      onClose={onClose}
    >
      <div className="layout-dialog-fields">
        {fields.map((field, index) => {
          const value = values[field.key]
          switch (field.kind) {
            case 'number':
              return <NumberRow key={field.key} label={label(field.key)} value={typeof value === 'number' ? value : null} min={field.min} max={field.max} decimals={field.decimals} unit={field.unit ?? undefined} placeholder={words.t('dialog.engineDefault')} onChange={(next) => set(field.key, next)} testId={testId(field.key)} />
            case 'choice':
              return <ChoiceRow key={field.key} label={label(field.key)} value={typeof value === 'string' ? value : field.initial} choices={field.options.map((option) => [option, field.key === 'parent' || field.key === 'style' ? option || optionLabel(words, '') : optionLabel(words, option)] as const)} onChange={(next) => set(field.key, next)} testId={testId(field.key)} />
            case 'check':
              return <CheckRow key={field.key} label={label(field.key)} checked={value === true} onChange={(next) => set(field.key, next)} testId={testId(field.key)} />
            case 'color':
              return <ColorRow key={field.key} label={label(field.key)} value={typeof value === 'string' ? value : field.initial} onChange={(next) => set(field.key, next)} testId={testId(field.key)} />
            case 'text':
            case 'list':
              return <TextRow key={field.key} label={label(field.key)} value={typeof value === 'string' ? value : ''} placeholder={field.kind === 'list' ? words.t(field.items === 'string' ? 'dialog.listOfNames' : 'dialog.listOfNumbers') : undefined} autoFocus={index === 0} onChange={(next) => set(field.key, next)} testId={testId(field.key)} />
          }
        })}
      </div>
    </DialogFrame>
  )
}
