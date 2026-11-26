import { useMemo, useRef, useState } from 'react'
import { Checkbox, Choice, useKitEditor } from '@kition/sdk/editor'
import { usePanelQuery } from './query'
import type { DocumentInfo } from '../layout'
import { DialogFrame, useRunInDialog } from '../dialogs/DialogFrame'
import { useCommandPreview } from '../dialogs/preview'
import { BUILT_IN_STYLES, sectionsOf, shownValue, styleAttrs, styleDraft, styleEdit, styleGeneral, type StyleDraft, type StyleField, type StyleGeneral, type StyleKind } from './styleRules'

/** A new style's look: the selected text's resolved attributes (`text.selectionAttrs`). */
export type NewStyle = { fromSelection: boolean; para?: Record<string, unknown>; chars?: Record<string, unknown> }

/** The character attributes a new character style takes from the selected text (InDesign's: its look, not its paragraph). */
const PICKED_CHARS = ['fontFamily', 'fontStyle', 'size', 'fill', 'tracking'] as const

/**
 * Paragraph and Character Style Options (plan 2026-11-25, stage 7): InDesign's sections in one
 * scrolling page with a list to jump to each (General with Based On and Next Style, Basic and
 * Advanced Character Formats, Indents and Spacing, Drop Caps, Hyphenation, Character Color). It
 * opens with the style's own values; a change shows on the text at once (the engine's preview of
 * `style.<kind>.edit`), Cancel takes it back, OK keeps it as one undo step. With `make`, the
 * options of a new style (from the selected text's look): OK makes it.
 */
export function StyleOptionsDialog({ info, kind, name, make, onClose, onMade }: { info: DocumentInfo; kind: StyleKind; name: string; make?: NewStyle; onClose: () => void; onMade?: (name: string) => void }) {
  const { t, phrase: p, session } = useKitEditor()
  const run = useRunInDialog()
  const families = usePanelQuery<string[]>('font.families', {}, session.doc ? 'families' : null) ?? []
  const defs = kind === 'paragraph' ? info.styleDefs.paragraph : info.styleDefs.character
  const names = kind === 'paragraph' ? info.paragraphStyles : info.characterStyles
  // The values it opens with stay fixed: the preview changes `info` as the person types.
  const opened = useMemo(() => {
    const def = defs.find((style) => style.name === name)
    const draft = make
      ? styleDraft(kind, kind === 'paragraph' ? { name, resolvedPara: make.para, resolvedChars: make.chars } : { name, resolvedChars: Object.fromEntries(PICKED_CHARS.filter((key) => make.chars?.[key] != null).map((key) => [key, make.chars?.[key]])) })
      : styleDraft(kind, def)
    return { general: styleGeneral(def, name), draft }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const [general, setGeneral] = useState<StyleGeneral>(opened.general)
  const [draft, setDraft] = useState<StyleDraft>(opened.draft)
  const builtIn = !make && BUILT_IN_STYLES.has(name)
  const command = kind === 'paragraph' ? 'style.paragraph.edit' : 'style.character.edit'
  // The edit the values make (a rename is left to OK: the preview would rename the style it shows).
  const edit = make ? null : styleEdit(kind, name, opened, { general: { ...general, name: opened.general.name }, draft })
  const preview = useCommandPreview(command, edit)
  const sections = sectionsOf(kind)
  const page = useRef<HTMLDivElement | null>(null)
  const set = (key: string, value: string) => setDraft((now) => ({ ...now, [key]: value }))
  const ok = async () => {
    if (make) {
      const made = general.name.trim() || name
      const changed = styleEdit(kind, made, opened, { general, draft })
      const attrs = styleAttrs(kind, draft)
      const params: Record<string, unknown> =
        kind === 'paragraph'
          ? { name: made, fromSelection: make.fromSelection, para: (changed?.para as object) ?? {}, chars: (changed?.chars as object) ?? {} }
          : { name: made, chars: attrs.chars }
      if (general.basedOn) params.basedOn = general.basedOn
      if (kind === 'paragraph' && general.nextStyle) params.nextStyle = general.nextStyle
      const answer = await run(kind === 'paragraph' ? 'style.paragraph.add' : 'style.character.add', params)
      onMade?.((JSON.parse(answer ?? 'null') as { name?: string } | null)?.name ?? made)
      return
    }
    const params = styleEdit(kind, name, opened, { general: builtIn ? opened.general : general, draft })
    if (!params) return
    if (preview && !params.rename && (await preview.keep(params))) return
    await run(command, params)
  }
  const others = names.filter((other) => other !== name && other !== '[No Paragraph Style]')
  const basedOnChoices = [['', kind === 'paragraph' ? p('[No Paragraph Style]') : p('[None]')] as const, ...others.filter((other) => other !== '[None]').map((other) => [other, other] as const)]
  const nextChoices = [['', t('style.sameStyle')] as const, ...names.filter((other) => other !== '[No Paragraph Style]' && other !== name).map((other) => [other, other] as const)]
  const field = (item: StyleField) => {
    const value = draft[item.key] ?? ''
    const label = t(`style.field.${item.key}`)
    const testId = `layout-style-${item.key === 'size' ? 'size' : item.key}`
    const unset = kind === 'character' ? [['', t('style.notSet')] as const] : []
    switch (item.kind) {
      case 'font': {
        const list = families.map((family) => [family, family] as const)
        if (value && !families.includes(value)) list.unshift([value, value])
        return <Choice key={item.key} label={label} value={value} choices={[...unset, ...list]} onChange={(next) => set(item.key, next)} testId={testId} />
      }
      case 'swatch': {
        const list = info.swatchColors.filter((swatch) => swatch.kind !== 'gradient').map((swatch) => [swatch.name, swatch.name] as const)
        if (value && !list.some(([known]) => known === value)) list.unshift([value, value])
        return <Choice key={item.key} label={label} value={value} choices={[...unset, ...list]} onChange={(next) => set(item.key, next)} testId={testId} />
      }
      case 'choice':
        return <Choice key={item.key} label={label} value={value} choices={[...unset, ...(item.options ?? []).map((option) => [option, t(`style.option.${option}`)] as const)]} onChange={(next) => set(item.key, next)} testId={testId} />
      case 'check':
        if (kind === 'character')
          return (
            <Choice
              key={item.key}
              label={label}
              value={value}
              choices={[...unset, ['true', t('style.on')] as const, ['false', t('style.off')] as const]}
              onChange={(next) => set(item.key, next)}
              testId={testId}
            />
          )
        return <Checkbox key={item.key} label={label} checked={value === 'true'} onChange={(on) => set(item.key, on ? 'true' : 'false')} testId={testId} />
      default:
        return (
          <label key={item.key} className="layout-field">
            <span>
              {label}
              {item.unit ? <span className="kui-muted"> ({item.key === 'leading' ? t('style.leadingUnit') : item.unit})</span> : null}
            </span>
            <input
              className="kui-field"
              inputMode={item.kind === 'number' && item.key !== 'leading' ? 'decimal' : undefined}
              value={value}
              placeholder={kind === 'character' ? t('style.notSet') : shownValue(item, null)}
              onChange={(event) => set(item.key, event.target.value)}
              data-testid={testId}
            />
          </label>
        )
    }
  }
  return (
    <DialogFrame
      command={make ? command.replace('.edit', '.add') : command}
      title={make ? t(kind === 'paragraph' ? 'newParagraphStyleOptions' : 'newCharacterStyleOptions') : t('styleOptions', { name })}
      testId="layout-style"
      preview={make ? undefined : preview}
      onOk={ok}
      onClose={onClose}
    >
      <div className="layout-options" data-testid="layout-style-dialog">
        <nav className="layout-options-nav" aria-label={t('style.sections')}>
          {sections.map((section) => (
            <button
              key={section.id}
              type="button"
              className="layout-list-item"
              onClick={() => page.current?.querySelector(`[data-section="${section.id}"]`)?.scrollIntoView({ block: 'start' })}
              data-testid={`layout-style-section-${section.id}`}
            >
              {t(`style.section.${section.id}`)}
            </button>
          ))}
        </nav>
        <div className="layout-options-page" ref={page}>
          {sections.map((section) => (
            <section key={section.id} data-section={section.id} className="layout-stack">
              <h3 className="layout-options-heading">{t(`style.section.${section.id}`)}</h3>
              {section.id === 'general' ? (
                <>
                  <label className="layout-field">
                    <span>{t('styleName')}</span>
                    <input className="kui-field" value={general.name} disabled={builtIn} onChange={(event) => setGeneral({ ...general, name: event.target.value })} data-testid="layout-style-name" />
                  </label>
                  <Choice label={t('style.basedOn')} value={general.basedOn} choices={basedOnChoices} onChange={(basedOn) => setGeneral({ ...general, basedOn })} testId="layout-style-based-on" />
                  {kind === 'paragraph' ? <Choice label={t('style.nextStyle')} value={general.nextStyle} choices={nextChoices} onChange={(nextStyle) => setGeneral({ ...general, nextStyle })} testId="layout-style-next" /> : null}
                </>
              ) : (
                <div className="layout-grid">{section.fields.map(field)}</div>
              )}
            </section>
          ))}
        </div>
      </div>
    </DialogFrame>
  )
}
