# Localization parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe InDesign 2026 (21.6)

Per-language status of the Kition Layout interface and of document typesetting in each script.
Script-specific typesetting detail: [cjk-typography.md](../features/cjk-typography.md),
[arabic-typography.md](../features/arabic-typography.md). Overall numbers: [target-app-parity.md](target-app-parity.md).

## Interface

The kit's own editor (`web/`) has two string catalogs, `en-US` and `zh-CN`
(`web/src/strings.ts`); `PHRASES` there translates the engine's command titles into Simplified
Chinese. kit-editor's shared strings come in the same two locales; other locales fall back to
English.

InDesign 2026 ships 25 interface localizations (`.lproj` folders: cs, da, de, el, en, en_GB, es,
fi, fr, hu, it, ja, ko, nb, nl, pl, pt, ro, ru, sq, sv, tr, uk, zh_CN, zh_TW); Arabic and Hebrew
come as Middle East editions with an English or French interface and right-to-left features.

## Key languages

| Language | Code | InDesign 2026 UI | Editor UI (`web/`) | Script support (documents) |
|---|---|---|---|---|
| English | en | ✓ | ✓ (`en-US`) | Latin ✓ |
| Simplified Chinese (Mandarin) | zh | ✓ (zh_CN) | ✓ (`zh-CN`) | CJK typesetting ✓ ([cjk-typography.md](../features/cjk-typography.md)); **no canvas IME** |
| Spanish | es | ✓ | ✗ | Latin ✓; Spanish hyphenation ✓ |
| Hindi | hi | ✗ | ✗ | Devanagari shaped by harfrust in documents but untested; no Hindi hyphenation or spelling |
| Arabic | ar | Middle East edition (English/French UI) | ✗ | Arabic composition, kashida, digits, RTL stories and tables ✓ ([arabic-typography.md](../features/arabic-typography.md)); list bullets and multi-column RTL open |
| French | fr | ✓ | ✗ | Latin ✓; no French hyphenation |
| Portuguese (Brazil) | pt-br | ✓ (pt) | ✗ | Latin ✓; no Portuguese hyphenation |
| Indonesian | id | ✗ | ✗ | Latin ✓ |
| Japanese | ja | ✓ | ✗ | vertical text, kinsoku, aki, ruby, warichu ✓ (partial depth, [cjk-typography.md](../features/cjk-typography.md)); **no canvas IME**; Mincho fonts from kition-fonts |
| German | de | ✓ | ✗ | Latin ✓; no German hyphenation (InDesign ships 2006 reform patterns) |
| Korean | ko | ✓ | ✗ | Korean line breaking at spaces ✓; **no IME** |
| Vietnamese | vi | ✗ | ✗ | Latin with stacked diacritics via harfrust, untested |

A language is `full` only when every user-visible string is translated and its script renders
and edits correctly.

InDesign ships 17 interface languages beyond its eight of the twelve (cs, da, el, en_GB, fi, hu,
it, nb, nl, pl, ro, ru, sq, sv, tr, uk, zh_TW); the editor ships none of them.

## Document languages

InDesign 2026 hyphenates and spell-checks ~40 languages (Hunspell and Duden dictionaries).
Kition Layout hyphenates **English and Spanish** and spell-checks **English** only. Text language
can be set per paragraph and character (`type.char`, `type.para`) and drives quotes and
locale shaping. See [typography-parity.md](typography-parity.md).

## Remaining effort

| Work | Opus 5.5 h |
|---|---|
| Canvas IME (zh, ja, ko) | 10–20 |
| Devanagari and Vietnamese rendering tests | 10–15 |
| Help content (at least English, then the twelve) | 15–25 |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
