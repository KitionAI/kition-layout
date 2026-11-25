/** The development host (dev.html): opens `public/sample/Sample.klayout`, or a file chosen in the page. */
import { startDevHost } from '@kition/sdk/dev-host'

// Pictures, as the host's default, and fonts for the font menu's Load Font File… (plan 2026-11-25, stage 4).
startDevHost({ sample: './sample/Sample.klayout', pickable: /\.(png|jpe?g|gif|svg|webp|ttf|otf|ttc)$/i })
