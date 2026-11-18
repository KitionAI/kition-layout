//! Text attributes as rich-text marks (design note section 2.4).
//!
//! - Every one of the 61 character attributes (`CharAttrs`) is one mark under its camelCase name,
//!   plus `cStyle`, the character style's object id; all `expand: "after"`, so concurrent
//!   formatting of overlapping ranges with different attributes all applies.
//! - The 77 paragraph attributes (`ParaAttrs`) are grouped by coupling into 13 JSON marks
//!   ([`PARA_GROUPS`]), plus `pStyle`, the paragraph style's object id. They sit on the
//!   paragraph's terminating `\n` and are `expand: "none"`: typing never extends them.
//! - `table` (on a paragraph's `\n`: the table anchored there) and `inline` (on an inline
//!   object's placeholder character) are `expand: "none"` too.

use std::collections::BTreeMap;

use kition_layout_doc::{CharAttrs, ParaAttrs};
use serde_json::{Map, Value};

use crate::{KdmResult, kdm_err};

/// The character style mark.
pub const C_STYLE: &str = "cStyle";
/// The paragraph style mark.
pub const P_STYLE: &str = "pStyle";
/// The anchored-table mark (on a paragraph's `\n`).
pub const TABLE: &str = "table";
/// The inline-object mark (on its placeholder character).
pub const INLINE: &str = "inline";

/// The 61 character attribute marks, as `CharAttrs` serializes its fields.
pub const CHAR_KEYS: &[&str] = &[
    "fontFamily",
    "fontStyle",
    "size",
    "leading",
    "kerning",
    "tracking",
    "leadingAki",
    "trailingAki",
    "tsume",
    "jidori",
    "characterAlignment",
    "leadingModel",
    "kentenCharacter",
    "hScale",
    "vScale",
    "baselineShift",
    "skew",
    "fill",
    "fillTint",
    "stroke",
    "strokeTint",
    "strokeWeight",
    "capitalization",
    "position",
    "underline",
    "strikethrough",
    "underlineWeight",
    "underlineOffset",
    "underlineColor",
    "underlineTint",
    "strikethroughWeight",
    "strikethroughOffset",
    "strikethroughColor",
    "strikethroughTint",
    "glyphForm",
    "ligatures",
    "noBreak",
    "tateChuYoko",
    "tateChuYokoXOffset",
    "tateChuYokoYOffset",
    "ruby",
    "kenten",
    "warichu",
    "warichuLines",
    "warichuSize",
    "warichuLineSpacing",
    "warichuAlignment",
    "warichuCharsBeforeBreak",
    "warichuCharsAfterBreak",
    "digits",
    "characterDirection",
    "allowKashidas",
    "diacriticPosition",
    "diacriticXOffset",
    "diacriticYOffset",
    "positionalForm",
    "language",
    "otfFeatures",
    "conditions",
    "change",
    "xmlTag",
];

/// The 13 paragraph marks and the `ParaAttrs` fields (camelCase) each one holds: values that only
/// make sense together share one mark (word spacing minimum, desired and maximum; all hyphenation
/// settings), so a merge never mixes two writers' halves.
pub const PARA_GROUPS: &[(&str, &[&str])] = &[
    ("pAlign", &["align", "direction", "singleWordJustify"]),
    ("pIndents", &["leftIndent", "rightIndent", "firstLineIndent", "lastLineIndent"]),
    ("pSpacing", &["spaceBefore", "spaceAfter", "autoLeading", "gridAlign"]),
    ("pDropCap", &["dropCapLines", "dropCapChars"]),
    ("pNested", &["nestedStyles", "grepStyles", "nestedLineStyles"]),
    (
        "pHyphenation",
        &[
            "hyphenate",
            "hyphMinWord",
            "hyphAfterFirst",
            "hyphBeforeLast",
            "hyphLimit",
            "hyphZone",
            "hyphCapitalized",
            "hyphLastWord",
            "hyphAcrossColumn",
            "hyphWeight",
        ],
    ),
    (
        "pJustification",
        &[
            "wordSpaceMin",
            "wordSpaceDesired",
            "wordSpaceMax",
            "letterSpaceMin",
            "letterSpaceDesired",
            "letterSpaceMax",
            "glyphScaleMin",
            "glyphScaleDesired",
            "glyphScaleMax",
            "composer",
            "kashidas",
            "arabicJustification",
            "paragraphKashidaWidth",
        ],
    ),
    (
        "pKeeps",
        &[
            "keepWithNext",
            "keepLinesTogether",
            "keepAllLines",
            "keepFirst",
            "keepLast",
            "startParagraph",
            "spanColumns",
            "splitInsideGutter",
            "splitOutsideGutter",
        ],
    ),
    ("pRules", &["ruleAbove", "ruleBelow"]),
    ("pTabs", &["tabs"]),
    ("pList", &["listType", "listName", "startAt", "bulletChar", "numberStyle", "numberExpression", "listSeparator"]),
    ("pCjk", &["kinsoku", "mojikumi", "kinsokuType", "kinsokuHang", "bunriKinshi", "rensuuji", "treatIdeographicSpaceAsSpace", "koreanCharBreaks"]),
    (
        "pDecor",
        &[
            "shadingOn",
            "shadingColor",
            "shadingTint",
            "shadingOffsets",
            "borderOn",
            "borderWeights",
            "borderColor",
            "borderTint",
            "borderOffsets",
            "balanceRagged",
            "opticalMargin",
        ],
    ),
];

/// The paragraph mark holding `ParaAttrs` field `key` (camelCase).
pub fn para_group(key: &str) -> Option<&'static str> {
    PARA_GROUPS.iter().find(|(_, keys)| keys.contains(&key)).map(|(group, _)| *group)
}

/// Whether `key` is one of the paragraph marks (the 13 groups).
pub fn is_para_group(key: &str) -> bool {
    PARA_GROUPS.iter().any(|(group, _)| *group == key)
}

/// Whether `key` is a character attribute mark.
pub fn is_char_key(key: &str) -> bool {
    CHAR_KEYS.contains(&key)
}

fn object_of(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// The set attributes of `attrs`, by mark key (camelCase), as plain JSON.
pub fn char_marks(attrs: &CharAttrs) -> KdmResult<Map<String, Value>> {
    // An attribute set to "none" (`Some(None)`, such as an automatic underline weight) serializes
    // as `null`, which a mark cannot hold; the engine's own file format reads it back as not set,
    // and so does the bridge.
    Ok(object_of(serde_json::to_value(attrs).map_err(kdm_err)?).into_iter().filter(|(_, v)| !v.is_null()).collect())
}

/// The attributes of the character marks among `marks` (other keys are ignored).
pub fn char_attrs(marks: &BTreeMap<String, Value>) -> KdmResult<CharAttrs> {
    let map: Map<String, Value> = marks.iter().filter(|(k, v)| is_char_key(k) && !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
    if map.is_empty() {
        return Ok(CharAttrs::default());
    }
    serde_json::from_value(Value::Object(map)).map_err(kdm_err)
}

/// The set attributes of `attrs` grouped into paragraph marks: each group present holds the
/// object of its set fields.
pub fn para_marks(attrs: &ParaAttrs) -> KdmResult<Map<String, Value>> {
    let mut out = Map::new();
    for (key, value) in object_of(serde_json::to_value(attrs).map_err(kdm_err)?) {
        let group = para_group(&key).ok_or_else(|| kdm_err(format!("paragraph attribute `{key}` has no mark")))?;
        if let Value::Object(fields) = out.entry(group).or_insert_with(|| Value::Object(Map::new())) {
            fields.insert(key, value);
        }
    }
    Ok(out)
}

/// The attributes of the paragraph marks among `marks`.
pub fn para_attrs<'a>(marks: impl IntoIterator<Item = (&'a String, &'a Value)>) -> KdmResult<ParaAttrs> {
    let mut map = Map::new();
    for (key, value) in marks {
        if is_para_group(key)
            && let Value::Object(fields) = value
        {
            for (k, v) in fields {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    if map.is_empty() {
        return Ok(ParaAttrs::default());
    }
    serde_json::from_value(Value::Object(map)).map_err(kdm_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camel(snake: &str) -> String {
        let mut out = String::new();
        let mut upper = false;
        for c in snake.chars() {
            if c == '_' {
                upper = true;
            } else if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn every_character_attribute_is_one_mark() {
        let keys: Vec<String> = CharAttrs::all_keys().iter().map(|k| camel(k)).collect();
        assert_eq!(keys.len(), 61);
        assert_eq!(keys, CHAR_KEYS.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>());
    }

    #[test]
    fn every_paragraph_attribute_is_in_exactly_one_group() {
        let keys: Vec<String> = ParaAttrs::all_keys().iter().map(|k| camel(k)).collect();
        assert_eq!(keys.len(), 77);
        for key in &keys {
            let holders = PARA_GROUPS.iter().filter(|(_, fields)| fields.contains(&key.as_str())).count();
            assert_eq!(holders, 1, "{key}");
        }
        let grouped: usize = PARA_GROUPS.iter().map(|(_, fields)| fields.len()).sum();
        assert_eq!(grouped, 77, "no group names a field ParaAttrs lacks");
        assert_eq!(PARA_GROUPS.len(), 13);
    }

    #[test]
    fn the_schema_declares_every_mark_once_per_text_field() {
        let schema: Value = serde_json::from_str(crate::SCHEMA).unwrap();
        for (ty, field) in [("Story", "text"), ("Inline", "text"), ("Cell", "text")] {
            let marks = schema["types"][ty]["fields"][field]["marks"].as_object().cloned().unwrap();
            for key in CHAR_KEYS.iter().chain([&C_STYLE]) {
                assert_eq!(marks.get(*key).and_then(|m| m["expand"].as_str()), Some("after"), "{ty}.{field} {key}");
            }
            for key in PARA_GROUPS.iter().map(|(g, _)| g).chain([&P_STYLE, &TABLE, &INLINE]) {
                assert_eq!(marks.get(*key).and_then(|m| m["expand"].as_str()), Some("none"), "{ty}.{field} {key}");
            }
            assert_eq!(marks.len(), 61 + 1 + 13 + 1 + 2, "{ty}.{field}");
        }
    }

    #[test]
    fn attributes_round_trip_through_marks() {
        let mut c = CharAttrs::default();
        c.set_json("fontFamily", &serde_json::json!("Inter")).unwrap();
        c.set_json("size", &serde_json::json!(14.5)).unwrap();
        c.set_json("otfFeatures", &serde_json::json!(["ss01"])).unwrap();
        c.underline_weight = Some(None);
        let marks: BTreeMap<String, Value> = char_marks(&c).unwrap().into_iter().collect();
        assert_eq!(marks.len(), 3, "no null mark value");
        c.underline_weight = None;
        assert_eq!(char_attrs(&marks).ok(), Some(c));

        let mut p = ParaAttrs::default();
        p.set_json("wordSpaceMin", &serde_json::json!(0.7)).unwrap();
        p.set_json("wordSpaceMax", &serde_json::json!(1.5)).unwrap();
        p.set_json("leftIndent", &serde_json::json!(12)).unwrap();
        let marks = para_marks(&p).unwrap();
        assert_eq!(marks.keys().cloned().collect::<Vec<_>>(), vec!["pIndents".to_owned(), "pJustification".to_owned()]);
        assert_eq!(para_attrs(&marks).ok(), Some(p));
    }
}
