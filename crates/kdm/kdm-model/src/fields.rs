//! The registers of each object type, written from the engine's values and read back (design
//! note section 2.2). Each `*_fields` function has its reading partner next to it, so the two stay
//! symmetric: whatever a field leaves out goes into the type's `props`/`options`/`numbering` JSON,
//! so a round trip loses nothing but what the schema deliberately does not store (an asset's disk
//! path, a page's view rotation, a story's revision counter, the id counter).

use std::collections::HashMap;
use std::sync::Arc;

use kition_kit::kdm::ObjectId;
use kition_kit::session::{BlobRef, BlobSink, BlobSource, TextRun};
use kition_layout_doc::{
    Asset, AssetId, Cell, CharacterStyle, Content, Guide, Hyperlink, HyperlinkDest, HyperlinkSource, Item, ItemId, Layer, ObjectStyle, Page,
    ParagraphStyle, Placeholder, PlaceholderAnchor, PlaceholderRole, Section, Story, StoryId,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::ids::{CELL, FAMILIES, Ids, OBJECT, PARAGRAPH, TABLE};
use crate::marks::{self, CHAR_KEYS, HYPERLINK, PLACEHOLDER};
use crate::refs::{swatches_in, swatches_out};
use crate::text::{NamesIn, NamesOut, TextRange};
use crate::{KdmResult, kdm_err};

/// The register fields of one object, as written (references already object ids). `null` means
/// "not set".
pub(crate) type Fields = Vec<(&'static str, Value)>;

pub(crate) fn to_map(v: &impl Serialize) -> KdmResult<Map<String, Value>> {
    match serde_json::to_value(v).map_err(kdm_err)? {
        Value::Object(map) => Ok(map),
        _ => Ok(Map::new()),
    }
}

pub(crate) fn from_map<T: DeserializeOwned>(map: Map<String, Value>) -> KdmResult<T> {
    serde_json::from_value(Value::Object(map)).map_err(kdm_err)
}

fn take(map: &mut Map<String, Value>, key: &str) -> Value {
    map.remove(key).unwrap_or(Value::Null)
}

fn oid(object: &ObjectId) -> Value {
    Value::String(object.as_str().to_owned())
}

fn json_or_null(map: Map<String, Value>) -> Value {
    if map.is_empty() { Value::Null } else { Value::Object(map) }
}

/// A `ref` value's object, if it is one.
pub(crate) fn as_object(v: &Value) -> Option<ObjectId> {
    v.as_str().map(ObjectId::from_string)
}

/// What writing needs: the objects behind local ids and names.
pub(crate) struct Out<'a> {
    pub ids: &'a Ids,
    pub names: NamesOut,
    /// Text anchor id (the engine's) to its `Inline` object.
    pub anchors: HashMap<u64, ObjectId>,
    /// Page objects by absolute page index (spreads only).
    pub page_at: Vec<ObjectId>,
    /// Per top-level story, the ranges of its text that are hyperlink sources and data merge
    /// placeholders, as the marks that name their objects.
    pub ranges: HashMap<StoryId, Vec<TextRange>>,
}

impl Out<'_> {
    fn item(&self, v: &Value) -> Option<Value> {
        v.as_u64().and_then(|n| self.ids.items.object(ItemId(n))).map(oid)
    }

    fn asset(&self, v: &Value) -> Option<Value> {
        v.as_u64().and_then(|n| self.ids.assets.object(n)).map(oid)
    }

    fn style_ref(&self, family: usize, name: &str) -> Option<Value> {
        self.names.styles[family].get(name).map(oid)
    }

    fn swatches(&self, v: &mut Value) {
        swatches_out(v, &self.names.swatches);
    }
}

/// What reading needs: the local ids and names behind objects.
#[derive(Default)]
pub(crate) struct In {
    pub names: NamesIn,
    pub items: HashMap<ObjectId, u64>,
    pub stories: HashMap<ObjectId, u64>,
    pub spreads: HashMap<ObjectId, u64>,
    pub pages: HashMap<ObjectId, u64>,
    pub layers: HashMap<ObjectId, u64>,
    pub assets: HashMap<ObjectId, u64>,
    /// `Inline` object of a text anchor to the anchor's local id.
    pub anchors: HashMap<ObjectId, u64>,
    /// Page objects to their absolute index (spreads only).
    pub page_index: HashMap<ObjectId, usize>,
    /// The layer an item on a deleted layer reads as (rule V7): the first layer.
    pub first_layer: u64,
    /// Each text frame's story, by the stories' `frames` lists.
    pub frame_story: HashMap<u64, u64>,
}

impl In {
    fn local(map: &HashMap<ObjectId, u64>, v: &Value) -> Option<Value> {
        as_object(v).and_then(|o| map.get(&o)).map(|n| Value::from(*n))
    }

    pub(crate) fn item(&self, v: &Value) -> Option<Value> {
        Self::local(&self.items, v)
    }

    pub(crate) fn story(&self, v: &Value) -> Option<Value> {
        Self::local(&self.stories, v)
    }

    fn asset(&self, v: &Value) -> Option<Value> {
        Self::local(&self.assets, v)
    }

    fn swatches(&self, v: &mut Value) {
        swatches_in(v, &self.names.swatches);
    }
}

// ---------------------------------------------------------------------------------------------
// Items

/// The keys of a serialized item that are registers of their own.
const ITEM_REGISTERS: &[&str] =
    &["xf", "path", "fill", "stroke", "corners", "opacity", "blend", "effects", "wrap", "locked", "hidden", "nonprinting", "altText", "name"];

/// An item's registers. `spread` is the spread object of a top-level item on a spread (`None` for
/// group children and anchored objects).
pub(crate) fn item_fields(item: &Item, spread: Option<&ObjectId>, out: &Out<'_>) -> KdmResult<Fields> {
    let mut shallow = item.clone();
    if let Content::Group { items } = &mut shallow.content {
        items.clear();
    }
    let mut map = to_map(&shallow)?;
    map.remove("id");
    let shape = take(&mut map, "shape");
    let layer = out.ids.layers.object(item.layer.0).map(oid).unwrap_or(Value::Null);
    map.remove("layer");
    let mut content = match take(&mut map, "content") {
        Value::Object(c) => c,
        _ => Map::new(),
    };
    let kind = content.remove("type").unwrap_or_else(|| json!("unassigned"));
    let mut text_options = Value::Null;
    let mut graphic = Value::Null;
    match kind.as_str() {
        Some("text") => text_options = content.remove("options").unwrap_or(Value::Null),
        Some("graphic") => {
            if let Some(asset) = content.get("asset") {
                let mapped = out.asset(asset).unwrap_or(Value::Null);
                content.insert("asset".into(), mapped);
            }
            graphic = Value::Object(content);
        }
        _ => {}
    }
    let mut registers: Vec<Value> = ITEM_REGISTERS.iter().map(|k| take(&mut map, k)).collect();
    let mut props = map;
    let object_style = match props.remove("objectStyle") {
        Some(Value::String(name)) if !name.is_empty() => match out.style_ref(OBJECT, &name) {
            Some(v) => v,
            None => {
                props.insert("objectStyle".into(), Value::String(name));
                Value::Null
            }
        },
        _ => Value::Null,
    };
    let overrides = props.remove("overrides").and_then(|v| out.item(&v)).unwrap_or(Value::Null);
    if let Some(Value::Object(caption)) = props.get_mut("liveCaption")
        && let Some(source) = caption.get("source")
    {
        let mapped = out.item(source).unwrap_or(Value::Null);
        caption.insert("source".into(), mapped);
    }
    if let Some(Value::Object(media)) = props.get_mut("media")
        && let Some(poster) = media.get("poster")
    {
        let mapped = out.asset(poster).unwrap_or(Value::Null);
        media.insert("poster".into(), mapped);
    }
    let mut props = json_or_null(props);
    for v in registers.iter_mut().chain([&mut text_options, &mut graphic, &mut props]) {
        out.swatches(v);
    }
    let [xf, path, fill, stroke, corners, opacity, blend, effects, wrap, locked, hidden, nonprinting, alt_text, name] =
        <[Value; 14]>::try_from(registers).map_err(|_| kdm_err("item registers"))?;
    Ok(vec![
        ("shape", shape),
        ("content", kind),
        ("spread", spread.map(oid).unwrap_or(Value::Null)),
        ("layer", layer),
        ("name", name),
        ("xf", xf),
        ("path", path),
        ("fill", fill),
        ("stroke", stroke),
        ("corners", corners),
        ("opacity", opacity),
        ("blend", blend),
        ("effects", effects),
        ("wrap", wrap),
        ("locked", locked),
        ("hidden", hidden),
        ("nonprinting", nonprinting),
        ("objectStyle", object_style),
        ("textOptions", text_options),
        ("graphic", graphic),
        ("overrides", overrides),
        ("altText", alt_text),
        ("props", props),
    ])
}

/// An item (without its group children) from its registers.
pub(crate) fn item_from(id: u64, get: &dyn Fn(&str) -> Value, inn: &In) -> KdmResult<Item> {
    let mut map = match get("props") {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if let Some(Value::Object(caption)) = map.get_mut("liveCaption")
        && let Some(source) = caption.get("source")
    {
        let mapped = inn.item(source).unwrap_or(Value::from(0));
        caption.insert("source".into(), mapped);
    }
    if let Some(Value::Object(media)) = map.get_mut("media")
        && let Some(poster) = media.get("poster")
    {
        match inn.asset(poster) {
            Some(v) => {
                media.insert("poster".into(), v);
            }
            None => {
                media.remove("poster");
            }
        }
    }
    map.insert("id".into(), Value::from(id));
    for key in ITEM_REGISTERS {
        let v = get(key);
        if !v.is_null() {
            map.insert((*key).to_owned(), v);
        }
    }
    map.insert("shape".into(), get("shape"));
    let layer = as_object(&get("layer")).and_then(|o| inn.layers.get(&o).copied()).unwrap_or(inn.first_layer);
    map.insert("layer".into(), Value::from(layer));
    if let Some(name) = as_object(&get("objectStyle")).and_then(|o| inn.names.styles[OBJECT].get(&o).cloned()) {
        map.insert("objectStyle".into(), Value::String(name));
    }
    if let Some(v) = inn.item(&get("overrides")) {
        map.insert("overrides".into(), v);
    }
    let kind = get("content");
    let content = match kind.as_str() {
        Some("text") => {
            let story = inn.frame_story.get(&id).copied().unwrap_or(0);
            let mut c = Map::new();
            c.insert("type".into(), json!("text"));
            c.insert("story".into(), Value::from(story));
            let options = get("textOptions");
            if !options.is_null() {
                c.insert("options".into(), options);
            }
            Value::Object(c)
        }
        Some("graphic") => {
            let mut c = match get("graphic") {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            let asset = c.get("asset").and_then(|a| inn.asset(a)).unwrap_or(Value::from(0));
            c.insert("asset".into(), asset);
            c.insert("type".into(), json!("graphic"));
            Value::Object(c)
        }
        Some("group") => json!({ "type": "group", "items": [] }),
        _ => json!({ "type": "unassigned" }),
    };
    map.insert("content".into(), content);
    let mut v = Value::Object(map);
    inn.swatches(&mut v);
    serde_json::from_value(v).map_err(kdm_err)
}

// ---------------------------------------------------------------------------------------------
// Layers, pages, guides, sections

pub(crate) fn layer_fields(layer: &Layer) -> KdmResult<Fields> {
    let mut map = to_map(layer)?;
    map.remove("id");
    let name = take(&mut map, "name");
    let color = take(&mut map, "color");
    let visible = take(&mut map, "visible");
    let locked = take(&mut map, "locked");
    let printable = take(&mut map, "printable");
    Ok(vec![("name", name), ("color", color), ("visible", visible), ("locked", locked), ("printable", printable), ("props", json_or_null(map))])
}

pub(crate) fn layer_from(id: u64, get: &dyn Fn(&str) -> Value) -> KdmResult<Layer> {
    let mut map = match get("props") {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    map.insert("id".into(), Value::from(id));
    for key in ["name", "color", "visible", "locked", "printable"] {
        let v = get(key);
        if !v.is_null() {
            map.insert(key.into(), v);
        }
    }
    map.entry("name").or_insert(json!(""));
    map.entry("color").or_insert(json!([0, 0, 0]));
    from_map(map)
}

pub(crate) fn page_fields(page: &Page, out: &Out<'_>) -> KdmResult<Fields> {
    let mut map = to_map(page)?;
    for key in ["id", "guides", "viewRotation"] {
        map.remove(key);
    }
    let size = json!({ "width": take(&mut map, "width"), "height": take(&mut map, "height") });
    let margins = take(&mut map, "margins");
    let columns = take(&mut map, "columns");
    map.remove("parent");
    let parent = page.parent.and_then(|p| out.ids.spreads.object(p.0)).map(oid).unwrap_or(Value::Null);
    let show = take(&mut map, "showParentItems");
    let x = take(&mut map, "x");
    let side = take(&mut map, "side");
    // `overridden` is the page's own list of item references (written by the caller as a set).
    map.remove("overridden");
    Ok(vec![
        ("size", size),
        ("margins", margins),
        ("columns", columns),
        ("parent", parent),
        ("showParentItems", show),
        ("x", x),
        ("side", side),
        ("props", json_or_null(map)),
    ])
}

/// The objects of the parent items `page` overrides, in its order (unknown ids left out).
pub(crate) fn page_overridden(page: &Page, out: &Out<'_>) -> Vec<ObjectId> {
    page.overridden.iter().filter_map(|id| out.ids.items.object(*id).cloned()).collect()
}

/// A page read from its registers; `overridden` holds the objects its own list names.
pub(crate) fn page_from(id: u64, get: &dyn Fn(&str) -> Value, overridden: &[ObjectId], guides: Vec<Guide>, inn: &In) -> KdmResult<Page> {
    let mut map = match get("props") {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let mut seen = std::collections::HashSet::new();
    let overridden: Vec<Value> = overridden.iter().filter_map(|o| inn.items.get(o).copied()).filter(|i| seen.insert(*i)).map(Value::from).collect();
    if !overridden.is_empty() {
        map.insert("overridden".into(), Value::Array(overridden));
    }
    map.insert("id".into(), Value::from(id));
    let size = get("size");
    map.insert("width".into(), size.get("width").cloned().unwrap_or(json!(612)));
    map.insert("height".into(), size.get("height").cloned().unwrap_or(json!(792)));
    for key in ["margins", "columns", "showParentItems", "x", "side"] {
        let v = get(key);
        if !v.is_null() {
            map.insert(key.into(), v);
        }
    }
    if let Some(parent) = as_object(&get("parent")).and_then(|o| inn.spreads.get(&o)) {
        map.insert("parent".into(), Value::from(*parent));
    }
    map.entry("x").or_insert(json!(0));
    map.entry("margins").or_insert(json!({ "top": 36, "bottom": 36, "inside": 36, "outside": 36 }));
    map.entry("columns").or_insert(json!({ "count": 1, "gutter": 12 }));
    map.insert("guides".into(), serde_json::to_value(guides).map_err(kdm_err)?);
    from_map(map)
}

pub(crate) fn guide_fields(guide: &Guide, page: &ObjectId, out: &Out<'_>) -> KdmResult<Fields> {
    let mut map = to_map(guide)?;
    map.remove("layer");
    map.insert("page".into(), oid(page));
    let layer = guide.layer.and_then(|l| out.ids.layers.object(l.0)).map(oid).unwrap_or(Value::Null);
    Ok(vec![("line", Value::Object(map)), ("layer", layer)])
}

/// A guide and the page object it belongs to.
pub(crate) fn guide_from(get: &dyn Fn(&str) -> Value, inn: &In) -> KdmResult<(Option<ObjectId>, Guide)> {
    let mut map = match get("line") {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let page = map.remove("page").as_ref().and_then(as_object);
    if let Some(layer) = as_object(&get("layer")).and_then(|o| inn.layers.get(&o)) {
        map.insert("layer".into(), Value::from(*layer));
    }
    Ok((page, from_map(map)?))
}

pub(crate) fn section_fields(section: &Section, out: &Out<'_>) -> KdmResult<Option<Fields>> {
    let Some(start) = out.page_at.get(section.start) else { return Ok(None) };
    let mut map = to_map(section)?;
    map.remove("start");
    Ok(Some(vec![("start", oid(start)), ("numbering", json_or_null(map))]))
}

pub(crate) fn section_from(get: &dyn Fn(&str) -> Value, inn: &In) -> KdmResult<Option<Section>> {
    let Some(start) = as_object(&get("start")).and_then(|o| inn.page_index.get(&o).copied()) else { return Ok(None) };
    let mut map = match get("numbering") {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    map.insert("start".into(), Value::from(start));
    Ok(Some(from_map(map)?))
}

// ---------------------------------------------------------------------------------------------
// Hyperlinks, data merge placeholders, named definitions

/// The ranges of each top-level story's text that `doc`'s hyperlinks and data merge placeholders
/// start from, as the marks naming their objects (`ids`).
pub(crate) fn text_ranges(doc: &kition_layout_doc::Document, ids: &Ids) -> HashMap<StoryId, Vec<TextRange>> {
    let mut out: HashMap<StoryId, Vec<TextRange>> = HashMap::new();
    for link in &doc.hyperlinks {
        if let HyperlinkSource::Text { story, start, end } = link.source
            && let Some(object) = ids.hyperlinks.object(link.id)
        {
            out.entry(story).or_default().push(TextRange { mark: HYPERLINK, object: object.clone(), start, end });
        }
    }
    for p in &doc.data_merge.placeholders {
        if let PlaceholderAnchor::Text { story, start, end } = p.anchor
            && let Some(object) = ids.placeholders.object(p.id)
        {
            out.entry(story).or_default().push(TextRange { mark: PLACEHOLDER, object: object.clone(), start, end });
        }
    }
    out
}

/// A hyperlink's registers: its name, the item it starts from (a text source is the `hyperlink`
/// mark on its story's text), and where it goes.
pub(crate) fn hyperlink_fields(link: &Hyperlink, out: &Out<'_>) -> Fields {
    let item = match link.source {
        HyperlinkSource::Item { id } => out.ids.items.object(id).map(oid).unwrap_or(Value::Null),
        HyperlinkSource::Text { .. } => Value::Null,
    };
    let (url, email, page) = match &link.dest {
        HyperlinkDest::Url(url) => (json!(url), Value::Null, Value::Null),
        HyperlinkDest::Email(email) => (Value::Null, json!(email), Value::Null),
        HyperlinkDest::Page(page) => (Value::Null, Value::Null, out.page_at.get(*page).map(oid).unwrap_or(Value::Null)),
    };
    vec![("name", json!(link.name)), ("item", item), ("url", url), ("email", email), ("page", page)]
}

/// A hyperlink read back; `text` is its source text by its `hyperlink` mark. `None` when what it
/// starts from or goes to is gone.
pub(crate) fn hyperlink_from(id: u64, get: &dyn Fn(&str) -> Value, text: Option<&(StoryId, usize, usize)>, inn: &In) -> Option<Hyperlink> {
    let source = match as_object(&get("item")) {
        Some(item) => HyperlinkSource::Item { id: ItemId(*inn.items.get(&item)?) },
        None => {
            let (story, start, end) = *text?;
            HyperlinkSource::Text { story, start, end }
        }
    };
    let dest = if let Some(url) = get("url").as_str() {
        HyperlinkDest::Url(url.to_owned())
    } else if let Some(email) = get("email").as_str() {
        HyperlinkDest::Email(email.to_owned())
    } else {
        HyperlinkDest::Page(*inn.page_index.get(&as_object(&get("page"))?)?)
    };
    let name = get("name").as_str().unwrap_or_default().to_owned();
    Some(Hyperlink { id, name, source, dest })
}

/// A data merge placeholder's registers: the source and field it takes, its role, and the item it
/// fills (a text placeholder is the `placeholder` mark on its story's text).
pub(crate) fn placeholder_fields(p: &Placeholder, out: &Out<'_>) -> Fields {
    let item = match p.anchor {
        PlaceholderAnchor::Item { id } => out.ids.items.object(id).map(oid).unwrap_or(Value::Null),
        PlaceholderAnchor::Text { .. } => Value::Null,
    };
    vec![("source", json!(p.source_id)), ("field", json!(p.field)), ("role", json!(p.role.as_str())), ("item", item)]
}

/// A data merge placeholder read back; `text` is its text by its `placeholder` mark. `None` when
/// what it fills is gone.
pub(crate) fn placeholder_from(id: u64, get: &dyn Fn(&str) -> Value, text: Option<&(StoryId, usize, usize)>, inn: &In) -> Option<Placeholder> {
    let anchor = match as_object(&get("item")) {
        Some(item) => PlaceholderAnchor::Item { id: ItemId(*inn.items.get(&item)?) },
        None => {
            let (story, start, end) = *text?;
            PlaceholderAnchor::Text { story, start, end }
        }
    };
    let role = PlaceholderRole::parse(get("role").as_str().unwrap_or_default())?;
    Some(Placeholder {
        id,
        source_id: get("source").as_u64().unwrap_or(0),
        field: get("field").as_str().unwrap_or_default().to_owned(),
        role,
        anchor,
    })
}

/// A named definition's registers (a condition, a text variable, a cross-reference format, as the
/// engine serializes it): its `name`, and the rest as `value`.
pub(crate) fn named_fields(entry: &Value) -> Fields {
    let mut map = match entry {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    let name = take(&mut map, "name");
    vec![("name", name), ("value", json_or_null(map))]
}

/// A named definition read back, as the engine serializes it; `None` when its registers do not make
/// one (a script wrote another shape).
pub(crate) fn named_from<T: DeserializeOwned>(get: &dyn Fn(&str) -> Value) -> Option<T> {
    let mut map = match get("value") {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    map.insert("name".into(), Value::String(get("name").as_str()?.to_owned()));
    serde_json::from_value(Value::Object(map)).ok()
}

// ---------------------------------------------------------------------------------------------
// Swatches, assets

pub(crate) fn swatch_fields(swatch: &kition_layout_doc::color::Swatch, out: &Out<'_>) -> KdmResult<Fields> {
    let mut map = to_map(swatch)?;
    let name = take(&mut map, "name");
    let mut value = take(&mut map, "value");
    out.swatches(&mut value);
    let locked = take(&mut map, "locked");
    let hidden = take(&mut map, "hidden");
    let named = take(&mut map, "named");
    Ok(vec![("name", name), ("value", value), ("locked", locked), ("hidden", hidden), ("named", named)])
}

pub(crate) fn swatch_from(get: &dyn Fn(&str) -> Value, inn: &In) -> KdmResult<kition_layout_doc::color::Swatch> {
    let mut map = Map::new();
    for key in ["name", "value", "locked", "hidden", "named"] {
        let v = get(key);
        if !v.is_null() {
            map.insert(key.into(), v);
        }
    }
    let mut v = Value::Object(map);
    inn.swatches(&mut v);
    serde_json::from_value(v).map_err(kdm_err)
}

pub(crate) fn asset_fields(asset: &Asset, blobs: &mut dyn BlobSink) -> KdmResult<Fields> {
    let blob = serde_json::to_value(blobs.put(&asset.data, &asset.mime)).map_err(kdm_err)?;
    let pixels = asset.pixels.map(|(w, h)| json!([w, h])).unwrap_or(Value::Null);
    let link = portable_link(asset).map_or(Value::Null, |link| json!(link));
    Ok(vec![("blob", blob), ("name", json!(asset.name)), ("pixels", pixels), ("page", json!(asset.page)), ("link", link)])
}

/// An asset's link when it is portable: relative to the document's folder (a package's
/// `Links/…`), with no `..`, no drive and no backslash. An absolute disk path names the host
/// (E5) and is never written.
pub(crate) fn portable_link(asset: &Asset) -> Option<&str> {
    let link = asset.link.as_deref()?;
    let portable = !link.is_empty()
        && !link.starts_with('/')
        && !link.contains('\\')
        && !link.contains(':')
        && !link.split('/').any(|part| part == "..")
        && link.len() <= 1024;
    portable.then_some(link)
}

/// Whether two assets write the same registers (their disk paths aside).
pub(crate) fn same_asset(a: &Asset, b: &Asset) -> bool {
    a.name == b.name
        && a.mime == b.mime
        && a.pixels == b.pixels
        && a.page == b.page
        && portable_link(a) == portable_link(b)
        && (Arc::ptr_eq(&a.data, &b.data) || a.data == b.data)
}

pub(crate) fn asset_from(id: u64, get: &dyn Fn(&str) -> Value, blobs: &dyn BlobSource) -> KdmResult<Asset> {
    let blob: Option<BlobRef> = serde_json::from_value(get("blob")).ok();
    let data = blob.as_ref().and_then(|b| blobs.get(&b.hash)).map(|bytes| bytes.to_vec()).unwrap_or_default();
    let pixels: Option<(u32, u32)> = serde_json::from_value(get("pixels")).ok().flatten();
    Ok(Asset {
        id: AssetId(id),
        name: get("name").as_str().unwrap_or_default().to_owned(),
        mime: blob.map(|b| b.mime).unwrap_or_default(),
        link: get("link").as_str().map(str::to_owned),
        data: Arc::new(data),
        pixels,
        page: get("page").as_u64().and_then(|p| u32::try_from(p).ok()).unwrap_or(0),
    })
}

// ---------------------------------------------------------------------------------------------
// Styles: one `Style` type for five families; `attrs` keys are the family's attribute clusters.

/// A style serialized, split into name, `basedOn`, `next` and its attribute clusters.
pub(crate) fn style_fields(family: usize, style: &impl Serialize, out: &Out<'_>) -> KdmResult<Fields> {
    let mut map = to_map(style)?;
    let name = take(&mut map, "name");
    let scope = crate::ids::FAMILY_NAMES.get(family).ok_or_else(|| kdm_err("invalid style family"))?;
    let mut attrs = Map::new();
    let based_on = match map.remove("basedOn") {
        Some(Value::String(n)) => out.style_ref(family, &n).unwrap_or_else(|| {
            attrs.insert("basedOnName".into(), Value::String(n));
            Value::Null
        }),
        _ => Value::Null,
    };
    let next = match map.remove("nextStyle") {
        Some(Value::String(n)) => out.style_ref(family, &n).unwrap_or_else(|| {
            attrs.insert("nextStyleName".into(), Value::String(n));
            Value::Null
        }),
        _ => Value::Null,
    };
    if family == PARAGRAPH
        && let Some(Value::Object(para)) = map.remove("para")
    {
        for (key, value) in para {
            let group = marks::para_group(&key).ok_or_else(|| kdm_err(format!("paragraph attribute `{key}` has no cluster")))?;
            if let Value::Object(g) = attrs.entry(group).or_insert_with(|| Value::Object(Map::new())) {
                g.insert(key, value);
            }
        }
    }
    if let Some(Value::Object(chars)) = map.remove("chars") {
        attrs.extend(chars);
    }
    attrs.extend(map);
    let attrs: Map<String, Value> = attrs.into_iter().filter(|(_, v)| !v.is_null()).collect();
    let mut attrs = Value::Object(attrs);
    out.swatches(&mut attrs);
    Ok(vec![(*scope, name.clone()), ("name", name), ("basedOn", based_on), ("next", next), ("attrs", attrs)])
}

/// A style of `family` from its registers (`attrs` as the map's entries).
pub(crate) fn style_from<T: DeserializeOwned>(
    family: usize,
    name: Value,
    based_on: Value,
    next: Value,
    attrs: Map<String, Value>,
    inn: &In,
) -> KdmResult<T> {
    let mut map = Map::new();
    map.insert("name".into(), name);
    let resolve = |v: &Value| as_object(v).and_then(|o| inn.names.styles[family].get(&o).cloned());
    let mut para = Map::new();
    let mut chars = Map::new();
    for (key, mut value) in attrs {
        inn.swatches(&mut value);
        if family == PARAGRAPH && marks::is_para_group(&key) {
            if let Value::Object(g) = value {
                para.extend(g);
            }
        } else if (family == PARAGRAPH || family == crate::ids::CHARACTER) && CHAR_KEYS.contains(&key.as_str()) {
            chars.insert(key, value);
        } else if key == "basedOnName" {
            map.insert("basedOn".into(), value);
        } else if key == "nextStyleName" {
            map.insert("nextStyle".into(), value);
        } else {
            map.insert(key, value);
        }
    }
    if let Some(n) = resolve(&based_on) {
        map.insert("basedOn".into(), Value::String(n));
    }
    if let Some(n) = resolve(&next) {
        map.insert("nextStyle".into(), Value::String(n));
    }
    if family == PARAGRAPH {
        map.insert("para".into(), Value::Object(para));
    }
    if family == PARAGRAPH || family == crate::ids::CHARACTER {
        map.insert("chars".into(), Value::Object(chars));
    }
    from_map(map)
}

/// The names of a document's styles, by family (see [`crate::ids::FAMILY_FIELDS`]).
pub(crate) fn style_names(styles: &kition_layout_doc::Styles) -> [Vec<&String>; FAMILIES] {
    [
        styles.paragraph.iter().map(|s: &ParagraphStyle| &s.name).collect(),
        styles.character.iter().map(|s: &CharacterStyle| &s.name).collect(),
        styles.object.iter().map(|s: &ObjectStyle| &s.name).collect(),
        styles.table.iter().map(|s| &s.name).collect(),
        styles.cell.iter().map(|s| &s.name).collect(),
    ]
}

// ---------------------------------------------------------------------------------------------
// Stories, tables, cells

/// The keys of a serialized top-level story the bridge models as fields or objects.
const STORY_MODELLED: &[&str] =
    &["id", "text", "paras", "chars", "frames", "rev", "tables", "notes", "endnotes", "anchors", "xrefs", "indexRefs", "objects", "editorial"];

/// A top-level story's `options`: its direction and the rest, the `link` to the story it copies
/// as that story's object.
pub(crate) fn story_options(story: &Story, out: &Out<'_>) -> KdmResult<Value> {
    let mut map = to_map(story)?;
    for key in STORY_MODELLED {
        map.remove(*key);
    }
    if let Some((parent, rev)) = story.link {
        match out.ids.stories.object(parent) {
            Some(object) => {
                map.insert("link".into(), json!([object.as_str(), rev]));
            }
            None => {
                map.remove("link");
            }
        }
    }
    Ok(json_or_null(map))
}

/// A story shell (no text, frames or inline objects yet) from its `options`.
pub(crate) fn story_from_options(id: u64, options: &Value, inn: &In) -> KdmResult<Story> {
    let mut map = match options {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    };
    if let Some(Value::Array(link)) = map.get("link").cloned() {
        match (link.first().and_then(|o| inn.story(o)), link.get(1)) {
            (Some(parent), Some(rev)) => {
                map.insert("link".into(), json!([parent, rev]));
            }
            _ => {
                map.remove("link");
            }
        }
    }
    map.insert("id".into(), Value::from(id));
    map.insert("text".into(), json!(""));
    map.insert("paras".into(), json!([]));
    map.insert("chars".into(), json!([]));
    map.insert("frames".into(), json!([]));
    from_map(map)
}

/// A table's `style` and `options`.
pub(crate) fn table_fields(table: &kition_layout_doc::Table, out: &Out<'_>) -> KdmResult<Fields> {
    let mut options = to_map(&table.options)?;
    let style = if table.style.is_empty() {
        Value::Null
    } else {
        out.style_ref(TABLE, &table.style).unwrap_or_else(|| {
            options.insert("$style".into(), Value::String(table.style.clone()));
            Value::Null
        })
    };
    let mut options = Value::Object(options);
    out.swatches(&mut options);
    Ok(vec![("style", style), ("options", options)])
}

pub(crate) fn table_from(
    id: u64,
    get: &dyn Fn(&str) -> Value,
    rows: Vec<kition_layout_doc::Row>,
    columns: Vec<kition_layout_doc::Column>,
    cells: Vec<Cell>,
    inn: &In,
) -> KdmResult<kition_layout_doc::Table> {
    let mut options = match get("options") {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let mut style = match options.remove("$style") {
        Some(Value::String(s)) => s,
        _ => String::new(),
    };
    if let Some(name) = as_object(&get("style")).and_then(|o| inn.names.styles[TABLE].get(&o).cloned()) {
        style = name;
    }
    let mut options = Value::Object(options);
    inn.swatches(&mut options);
    Ok(kition_layout_doc::Table { id, rows, columns, cells, options: serde_json::from_value(options).map_err(kdm_err)?, style })
}

/// A row's or column's `props`.
pub(crate) fn line_props(v: &impl Serialize) -> KdmResult<Value> {
    serde_json::to_value(v).map_err(kdm_err)
}

/// A cell's registers and text (`None`: the default cell, which has no object).
pub(crate) fn cell_fields(cell: &Cell, out: &Out<'_>) -> KdmResult<Option<(Fields, Vec<TextRun>)>> {
    if *cell == Cell::default() {
        return Ok(None);
    }
    let mut map = to_map(cell)?;
    let span = if cell.row_span == 1 && cell.col_span == 1 { Value::Null } else { json!({ "rowSpan": cell.row_span, "colSpan": cell.col_span }) };
    map.remove("rowSpan");
    map.remove("colSpan");
    map.remove("text");
    map.remove("style");
    let style = if cell.style.is_empty() {
        Value::Null
    } else {
        out.style_ref(CELL, &cell.style).unwrap_or_else(|| {
            map.insert("$style".into(), Value::String(cell.style.clone()));
            Value::Null
        })
    };
    if let Some(Value::Object(graphic)) = map.get_mut("graphic")
        && let Some(asset) = graphic.get("asset")
    {
        let mapped = out.asset(asset).unwrap_or(Value::Null);
        graphic.insert("asset".into(), mapped);
    }
    map.insert("story".into(), crate::text::nested_extras(&cell.text)?);
    let mut props = Value::Object(map);
    out.swatches(&mut props);
    let runs =
        crate::text::story_runs(&cell.text, &out.names, &crate::text::Anchors { table: &crate::text::nested_table, inlines: None, ranges: &[] })?;
    Ok(Some((vec![("span", span), ("style", style), ("props", props)], runs)))
}

/// A cell from its registers and text runs; the default cell when it holds nothing.
pub(crate) fn cell_from(get: &dyn Fn(&str) -> Value, runs: &[TextRun], inn: &In) -> KdmResult<Cell> {
    let props = get("props");
    if runs.is_empty() && props.is_null() {
        return Ok(Cell::default());
    }
    let mut map = match props {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let extras = map.remove("story").unwrap_or(Value::Null);
    let mut style = match map.remove("$style") {
        Some(Value::String(s)) => s,
        _ => String::new(),
    };
    if let Some(name) = as_object(&get("style")).and_then(|o| inn.names.styles[CELL].get(&o).cloned()) {
        style = name;
    }
    let span = get("span");
    map.insert("rowSpan".into(), span.get("rowSpan").cloned().unwrap_or(json!(1)));
    map.insert("colSpan".into(), span.get("colSpan").cloned().unwrap_or(json!(1)));
    if let Some(Value::Object(graphic)) = map.get_mut("graphic")
        && let Some(asset) = graphic.get("asset")
    {
        let mapped = inn.asset(asset).unwrap_or(Value::from(0));
        graphic.insert("asset".into(), mapped);
    }
    let mut v = Value::Object(map);
    inn.swatches(&mut v);
    let Value::Object(mut map) = v else { return Err(kdm_err("cell props")) };
    map.insert("text".into(), serde_json::to_value(kition_layout_doc::Story::new(StoryId(0))).map_err(kdm_err)?);
    let mut cell: Cell = from_map(map)?;
    cell.style = style;
    cell.text = crate::text::nested_story(runs, &extras, &inn.names)?;
    Ok(cell)
}
