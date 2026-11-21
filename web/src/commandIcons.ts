/**
 * Tessera icons for the Layout engine's commands that kit-editor's rules cannot read or would give a
 * shared mark: its own vocabulary, by command id. Registered once, when the editor loads
 * (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { registerCommandIcons } from '@kition/sdk/editor'
import { BracesIcon, BringToFrontIcon, CodeXmlIcon, CopyIcon, FeatherIcon, FileCodeIcon, FlagIcon, FrameIcon, GemIcon, ImageIcon, LinkIcon, ListOrderedIcon, PackageIcon, ParallelogramIcon, ScalingIcon, SendToBackIcon, SlidersHorizontalIcon, SquareFunctionIcon, SquareSquareIcon, SquareTerminalIcon, SquaresUniteIcon, SunIcon, SunDimIcon, TagIcon, UnlinkIcon, WavesHorizontalIcon } from '@kition/tessera/icons/react'

registerCommandIcons({
  "anchored.release": UnlinkIcon,
  "conveyor.collect": PackageIcon,
  "index.generate": ListOrderedIcon,
  "layout.detachAll": UnlinkIcon,
  "library.json": BracesIcon,
  "math.svg": SquareFunctionIcon,
  "object.attributes": SlidersHorizontalIcon,
  "object.bevel": GemIcon,
  "object.content": ImageIcon,
  "object.directionalFeather": FeatherIcon,
  "object.dropShadow": CopyIcon,
  "object.feather": FeatherIcon,
  "object.fit": ScalingIcon,
  "object.innerGlow": SunDimIcon,
  "object.innerShadow": SquareSquareIcon,
  "object.label": TagIcon,
  "object.outerGlow": SunIcon,
  "object.pathfinder": SquaresUniteIcon,
  "object.satin": WavesHorizontalIcon,
  "object.setFlags": FlagIcon,
  "script.run": SquareTerminalIcon,
  "select.container": FrameIcon,
  "select.content": ImageIcon,
  "select.firstAbove": BringToFrontIcon,
  "select.lastBelow": SendToBackIcon,
  "states.release": UnlinkIcon,
  "transform.shear": ParallelogramIcon,
  "xml.loadDtd": FileCodeIcon,
  "xml.structure": CodeXmlIcon,
  "xref.formats": LinkIcon,
  "xref.setFormat": LinkIcon,
})
