#![allow(dead_code)]
//! Fixtures and helpers shared by the bridge's tests.

use std::sync::Arc;

use kition_layout_doc::build::NewDocument;
use kition_layout_doc::color::{Color, Swatch, SwatchValue};
use kition_layout_doc::geom::{Affine, Point, Rect, shapes};
use kition_layout_doc::{
    AnchorPosition, AnchoredObject, CharAttrs, Content, CrossRef, Document, Fill, IndexRef, Item, ItemId, ParaFormat, Shape, SpreadRef, StoryId,
    Table,
};
use kition_layout_kdm::kition_kit::kdm::{ActorId, DocId, Origin, Replica};
use kition_layout_kdm::kition_kit::session::{KitModel, MemoryBlobs};
use kition_layout_kdm::{Ids, Known, LayoutModel, compare::normalize, read_document, read_document_known, write_changes, write_document};

/// The web editor's sample document.
pub fn sample() -> Document {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../web/public/sample/Sample.klayout")).expect("the sample");
    kition_layout_kdm::decode("Sample.klayout", &bytes).expect("the sample loads")
}

/// A new three-page facing document, as File › New makes it.
pub fn blank() -> Document {
    Document::new(&NewDocument { pages: 3, facing_pages: true, ..Default::default() })
}

/// A document using most of what the model holds: swatches (a tint of another), a threaded story
/// with character and paragraph formatting, a table with a merged cell, footnotes, an endnote, a
/// text anchor with a cross-reference to it, an index entry, an anchored object, a group, a
/// rotated oval, an image, a parent spread with an item and a page using it, a second layer and
/// a section.
pub fn rich() -> Document {
    let mut d = blank();
    d.title = "Rich".into();
    d.swatches.push(Swatch::color("Brand", Color::rgb(1.0, 0.0, 0.0)));
    d.swatches.push(Swatch {
        name: "Brand 30".into(),
        value: SwatchValue::Tint { base: "Brand".into(), tint: 0.3 },
        locked: false,
        named: true,
        hidden: false,
    });
    let lid = d.default_layer();
    let (f1, sid) = d
        .add_text_frame(SpreadRef::Doc(0), Rect::new(40.0, 40.0, 300.0, 200.0), lid, "Héllo wörld — one\nTwo\n\nFour 😀 end", ParaFormat::default())
        .unwrap();
    let (f2, _) = d.add_text_frame(SpreadRef::Doc(1), Rect::new(700.0, 40.0, 900.0, 200.0), lid, "", ParaFormat::default()).unwrap();
    d.thread(f1, f2).unwrap();
    {
        let s = d.story_mut(sid).unwrap();
        s.format_chars(0..5, |f| f.over = CharAttrs { size: Some(20.0), fill: Some("Brand".into()), ..Default::default() });
        s.format_chars(7..12, |f| f.style = "Emphasis".into());
        s.format_paras(0..1, |p| {
            p.para.set_json("leftIndent", &serde_json::json!(18)).unwrap();
            p.para.set_json("wordSpaceMin", &serde_json::json!(0.7)).unwrap();
            p.para.set_json("ruleBelow", &serde_json::json!({ "on": true, "color": "Brand" })).unwrap();
        });
        let end = s.len();
        s.insert_note(3, "A footnote\nin two paragraphs", ParaFormat::default());
        s.insert_endnote(end, "An endnote", ParaFormat::default());
        let anchor = s.insert_anchor(0, "Top");
        s.insert_xref(s.len(), CrossRef { target: anchor, format: "Page Number".into() });
        s.insert_index_ref(2, IndexRef { topics: vec!["Typography".into(), "kerning".into()], ..Default::default() });
        let mut t = Table::new(d_alloc_placeholder(), 2, 3, 1, 0, 240.0);
        t.cell_mut(0, 0).unwrap().text.insert(0, "Head");
        t.cell_mut(0, 0).unwrap().fill = "Brand".into();
        t.cell_mut(1, 2).unwrap().text.insert(0, "Body\nTwo");
        t.columns[0].width = 50.0;
        t.merge(kition_layout_doc::CellRange::new(1, 0, 2, 1)).unwrap();
        let at = s.text.find("Two").unwrap();
        s.insert_table(at, t);
    }
    // An anchored object in the story.
    let anchored_id = ItemId(d.alloc());
    let mut star = Item::new(anchored_id, lid, Shape::Polygon, shapes::rectangle(Rect::new(0.0, 0.0, 20.0, 20.0)));
    star.fill = Fill::swatch("Brand");
    d.story_mut(sid).unwrap().insert_object(1, AnchoredObject::new(star, AnchorPosition::default()));
    // A rotated, semi-transparent oval and a group of two rectangles.
    let oval_id = ItemId(d.alloc());
    let mut oval = Item::new(oval_id, lid, Shape::Oval, shapes::ellipse(Rect::new(650.0, 300.0, 750.0, 380.0)));
    oval.fill = Fill::swatch("Brand 30");
    oval.xf = Affine::rotate_about(0.3, Point::new(700.0, 340.0));
    oval.opacity = 0.5;
    oval.name = "Oval".into();
    d.insert_item(SpreadRef::Doc(1), oval, None).unwrap();
    let mut kids = Vec::new();
    for i in 0..2 {
        let id = ItemId(d.alloc());
        let mut r =
            Item::new(id, lid, Shape::Rectangle, shapes::rectangle(Rect::new(50.0 + 60.0 * f64::from(i), 400.0, 100.0 + 60.0 * f64::from(i), 450.0)));
        r.fill = Fill::swatch("Brand");
        kids.push(Arc::new(r));
    }
    let gid = ItemId(d.alloc());
    let mut group = Item::new(gid, lid, Shape::Group, shapes::rectangle(Rect::new(50.0, 400.0, 160.0, 450.0)));
    group.content = Content::Group { items: kids };
    d.insert_item(SpreadRef::Doc(0), group, None).unwrap();
    // An image.
    let asset = kition_layout_doc::AssetId(d.alloc());
    d.assets.insert(
        asset,
        Arc::new(kition_layout_doc::Asset {
            id: asset,
            name: "dot.png".into(),
            mime: "image/png".into(),
            link: None,
            data: Arc::new(vec![0x89, b'P', b'N', b'G', 1, 2, 3]),
            pixels: Some((1, 1)),
            page: 0,
        }),
    );
    let pic_id = ItemId(d.alloc());
    let mut pic = Item::new(pic_id, lid, Shape::Rectangle, shapes::rectangle(Rect::new(300.0, 300.0, 400.0, 400.0)));
    pic.content = Content::Graphic(kition_layout_doc::Graphic {
        asset,
        size: (1.0, 1.0),
        xf: Affine::scale(100.0),
        auto_fit: Default::default(),
        fit_align: 4,
        crop: [0.0; 4],
    });
    d.insert_item(SpreadRef::Doc(0), pic, None).unwrap();
    // A second layer, and a parent spread with an item on it.
    let layer2 = kition_layout_doc::LayerId(d.alloc());
    d.layers.insert(0, kition_layout_doc::Layer { id: layer2, name: "Top".into(), ..d.layers[0].clone() });
    let parent = d.add_parent("B", "Parent", 2, 12.0, kition_layout_doc::Margins::uniform(36.0));
    let pidx = d.parent_index(parent).unwrap();
    let folio_id = ItemId(d.alloc());
    let folio = Item::new(folio_id, layer2, Shape::Rectangle, shapes::rectangle(Rect::new(10.0, 10.0, 30.0, 30.0)));
    d.insert_item(SpreadRef::Parent(pidx), folio, None).unwrap();
    d.apply_parent(&[1], Some(parent)).unwrap();
    d.sections.push(kition_layout_doc::Section {
        start: 1,
        start_number: Some(5),
        style: Default::default(),
        prefix: "A-".into(),
        marker: String::new(),
        include_prefix: true,
    });
    d.check().unwrap();
    d
}

/// Table ids are a story's own numbers; any number works for a fixture.
fn d_alloc_placeholder() -> u64 {
    77
}

/// The rich document exported to IDML and imported back: what the importer builds.
pub fn from_idml() -> Document {
    kition_layout_idml::import_idml(&kition_layout_idml::export_idml(&rich())).unwrap()
}

/// Every fixture, by name.
pub fn fixtures() -> Vec<(&'static str, Document)> {
    vec![("blank", blank()), ("sample", sample()), ("rich", rich()), ("idml", from_idml())]
}

pub fn actor(n: u64) -> ActorId {
    ActorId::from_u64(n).unwrap()
}

/// A replica holding `doc`, its blobs and the ids the write left.
pub fn replica_of(doc: &Document) -> (Replica, MemoryBlobs, Ids) {
    let mut blobs = MemoryBlobs::default();
    let mut ids = Ids::default();
    let (replica, ()) = Replica::create_with(LayoutModel::schema(), DocId::new(), actor(1), &Origin::system("test"), |tx| {
        write_document(tx, doc, &mut ids, &mut blobs)?;
        Ok(())
    })
    .unwrap();
    (replica, blobs, ids)
}

/// Writes `after` over `before` (which `ids` describe) in one transaction. Answers whether it
/// wrote anything.
pub fn apply_change(replica: &mut Replica, blobs: &mut MemoryBlobs, before: &Document, after: &Document, ids: &mut Ids) -> bool {
    replica.transact(&Origin::person("test"), |tx| write_changes(tx, before, after, ids, blobs)).unwrap()
}

/// The replica read back.
pub fn read(replica: &Replica, blobs: &MemoryBlobs) -> Document {
    read_document(replica, blobs).unwrap().0
}

/// The replica read back keeping `known` ids.
pub fn read_known(replica: &Replica, blobs: &MemoryBlobs, known: &Known) -> (Document, Ids) {
    read_document_known(replica, blobs, known).unwrap()
}

/// Asserts two documents hold the same content, ids aside, naming the first difference.
pub fn assert_same(what: &str, a: &Document, b: &Document) {
    let (x, y) = (normalize(a), normalize(b));
    if x != y {
        let first = first_difference(&x, &y, String::new()).unwrap_or_default();
        panic!("{what}: documents differ at {first}");
    }
}

fn first_difference(a: &serde_json::Value, b: &serde_json::Value, at: String) -> Option<String> {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                let (p, q) = (x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null));
                if p != q {
                    return first_difference(p, q, format!("{at}.{key}"));
                }
            }
            Some(format!("{at}: {} vs {}", short(a), short(b)))
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if p != q {
                    return first_difference(p, q, format!("{at}[{i}]"));
                }
            }
            Some(format!("{at}: {} vs {}", short(a), short(b)))
        }
        _ => Some(format!("{at}: {} vs {}", short(a), short(b))),
    }
}

fn short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 400 { format!("{}…", &s[..400]) } else { s }
}

/// The story of `d` with the most text.
pub fn main_story(d: &Document) -> StoryId {
    d.stories.values().max_by_key(|s| s.text.len()).map(|s| s.id).unwrap()
}
