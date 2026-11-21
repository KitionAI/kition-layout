import type { KitModuleManifest } from '@kition/sdk/editor'
import { canvasColor } from './backdrop'

/** The package's fonts: a CJK fallback and the four OFL families of Kition's designs. */
export const PACKAGE_FONTS = ['DroidSansFallbackFull.ttf', 'Inter-Variable.ttf', 'Lora-Variable.ttf', 'JetBrainsMono-Variable.ttf', 'BricolageGrotesque-Variable.ttf']

/** The engine, from this package's own folder, on the host's canvas color. Called after the theme is applied. */
export function engineManifest(): KitModuleManifest {
  const canvas = canvasColor()
  return {
    kitId: 'kit.layout',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_layout_kit',
    documentClass: 'LayoutDocument',
    setup: canvas ? [{ method: 'setBackdrop', args: [canvas] }] : [],
    // Faces the web build does not embed (build-engine.sh copies them from kition-fonts).
    fonts: PACKAGE_FONTS.map((file) => new URL(`./fonts/${file}`, document.baseURI).href),
  }
}
