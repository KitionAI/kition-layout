# CJK import and composition

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe InDesign 2026

How the typesetting pipeline imports and composes Chinese, Japanese and Korean text. It is not a
claim of InDesign composition parity.

## Data flow

The IDML adapter resolves font resources and named/custom kinsoku sets into portable
document data. `doc::cjk` owns those resources; `CharAttrs` and `ParaAttrs` retain
inheritance and local overrides. The composer resolves composite-font character
assignments before shaping, applies character metrics, then supplies break
constraints to both line breakers. Line placement applies em alignment, leading
references and hanging punctuation. The renderer consumes placed glyphs unchanged.

`style.compositeFont.list` and `style.compositeFont.set` expose resources through the
existing command channel. Editing uses normal document history and style cache
invalidation. Font listing, replacement and preflight resolve component fonts.

## Implemented behavior

| Rule | Behavior and coverage |
| --- | --- |
| Composite fonts | Character mappings, component family/style, relative size, scales and baseline shift; synthetic selection and command/undo tests |
| Leading/trailing aki | Explicit em spacing without outline scaling; inherited resets and IDML round trip |
| Kerning import | Automatic methods take precedence over inactive numeric values; the 1e11 sentinel cannot become manual spacing |
| Tsume | Compress nonnegative glyph sidebearings without scaling ink; full compression regression |
| Jidori | Fixed group width with protected internal boundaries and justification advances; both composers tested |
| Kinsoku sets | Custom character sets and independent common punctuation defaults for named sets; explicit disabling retained |
| Bunri kinshi / rensuuji | Protect repeated dash/ellipsis pairs and numeric sequences |
| Hanging punctuation | Regular/forced edge allowance integrated with line breaking and placement |
| Ideographic spaces | Paragraph switch controls whether fullwidth spaces participate in word spacing |
| Em alignment / leading | Em top/center/bottom alignment and baseline reference offsets; mixed-size regressions |
| Kenten / glyph forms | Preserve emphasis character, shape it with font fallback; map supported OpenType forms |
| Tate-chu-yoko offsets | Carry horizontal/vertical offsets through the existing vertical group placement |

## Vertical text, Korean, ruby and languages

Each row is checked against the code and its tests.

| Area | Behavior and coverage |
| --- | --- |
| Story direction | Vertical or horizontal per story (`text.storyDirection`; IDML `StoryOrientation`); compose test `vertical_frames_compose_in_the_turned_box` |
| Vertical shaping and metrics | Upright runs are shaped top to bottom and punctuation takes its vertical form; glyphs advance by their vertical metrics (`vmtx`) and hang from their vertical origins (`VORG`, else `vmtx` and the glyph's top) on the ideographic em box (`BASE`, else OS/2 typo metrics, else ascender and descender); Latin turns as a run; with Horizontal Kana (`hkna`) on, upright runs take the vertical kana (`vkna`). Tests in `crates/support/fonts/src/tests_vertical.rs` and the `upright_*` / `vertical_*` compose tests |
| Kinsoku without a set | CJK text breaks between characters, with no-start and no-end characters respected (`cjk_text_breaks_between_characters_with_kinsoku`) |
| Korean line breaking | At spaces, for Hangul and for hanja in Korean text, also under a kinsoku set and when the language is given as a locale code; between characters when the paragraph asks (`text.para {koreanCharBreaks}`). The option is not written to IDML. Tests `korean_*` and `hanja_in_korean_text_breaks_at_spaces` |
| Tate-chu-yoko | Set by command (`text.tateChuYoko`); IDML `Tatechuyoko` |
| Ruby | Group ruby (`text.ruby`; IDML `RubyString`) at half the base size: shorter ruby is spread with half a gap at each end, longer ruby is centred over its base (`crates/engine/compose/src/ruby.rs`) |
| Kenten | Emphasis marks centred over each character (`text.kenten`; IDML `KentenKind`) |
| Warichu | `text.warichu` (`lines`, `size`, `align`, `lineSpacing`, `charsBeforeBreak`, `charsAfterBreak`) stacks the run in smaller lines inside the line after the line is broken, and the rest of the line closes up; the minimum characters before and after a break apply to the first and last small line, the small lines are balanced by width, and negative line spacing tightens them; hit testing, the caret and the selection follow each small line (`warichu_*` compose tests). IDML reads and writes `Warichu`, `WarichuLines`, `WarichuSize`, `WarichuLineSpacing`, `WarichuAlignment`, `WarichuCharsBeforeBreak` and `WarichuCharsAfterBreak` |
| Languages | Japanese, Korean, Simplified and Traditional Chinese reach the shaper's localized forms (`locl`), typographer's quotes, line breaking and fallback fonts. IDML language names, locale codes (`ja_JP`, `ko-KR`) and the common spellings of Chinese ("Simplified Chinese", "Chinese (Traditional)", `zh_CN`, `zh-Hant`) map to a language tag (`language_tag` in `crates/engine/doc/src/attrs.rs`; tests `chinese_japanese_and_korean_names_map_in_their_common_spellings`, `chinese_and_japanese_take_their_localized_forms`, `cjk_languages_have_their_quotes_and_no_english_rules`) |
| Fallback by language | Fallback for CJK characters follows the text language (`cjk_fallback_follows_the_language`): each of Japanese, Simplified Chinese, Traditional Chinese and Korean has its own chain of installed fonts, serif faces first; Japanese tries the faces of the optional kition-fonts build input first. Only native builds embed kition-fonts; the WebAssembly build embeds none, and the editor in `web/` supplies fonts to the engine at run time |
| Missing glyphs | A character the applied font lacks is drawn as that font's missing-glyph box unless the document's `glyphFallback` setting draws it from fallback fonts; the same holds for kenten (`missing_glyphs_are_the_fonts_box_unless_fallback_is_on`, `kenten_missing_from_the_font_are_its_box_unless_fallback_is_on`) |
| Document fonts | Fonts in a `Document Fonts` folder beside an opened file load ahead of installed fonts of the same name (`document_fonts_load_ahead_of_installed_ones`); Package copies the fonts that draw text, fallback fonts included (`package_copies_the_fallback_fonts_that_draw_text`), and reads the OS/2 embedding bits to find restricted-license fonts (`restricted_licence_reads_the_embedding_bits`). Engine sessions outside a host only: a hosted session (the editor, the headless kit serving Kition) reads no `Document Fonts` folder and refuses Package |

## Unfinished behavior

- **Mojikumi**: definitions, base-set names, all override rows and paragraph references
  survive import/export. The numeric character-class spacing tables and their
  compression priorities are **not executed**. Preflight reports this explicitly.
- **Kinsoku push-in/push-out priorities**: retained, but not used in candidate
  selection. Preflight reports this explicitly. Boundary prohibition alone is not
  equivalent to implementing this priority policy.
- Named kinsoku defaults are clean-room common punctuation rules, not verified
  reproductions of every vendor preset. Explicit custom tables take precedence.
- ICF alignment currently uses font ascender/descender metrics; exact ideographic
  character-face metrics are not implemented. CenterDown currently shares the
  center leading reference. These require separate compatibility work.
- Jidori groups are scoped to resolved shaping runs. Mixed component-font or style
  changes inside one intended group need a group model above shaping.
- Frame-grid composition is not implemented.
- Ruby is group ruby only, at a fixed half size, with no alignment, position or overhang
  options; ruby and kenten don't change line breaks or leading. Kenten has no position,
  size, colour or font options.
- Tate-chu-yoko is manual only; there is no automatic tate-chu-yoko for runs of digits.
- A justified CJK line without spaces gets no stretch except from Single Word
  Justification.
- Vertical metrics of variable fonts are read at the default location only (no `VVAR`).

Preserving a parameter is not evidence that its visual rule has been implemented.
Missing fonts, missing links and overset remain separate preflight findings;
removing unsupported-rule warnings to produce a clean report is not acceptable.

## References

- [Adobe MojikumiTable API](https://developer.adobe.com/indesign/uxp/dom/api/m/mojikumi-table/)
  describes the serialized override tuple, but does not define the numeric class mapping.
- [Adobe spacing priorities](https://helpx.adobe.com/hk_en/indesign/desktop/language-and-proofing/chinese-japanese-and-korean/set-spacing-priorities-in-mojikumi-character-classes.html)
  describes ordered compression, which requires more than loading the table.

Tests use synthetic documents; user IDML files and proprietary application output
are not committed as fixtures.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
