// The editor in each display mode, through the development host: node tests/modes.mjs [chromium|webkit]
import './platform.mjs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/kit-dev-host/mode-smoke'

await runModeSmoke({
  kit: 'kit.layout',
  config: new URL('../vite.config.ts', import.meta.url).pathname,
  browser: process.argv[2] === 'webkit' ? webkit : chromium,
  createServer,
  // The rail's tools are not in the row (D3): the rail always shows them.
  line: { tab: 'home', primary: ['layout-home-place', 'layout-cmd-layout.pages.insert', 'layout-cmd-layout.parents.new', 'layout-home-document-setup', 'layout-home-paragraph-style', 'layout-home-export', 'layout-home-templates'] },
})
