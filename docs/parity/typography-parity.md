# Typography and composition parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe InDesign 2026 (21.6)

Typesetting is what InDesign is bought for. A feature can exist and still set text differently
from InDesign: different line breaks, different page count, overset where InDesign fits. This
document tracks both: does the attribute exist, and does it compose like InDesign? Script-specific
detail lives in [cjk-typography.md](../features/cjk-typography.md) and
[arabic-typography.md](../features/arabic-typography.md). Overall numbers: [target-app-parity.md](target-app-parity.md).

**Dimension score: ~45% ready (estimated), 120–200 h to parity.** Presence is high (76% scored below, nothing absent); fidelity
against InDesign is **unmeasured**, which is the main gap.

## Fidelity: how we'd know

There is no oracle yet. What exists: unit tests of break points and invariants in
`crates/engine/compose/src/tests.rs` (165 tests in the crate), and IDML files built by Kition Layout and opened
in InDesign 2026 by hand.

**Needed:** a set of synthetic stories (our own text and fonts:
Source Serif 4, Inter, Shippori Mincho from kition-fonts) covering each row below, set in InDesign
2026 by observation on a maintainer's Mac (results under `plan/`, never committed), exported as IDML;
Kition Layout opens the IDML and a test compares per paragraph: line count, break positions (by
character offset), last-line width, overset and frame fit. Report the share of paragraphs whose
breaks match exactly. Target for beta: ≥ 90% of Latin body paragraphs identical, 100% fit in the
same frames.

## Checklist

Status: **✓** present and believed close · **~** present, known differences or partial · **✗** missing.
Fidelity: **unmeasured** unless stated.

| Area | InDesign 2026 | Kition Layout | Status | Notes / evidence |
|---|---|---|---|---|
| Paragraph Composer | Adobe's multi-line composer | Knuth–Plass total fit, fitness classes, hyphen penalties | ~ | same family of algorithm, Adobe's weights unknown; breaks will differ on some paragraphs |
| Single-line Composer | greedy | greedy with the same ranges | ✓ | |
| World-Ready composers | Paragraph / Single-line World-Ready | RTL, Arabic and bidi in the main composer | ~ | Indic scripts (Devanagari etc.) shape through harfrust but are untested |
| Justification | word/letter spacing, glyph scaling min/desired/max, single-word, last line | all of these | ✓ | a justified line with a tab justifies after the tab |
| Hyphenation | per-language dictionaries (~40), zone, limits, capitalized, last word, across columns | Liang patterns: English (trained from the public-domain Moby list) and Spanish (Bezos); zone, limits, exceptions in the user dictionary | ~ | other languages don't hyphenate |
| Spelling | per-language dictionaries | English (Moby) + document/user dictionary | ~ | |
| Kerning | Metrics, Optical, manual, none | Metrics, manual, none | ~ | **Optical treated as Metrics**: imported display text sets wider and can overset |
| Tracking, scale, skew, baseline shift | ✓ | ✓ | ✓ | |
| OpenType features | ligatures, discretionary, swash, fractions, ordinals, figures, small caps, stylistic sets, positional forms | through harfrust | ✓ | |
| Variable fonts | axes in Character panel | named instances and free axis values; PDF embeds the instance | ✓ | |
| Leading | auto (120%), absolute, leading model | ✓ | ✓ | CJK leading models in [cjk-typography.md](../features/cjk-typography.md) |
| First baseline | ascent, cap height, x-height, leading, fixed, min | ✓ | ✓ | |
| Frame fit / overset | baseline of the last line must fit | descenders must fit | ~ | **differs**: frames go overset where InDesign fits |
| Baseline grid | document and frame grids | ✓ | ✓ | |
| Vertical justification | top, centre, bottom, justify, paragraph spacing limit | ✓ | ✓ | |
| Columns, span / split columns | ✓ | ✓ | ✓ | split columns within a column; text after a span flows below it |
| Column rules | ✓ | ✓ (screen, PDF, IDML) | ~ | no stroke types or overprint |
| Keep options and breaks | keep with next, lines together, start in next column/frame/page/odd/even | ✓ | ✓ | break characters end paragraphs; the line before a break is a last line |
| Drop caps, nested, line and GREP styles | ✓ | ✓ | ✓ | |
| Bullets and numbering | lists, levels, formats, restart, across stories | ✓ | ~ | numbering expressions from IDML; Convert Bullets/Numbering to Text missing |
| Tabs | left/center/right/decimal/align-on, leaders, right-indent tab | ✓ | ~ | **Tabs panel missing** |
| Paragraph rules, borders, shading | ✓ | ✓ | ✓ | |
| Balance ragged lines | ✓ | ✓ | ✓ | |
| Optical margin alignment | ✓ | ✓ (hang table of our own) | ~ | hang amounts are ours, InDesign's unknown |
| Text wrap | bounding box, shape, jump object/column, contour options | ✓ | ✓ | |
| Anchored objects | inline, above line, custom | ✓ | ~ | anchored objects in table cells not drawn |
| Footnotes, endnotes | ✓ | ✓ | ~ | footnotes don't split across columns yet |
| Type on a path | ✓ | ✓ | ✓ | |
| Vertical text (CJK) | ✓ | ✓ | ~ | see [cjk-typography.md](../features/cjk-typography.md) |
| CJK composition | mojikumi, kinsoku, aki, tsume, ruby, kenten, warichu, frame grid | most; mojikumi tables kept but not applied; ruby group-only; no frame grid | ~ | cjk-typography.md |
| Arabic / Hebrew | kashida, digits, diacritic positioning, RTL stories and tables | ✓ | ~ | vendor justification presets unsupported; list bullets and multi-column RTL open |
| Missing glyphs and fonts | pink highlight, substitution | ✓ | ✓ | |
| Installed fonts | every activated font | system font folders; user folders, font managers and Adobe Fonts not always found | ~ | |
| Composition highlights | H&J, keeps, custom tracking/kerning, substituted fonts | ✓ | ✓ | |

Scored from the table above: 34 rows, 18 ✓, 16 ~, 0 ✗, scoring ✓ = 1 and ~ = 0.5:
**76%**. Every row exists in some form; the ~ rows are where InDesign users will see a difference.

## Remaining effort

| Work | Opus 5.5 h |
|---|---|
| Oracle harness (synthetic stories, IDML exchange, comparison metric) | 20–30 |
| Optical kerning approximation (outline-based side-bearing model) + preflight warning | 15–25 |
| Overset rule (baseline fits) and frame-fit parity | 5–10 |
| Fixing composer differences the harness finds (weights, penalties, glyph-scaling order) | 40–70 |
| Hyphenation patterns for the top 15 InDesign languages (permissive sources) | 20–35 |
| Spelling dictionaries beyond English (license review per language) | 10–20 (+ maintainer review) |
| Tabs panel, list conversion commands | 8–15 |
| **Total** | **120–200** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
