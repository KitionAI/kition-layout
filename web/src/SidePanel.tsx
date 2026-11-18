import type { DocumentInfo, Spread } from './layout'
import { LayersSection } from './panels/Layers'
import { LinksSection } from './panels/Links'
import { PagesSection } from './panels/Pages'
import { PropertiesSection } from './panels/Properties'
import { SizesSection } from './panels/Sizes'
import { StylesSection } from './panels/Styles'
import { SwatchesSection } from './panels/Swatches'

/**
 * InDesign's panels beside the spreads: Properties, Pages, Sizes (alternate layouts), Layers,
 * Swatches, Paragraph and Character Styles, and Links. `spread` is the one in view (the Layers panel lists its objects).
 */
export function SidePanel({ info, spread, onShow, onSetup }: { info: DocumentInfo; spread: Spread | null; onShow: (spread: Spread) => void; onSetup: () => void }) {
  return (
    <aside className="kui-panel layout-side" data-side="right" data-testid="layout-side">
      <div className="kui-panel-body">
        <PropertiesSection info={info} />
        <PagesSection info={info} onShow={onShow} onSetup={onSetup} />
        <SizesSection info={info} onShow={onShow} />
        <LayersSection info={info} spread={spread} />
        <SwatchesSection info={info} />
        <StylesSection info={info} kind="paragraph" />
        <StylesSection info={info} kind="character" />
        <LinksSection info={info} />
      </div>
    </aside>
  )
}
