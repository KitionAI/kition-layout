// Takes the README's pictures of the Layout editor (assets/readme/*.png at the repository root):
// the editor in the development host, on the showcase sample "Community Garden Tri-Fold", at
// 1440 x 900, device scale 2, Classic display mode, light theme, English. Not a test; re-run it by
// hand when the editor changes:
//
//   cd web && pnpm install && pnpm run engine      # the engine and fonts in public/
//   node scripts/readme-shots.mjs [name ...]       # every shot, or only the named ones
//
// It needs kition-sdk (the shared runner, tools/readme-shots/) and kition-showcase (the sample)
// checked out beside this repository, or KITION_SDK and KITION_SHOWCASE naming them. Run it with
// no HTTP proxy set; CHROME may name a Chrome or Chromium executable. Every step is what a person
// does in the editor: clicks on the canvas, the panels and the ribbon. When pngquant is installed,
// each picture is then shrunk with `pngquant --quality 70-90` (a README picture is at most 400 KB).
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { takeShots } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)

const SAMPLE = path.join(showcase, 'layout', 'community-garden-trifold', 'community-garden-trifold.klayout')

/** Where a point of the page in view (points from its top left corner) is on the screen, from the rulers. */
async function toScreen(kit, x, y) {
  const [zoom] = (await kit.getByTestId('layout-workspace').getAttribute('data-view')).split(' ').map(Number)
  const across = kit.getByTestId('layout-ruler-x')
  const down = kit.getByTestId('layout-ruler-y')
  const left = (await across.boundingBox()).x + Number(await across.getAttribute('data-origin'))
  const top = (await down.boundingBox()).y + Number(await down.getAttribute('data-origin'))
  return { x: left + x * zoom, y: top + y * zoom }
}

/** Opens or closes a section of the side panel by its title. */
async function section(kit, title, open) {
  const toggle = kit.locator('.kui-panel-section-toggle', { hasText: title }).first()
  if ((await toggle.getAttribute('aria-expanded')) !== String(open)) await toggle.click()
}

/** Opens the side panel's sections a shot shows and closes the others, so those have the room. */
async function only(kit, page, ...titles) {
  for (const title of ['Properties', 'Pages', 'Sizes', 'Layers', 'Swatches', 'Paragraph Styles', 'Character Styles', 'Links']) {
    await section(kit, title, titles.includes(title))
  }
  await page.waitForTimeout(300)
}

/** A click on a point of the page in view. */
async function click(page, kit, x, y, options = {}) {
  const at = await toScreen(kit, x, y)
  await page.mouse.click(at.x, at.y, options)
  await page.waitForTimeout(400)
}

const report = await takeShots({
  web,
  sample: SAMPLE,
  query: { modes: 'classic' },
  ready: '[data-testid="layout-ruler-x"][data-origin]',
  settle: { open: 3000, shot: 1000 },
  shots: [
    // The outside of the tri-fold as it opens: the inside flap, the back panel and the cover.
    { name: 'hero' },
    {
      // The caret in the opening-hours list; the paragraph styles of the brochure beside it.
      name: 'see-1',
      steps: async (page, { kit }) => {
        await only(kit, page, 'Properties', 'Paragraph Styles')
        await click(page, kit, 340, 104, { clickCount: 2 })
        await page.waitForTimeout(600)
      },
    },
    {
      // The cover's flower selected, with its handles; the document's swatches in the panel.
      name: 'see-2',
      steps: async (page, { kit }) => {
        await only(kit, page, 'Properties', 'Swatches')
        await click(page, kit, 612, 110)
        // The panel scrolled to its end: the fill in Properties, and the brochure's swatches below it.
        await kit.locator('.kui-panel-body').evaluate((body) => { body.scrollTop = body.scrollHeight })
      },
    },
    {
      // One of the three flowers, a group, selected; the Layers panel lists the page's objects.
      name: 'see-3',
      steps: async (page, { kit }) => {
        await only(kit, page, 'Layers')
        await click(page, kit, 145, 500)
      },
    },
    {
      // The inside of the brochure, chosen in the Pages panel: the story, the membership prices and the calendar.
      name: 'see-4',
      steps: async (page, { kit }) => {
        await only(kit, page, 'Properties', 'Pages')
        await kit.getByTestId('layout-spread-1').click()
        await page.waitForTimeout(800)
        await kit.getByTestId('layout-page-1').click()
      },
    },
  ],
})

// Shrinks the pictures when pngquant is there (it keeps a file it cannot make smaller).
const repo = path.resolve(web, '..')
for (const { file } of report) {
  try {
    execFileSync('pngquant', ['--quality', '70-90', '--force', '--skip-if-larger', '--output', file, file], { cwd: repo, stdio: 'ignore' })
  } catch (error) {
    if (error.code === 'ENOENT') break
  }
  console.log(`${file}  ${Math.ceil(fs.statSync(path.join(repo, file)).size / 1024)} KB`)
}
