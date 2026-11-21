import { BringToFrontIcon as BringToFront, DownloadIcon as Download, FilePlusCornerIcon as FilePlus2, FrameIcon as Frame, GroupIcon as Group, ImagePlusIcon as ImagePlus, LayoutTemplateIcon as LayoutTemplate, PilcrowIcon as Pilcrow, SendToBackIcon as SendToBack, Settings2Icon as Settings2, TypeIcon as Type, UngroupIcon as Ungroup } from '@kition/tessera/icons/react'
import { CommandButton, MenuItem, RibbonButton, RibbonGroup, RibbonMenu, RibbonStack, ToolButton, base64, toolIcon, useHostPick, useKitEditor, useKitTools } from '@kition/sdk/editor'
import type { DocumentInfo } from './layout'
import { PLACE_TYPES } from './panels/Links'

/**
 * One of the engine's tools by id (the rail draws it too, so it has its own test id). The rail
 * always shows the tools, so they are not `primary`: Classic's ribbon has them, Single line's row
 * does not repeat them.
 */
function HomeTool({ id, primary, size = 'large' }: { id: string; primary?: boolean; size?: 'large' | 'small' }) {
  const tools = useKitTools()
  const { phrase: p } = useKitEditor()
  const tool = tools?.tools.find((item) => item.id === id)
  if (!tool) return null
  return <ToolButton tool={id} icon={toolIcon(tool.icon, id)} label={p(tool.label)} size={size} testId={`layout-home-tool-${id}`} primary={primary} />
}

/**
 * Layout's Home tab: the layout tasks done most, in every display mode (a drop-down panel in
 * Menus): frames and shapes, placing an image, pages, styles, arranging, and export.
 */
export function HomeTab({ onDialog }: { onDialog: (dialog: 'setup' | 'export' | 'templates') => void }) {
  const { run, enabled, phrase: p, t, session } = useKitEditor()
  const info = session.state as DocumentInfo | null
  const { can, pick } = useHostPick()
  // As the Links panel's Place: into the one selected frame, else onto the page.
  const place = async () => {
    const file = await pick(t('placePurpose'), PLACE_TYPES)
    if (!file) return
    const frame = info?.selection.length === 1 ? info.selection[0] : undefined
    await run('file.place', { base64: base64(file.bytes), name: file.name, ...(frame != null ? { frame } : {}) })
  }
  return (
    <>
      <RibbonGroup label={t('home.frames')} icon={Frame}>
        <HomeTool id="type" />
        <HomeTool id="rectangleFrame" />
        <HomeTool id="rectangle" />
        <HomeTool id="line" />
        <RibbonStack>
          <HomeTool id="ellipseFrame" size="small" />
          <HomeTool id="ellipse" size="small" />
          <HomeTool id="pen" size="small" />
        </RibbonStack>
        <RibbonButton icon={ImagePlus} label={t('home.place')} size="large" disabled={!can || !enabled('file.place')} onClick={() => void place()} data-testid="layout-home-place" primary />
      </RibbonGroup>

      <RibbonGroup label={p('Pages')} icon={FilePlus2}>
        <CommandButton command="layout.pages.insert" params={{ count: 1 }} icon={FilePlus2} size="large" primary />
        <RibbonStack>
          <CommandButton command="layout.parents.new" icon={LayoutTemplate} primary />
          <RibbonButton icon={Settings2} label={t('documentSetupMenu')} onClick={() => onDialog('setup')} data-testid="layout-home-document-setup" primary />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('home.styles')} icon={Pilcrow}>
        <RibbonMenu icon={Pilcrow} label={p('Paragraph Styles')} size="large" disabled={!enabled('style.paragraph.apply')} testId="layout-home-paragraph-style" primary>
          {(info?.paragraphStyles ?? []).map((name) => (
            <MenuItem key={name} label={name} onSelect={() => void run('style.paragraph.apply', { name })} testId={`layout-home-paragraph-style-${name}`} />
          ))}
        </RibbonMenu>
        <RibbonMenu icon={Type} label={p('Character Styles')} disabled={!enabled('style.character.apply')} testId="layout-home-character-style">
          {(info?.characterStyles ?? []).map((name) => (
            <MenuItem key={name} label={name} onSelect={() => void run('style.character.apply', { name })} testId={`layout-home-character-style-${name}`} />
          ))}
        </RibbonMenu>
      </RibbonGroup>

      <RibbonGroup label={p('Arrange')} icon={Group}>
        <CommandButton command="object.group" icon={Group} size="large" primary />
        <RibbonStack>
          <CommandButton command="object.ungroup" icon={Ungroup} />
          <CommandButton command="object.bringForward" icon={BringToFront} />
          <CommandButton command="object.sendBackward" icon={SendToBack} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('home.output')} icon={Download}>
        <RibbonButton icon={Download} label={t('exportMenu')} size="large" onClick={() => onDialog('export')} data-testid="layout-home-export" primary />
        <RibbonButton icon={LayoutTemplate} label={t('templatesMenu')} onClick={() => onDialog('templates')} data-testid="layout-home-templates" primary />
      </RibbonGroup>
    </>
  )
}
