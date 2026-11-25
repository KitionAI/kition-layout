import { Button, Choice, useEngineQuery, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { findItem, type DocumentInfo } from '../layout'
import { AppearanceGroup, type InspectedItem } from './properties/Appearance'
import { ArrangeGroups, TextFrameGroup } from './properties/Arrange'
import { CharacterGroup, type CharAttrs } from './properties/Character'
import { DocumentGroup } from './properties/Document'
import { ParagraphGroup, type ParaAttrs } from './properties/Paragraph'
import { TableGroup } from './properties/Table'
import { TransformGroup } from './properties/Transform'

type TextAttrs = { paragraphStyle?: string; characterStyle?: string; para?: ParaAttrs; chars?: CharAttrs }

/**
 * The Properties panel (plan 2026-11-25, stage 4; InDesign's Properties panel and the Control
 * panel's rows): what the selection is decides the groups. Objects: Transform, Appearance (fill,
 * stroke and its options, opacity, corners), Text Wrap, Align, Pathfinder, Fitting, and a text
 * frame's options. Text being edited, or selected text frames: Character, Paragraph, the styles
 * and Fit Text to Frame; with the caret in a table, the table's rows, columns and cells (stage 5). Nothing selected: the document and the pages' background. Read-only (a
 * viewer), it shows them and changes nothing.
 */
export function PropertiesPanel({ info }: { info: DocumentInfo }) {
  const { t } = useKitEditor()
  const readOnly = useReadOnly()
  const first = info.selection.length ? findItem(info, info.selection[0]) : null
  const item = first?.item as InspectedItem | undefined
  const text = info.editingText || item?.story != null
  const objects = item && !info.editingText
  return (
    <div className="layout-properties" data-testid="layout-properties">
      <fieldset className="layout-fieldset" disabled={readOnly}>
        {!item && !info.editingText ? (
          <>
            <p className="layout-empty">{t('nothingSelected')}</p>
            <DocumentGroup info={info} />
          </>
        ) : null}
        {objects ? <TransformGroup info={info} item={item} /> : null}
        {objects ? <AppearanceGroup info={info} item={item} /> : null}
        {objects && item.story != null ? <TextFrameGroup info={info} item={item} /> : null}
        {info.editingText ? <TableGroup info={info} /> : null}
        {text ? <TextGroups info={info} /> : null}
        {objects ? <ArrangeGroups info={info} item={item} /> : null}
      </fieldset>
    </div>
  )
}

/** Character, Paragraph, the styles at the selection, and Fit Text to Frame. */
function TextGroups({ info }: { info: DocumentInfo }) {
  const { run, phrase: p } = useKitEditor()
  const attrs = useEngineQuery<TextAttrs>('text.selectionAttrs', {}, `${info.selection.join(',')}:${info.editingText}`)
  if (!attrs) return null
  return (
    <div data-testid="layout-typography">
      <CharacterGroup info={info} chars={attrs.chars ?? {}} />
      <ParagraphGroup para={attrs.para ?? {}} />
      <div className="layout-stack">
        <Choice
          label={p('Paragraph Style')}
          value={attrs.paragraphStyle ?? ''}
          choices={info.paragraphStyles.map((name) => [name, name] as const)}
          onChange={(name) => void run('style.paragraph.apply', { name })}
          testId="layout-paragraph-style-choice"
        />
        <Choice
          label={p('Character Style')}
          value={attrs.characterStyle ?? '[None]'}
          choices={info.characterStyles.map((name) => [name, name] as const)}
          onChange={(name) => void run('style.character.apply', { name })}
          testId="layout-character-style-choice"
        />
        <Button onClick={() => void run('frame.fitText', info.editingText ? {} : { ids: info.selection })} data-testid="layout-fit-text">
          {p('Fit Text to Frame')}
        </Button>
      </div>
    </div>
  )
}
