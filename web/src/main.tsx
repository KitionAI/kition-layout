import { createRoot } from 'react-dom/client'
import { connectHost, createKitStrings, loadPhrases, withPhrases } from '@kition/sdk/editor'
import '@kition/sdk/editor/styles.css'
import './app.css'
import './commandIcons'
import { App } from './App'
import { CATALOGS, PHRASES, withEllipsis } from './strings'
import manifest from '../../kit.json'

const root = createRoot(document.getElementById('root')!)

// Opened outside a host, the page says where it belongs instead of waiting forever.
const alone = window.setTimeout(() => {
  const t = createKitStrings(CATALOGS, navigator.language)
  root.render(<div className="kui-alone">{t('noHost')}</div>)
}, 3000)

// `replica`: kit-editor's runtime keeps the layout's KDM replica for the host (`replica/*`), so a
// host co-edits it through its sync room (Kition Server) and keeps it beside the file.
void connectHost({ id: 'kit.layout', version: '0.1.4', capabilities: ['ai', 'replica'], brand: manifest.ui.brand, display: manifest.ui.display }).then(async (host) => {
  window.clearTimeout(alone)
  // Kition Layout ships no catalogs of its own; this editor's phrases translate its labels.
  const phrase = withEllipsis(withPhrases(await loadPhrases({}, host.init.locale), PHRASES, host.init.locale))
  // No StrictMode: its doubled effects would open the document twice, in two engine workers.
  root.render(<App host={host} phrase={phrase} />)
})
