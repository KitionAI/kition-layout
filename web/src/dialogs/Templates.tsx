import { Button, Dialog, useEngineQuery, useKitEditor } from '@kition/sdk/editor'

type Template = { id: string; name: string; width: number; height: number }

/** A template's preview, drawn from it when the package was made (`templates/<id>.png`). */
const preview = (id: string) => new URL(`./templates/${id}.png`, document.baseURI).href

/**
 * File › New from Template: Kition's starter layouts (Editorial, Event, Promotion, Quote). The
 * chosen one replaces the document's pages, parents, swatches and styles in one step (Undo brings
 * the document back); its named swatches (Primary, Text, Accent, Surface) recolour it.
 */
export function TemplatesDialog({ onClose }: { onClose: () => void }) {
  const { run, t } = useKitEditor()
  const templates = useEngineQuery<Template[]>('template.list', {}, 'templates') ?? []
  const choose = async (id: string) => {
    await run('file.applyTemplate', { template: id })
    onClose()
  }
  return (
    <Dialog title={t('templates')} onClose={onClose} testId="layout-templates-dialog" actions={<Button onClick={onClose}>{t('cancel')}</Button>}>
      <div className="layout-templates" role="list">
        {templates.map((template) => (
          <button key={template.id} type="button" role="listitem" className="layout-template" onClick={() => void choose(template.id)} data-testid={`layout-template-${template.id}`}>
            <img className="layout-template-preview" src={preview(template.id)} alt="" style={{ aspectRatio: `${template.width} / ${template.height}` }} />
            <span>{t(`template.${template.id}`)}</span>
            <span className="kui-muted">{`${template.width} × ${template.height} px`}</span>
          </button>
        ))}
      </div>
      <p className="kui-muted layout-note">{t('templatesNote')}</p>
    </Dialog>
  )
}
