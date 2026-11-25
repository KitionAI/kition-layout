import {
  ListIcon,
  ListOrderedIcon,
  TextAlignCenterIcon,
  TextAlignEndIcon,
  TextAlignJustifyIcon,
  TextAlignStartIcon,
  TextJustifyCenterIcon,
  TextJustifyEndIcon,
  TextJustifyStartIcon,
} from '@kition/tessera/icons/react'
import { Checkbox, Choice, IconButton, NumberField, Segmented, useKitEditor } from '@kition/sdk/editor'
import { useEditorView } from '../../viewContext'
import { LengthField, PropertyGroup } from '../fields'
import { PARAGRAPH_ALIGNS } from '../panelLogic'

/** The resolved paragraph attributes at the text selection (`text.selectionAttrs`'s `para`). */
export type ParaAttrs = {
  align?: string
  leftIndent?: number
  rightIndent?: number
  firstLineIndent?: number
  lastLineIndent?: number
  spaceBefore?: number
  spaceAfter?: number
  dropCapLines?: number
  dropCapChars?: number
  hyphenate?: boolean
  listType?: string
  gridAlign?: string
}

const ALIGN_ICONS = {
  left: TextAlignStartIcon,
  center: TextAlignCenterIcon,
  right: TextAlignEndIcon,
  leftJustified: TextJustifyStartIcon,
  centerJustified: TextJustifyCenterIcon,
  rightJustified: TextJustifyEndIcon,
  fullyJustified: TextAlignJustifyIcon,
} as const

/**
 * Paragraph (InDesign's Paragraph panel and the Control panel's paragraph row): the seven
 * alignments, left, right, first-line and last-line indents, space before and after, drop cap
 * lines and characters, hyphenation, bullets and numbering (the paragraph's list type), and
 * alignment to the baseline grid. Each sets the paragraphs of the selected text (or of the
 * selected frames' stories).
 */
export function ParagraphGroup({ para }: { para: ParaAttrs }) {
  const { run, t } = useKitEditor()
  const { unit } = useEditorView().view
  const set = (attrs: Record<string, unknown>) => void run('text.para', { attrs })
  const indent = (key: keyof ParaAttrs, testId: string, label: string, min = -10000) => (
    <LengthField label={label} value={typeof para[key] === 'number' ? (para[key] as number) : 0} unit={unit} min={min} onCommit={(points) => set({ [key]: points })} testId={testId} />
  )
  const list = para.listType ?? 'none'
  const align = (PARAGRAPH_ALIGNS as readonly string[]).includes(para.align ?? 'left') ? (para.align ?? 'left') : 'left'
  return (
    <PropertyGroup title={t('props.paragraph')} testId="layout-paragraph">
      <div className="layout-stack">
        <Segmented
          label={t('alignment')}
          value={align as (typeof PARAGRAPH_ALIGNS)[number]}
          segments={PARAGRAPH_ALIGNS.map((value) => ({ value, icon: ALIGN_ICONS[value], label: t(`align.${value}`) }))}
          onChange={(next) => set({ align: next })}
          testId="layout-align"
        />
        <div className="layout-grid layout-grid-tight">
          {indent('leftIndent', 'layout-left-indent', t('props.leftIndent'))}
          {indent('rightIndent', 'layout-right-indent', t('props.rightIndent'))}
          {indent('firstLineIndent', 'layout-first-indent', t('props.firstIndent'))}
          {indent('lastLineIndent', 'layout-last-indent', t('props.lastIndent'))}
          {indent('spaceBefore', 'layout-space-before', t('props.spaceBefore'), 0)}
          {indent('spaceAfter', 'layout-space-after', t('props.spaceAfter'), 0)}
          <NumberField label={t('props.dropCapLines')} value={para.dropCapLines ?? 0} min={0} max={25} step={1} onChange={(value, final) => final && set({ dropCapLines: Math.round(value) })} testId="layout-drop-lines" />
          <NumberField label={t('props.dropCapChars')} value={para.dropCapChars ?? 0} min={0} max={150} step={1} onChange={(value, final) => final && set({ dropCapChars: Math.round(value) })} testId="layout-drop-chars" />
        </div>
        <div className="layout-variant-row">
          <IconButton icon={ListIcon} label={t('props.bullets')} pressed={list === 'bullets'} onClick={() => set({ listType: list === 'bullets' ? 'none' : 'bullets' })} data-testid="layout-bullets" />
          <IconButton icon={ListOrderedIcon} label={t('props.numbers')} pressed={list === 'numbers'} onClick={() => set({ listType: list === 'numbers' ? 'none' : 'numbers' })} data-testid="layout-numbers" />
          <Checkbox label={t('props.hyphenate')} checked={para.hyphenate ?? true} onChange={(hyphenate) => set({ hyphenate })} testId="layout-hyphenate" />
        </div>
        <Choice label={t('props.gridAlign')} value={para.gridAlign ?? 'none'} choices={(['none', 'allLines', 'firstLineOnly'] as const).map((value) => [value, t(`gridAlign.${value}`)] as const)} onChange={(gridAlign) => set({ gridAlign })} testId="layout-grid-align" />
      </div>
    </PropertyGroup>
  )
}
