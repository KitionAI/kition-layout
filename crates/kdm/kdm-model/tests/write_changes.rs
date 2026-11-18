//! `write_changes` writes what an edit changed: after every kind of edit, the replica reads back
//! as the edited document, keeping the local ids the view had, and an edit that changes nothing
//! writes nothing.

mod common;

use std::sync::Arc;

use common::*;
use kition_layout_doc::geom::{Rect, shapes};
use kition_layout_doc::{CharAttrs, Content, Document, Fill, Item, ItemId, ParaFormat, Shape, SpreadRef};
use kition_layout_kdm::Known;
use kition_layout_kdm::kition_kit::kdm::Origin;

/// Writes `rich()`, applies `edit` to a copy, writes the difference, and checks the replica reads
/// back as the edited document with the view's ids. Answers the edited document.
fn check(what: &str, edit: impl FnOnce(&mut Document)) -> Document {
    let before = rich();
    let (mut replica, mut blobs, mut ids) = replica_of(&before);
    let mut after = before.clone();
    edit(&mut after);
    after.check().unwrap_or_else(|e| panic!("{what}: the edit left an inconsistent document: {e:?}"));
    assert!(apply_change(&mut replica, &mut blobs, &before, &after, &mut ids), "{what}: wrote nothing");
    let mut known = Known::default();
    known.remember(&after);
    let (back, _) = read_known(&replica, &blobs, &known);
    assert_same(what, &after, &back);
    // The view's ids survive: items, stories and spreads read back under the ids the edit used.
    let item_ids = |d: &Document| {
        let mut v: Vec<u64> = d.spreads.iter().chain(&d.parents).flat_map(|s| s.items.iter().map(|i| i.id.0)).collect();
        v.sort_unstable();
        v
    };
    assert_eq!(item_ids(&after), item_ids(&back), "{what}: item ids");
    assert_eq!(after.stories.keys().collect::<Vec<_>>(), back.stories.keys().collect::<Vec<_>>(), "{what}: story ids");
    // And a second write of the same state is a no-op.
    let wrote = replica.transact(&Origin::person("test"), |tx| kition_layout_kdm::write_changes(tx, &after, &after, &mut ids, &mut blobs)).unwrap();
    assert!(!wrote, "{what}: a write of an unchanged document wrote something");
    back
}

fn story_mut(d: &mut Document) -> &mut kition_layout_doc::Story {
    let sid = main_story(d);
    d.story_mut(sid).unwrap()
}

#[test]
fn an_unchanged_document_writes_nothing() {
    let doc = rich();
    let (mut replica, mut blobs, mut ids) = replica_of(&doc);
    assert!(!apply_change(&mut replica, &mut blobs, &doc, &doc.clone(), &mut ids));
}

#[test]
fn typing_formatting_and_paragraphs() {
    check("typing in the middle", |d| story_mut(d).insert(8, "inserted "));
    check("typing at the end", |d| {
        let s = story_mut(d);
        let end = s.len();
        s.insert(end, " and more");
    });
    check("deleting", |d| story_mut(d).delete(2..6));
    check("bold", |d| story_mut(d).format_chars(4..9, |f| f.over = CharAttrs { font_style: Some("Bold".into()), ..Default::default() }));
    check("a paragraph indent", |d| story_mut(d).format_paras(10..11, |p| p.para.set_json("rightIndent", &serde_json::json!(9)).unwrap()));
    check("a paragraph style", |d| story_mut(d).format_paras(0..1, |p| p.style = "Heading".into()));
    check("splitting a paragraph", |d| story_mut(d).insert(6, "\n"));
    check("joining paragraphs", |d| {
        let s = story_mut(d);
        let at = s.text.find('\n').unwrap();
        s.delete(at..at + 1);
    });
}

#[test]
fn inline_objects_and_tables() {
    check("a new footnote", |d| {
        story_mut(d).insert_note(1, "Another note", ParaFormat::default());
    });
    check("editing a footnote", |d| {
        let s = story_mut(d);
        Arc::make_mut(&mut s.notes[0]).text.insert(0, "Edited: ");
    });
    check("deleting the cross-reference", |d| {
        let s = story_mut(d);
        let at = s.text.find(kition_layout_doc::XREF_MARK).unwrap();
        s.delete(at..at + kition_layout_doc::XREF_MARK.len_utf8());
    });
    check("typing into a cell", |d| {
        let s = story_mut(d);
        let t = Arc::make_mut(s.tables.values_mut().next().unwrap());
        t.cell_mut(1, 2).unwrap().text.insert(0, "More ");
        t.cell_mut(0, 2).unwrap().text.insert(0, "Was empty");
    });
    check("a row inserted in the middle", |d| {
        let s = story_mut(d);
        Arc::make_mut(s.tables.values_mut().next().unwrap()).insert_rows(1, 1);
    });
    check("a row deleted", |d| {
        let s = story_mut(d);
        Arc::make_mut(s.tables.values_mut().next().unwrap()).delete_rows(0, 1);
    });
    check("a cell emptied", |d| {
        let s = story_mut(d);
        let t = Arc::make_mut(s.tables.values_mut().next().unwrap());
        *t.cell_mut(0, 0).unwrap() = kition_layout_doc::Cell::default();
    });
}

#[test]
fn items_spreads_and_pages() {
    check("an item moved to another spread", |d| {
        let oval = d.spreads[1].items.iter().find(|i| i.shape == Shape::Oval).unwrap().id;
        let item = d.remove_item(oval).unwrap();
        d.insert_item(SpreadRef::Doc(0), Arc::unwrap_or_clone(item), None).unwrap();
    });
    check("brought to the front", |d| {
        let spread = Arc::make_mut(&mut d.spreads[0]);
        let first = spread.items.remove(0);
        spread.items.push(first);
    });
    check("an item recoloured and moved", |d| {
        let id = d.spreads[0].items.last().unwrap().id;
        let item = d.item_mut(id).unwrap();
        item.fill = Fill::swatch("Brand 30");
        item.xf = kition_layout_doc::geom::Affine::translate((5.0, 7.0)) * item.xf;
    });
    check("a group's child deleted", |d| {
        let group = d.spreads[0].items.iter().find(|i| matches!(i.content, Content::Group { .. })).unwrap().id;
        let child = match &d.item(group).unwrap().content {
            Content::Group { items } => items[0].id,
            _ => unreachable!(),
        };
        d.remove_item(child).unwrap();
    });
    check("two items grouped", |d| {
        let lid = d.default_layer();
        let a = Item::new(ItemId(d.alloc()), lid, Shape::Rectangle, shapes::rectangle(Rect::new(0.0, 0.0, 10.0, 10.0)));
        let a = d.insert_item(SpreadRef::Doc(0), a, None).unwrap();
        let picture = d.spreads[0].items.iter().find(|i| matches!(i.content, Content::Graphic(_))).unwrap().id;
        let one = d.remove_item(a).unwrap();
        let two = d.remove_item(picture).unwrap();
        let mut group = Item::new(ItemId(d.alloc()), lid, Shape::Group, shapes::rectangle(Rect::new(0.0, 0.0, 400.0, 400.0)));
        group.content = Content::Group { items: vec![one, two] };
        d.insert_item(SpreadRef::Doc(0), group, None).unwrap();
    });
    check("an item deleted", |d| {
        let id = d.spreads[1].items.iter().find(|i| i.shape == Shape::Oval).unwrap().id;
        d.remove_item(id).unwrap();
    });
    check("pages inserted", |d| {
        d.insert_pages(Some(0), 2, None).unwrap();
    });
    check("a page deleted", |d| d.delete_pages(&[2]).unwrap());
    check("a new text frame", |d| {
        let lid = d.default_layer();
        d.add_text_frame(SpreadRef::Doc(1), Rect::new(10.0, 10.0, 90.0, 90.0), lid, "New story", ParaFormat::default()).unwrap();
    });
    check("a story's frames deleted", |d| {
        let sid = main_story(d);
        for frame in d.stories[&sid].frames.clone() {
            d.remove_item(frame).unwrap();
        }
    });
    check("a frame unthreaded", |d| {
        let sid = main_story(d);
        let first = d.stories[&sid].frames[0];
        d.unthread_after(first).unwrap();
    });
}

#[test]
fn swatches_styles_layers_and_settings() {
    let back = check("a swatch renamed with its users", |d| {
        let i = d.swatches.iter().position(|s| s.name == "Brand").unwrap();
        d.swatches[i].name = "Crimson".into();
        for s in d.swatches.iter_mut() {
            if let kition_layout_doc::color::SwatchValue::Tint { base, .. } = &mut s.value
                && base == "Brand"
            {
                *base = "Crimson".into();
            }
        }
        let rename = |item: &mut Item| {
            if item.fill.swatch == "Brand" {
                item.fill.swatch = "Crimson".into();
            }
        };
        for spread in d.spreads.iter_mut().chain(d.parents.iter_mut()) {
            for item in &mut Arc::make_mut(spread).items {
                let item = Arc::make_mut(item);
                rename(item);
                if let Content::Group { items } = &mut item.content {
                    for child in items {
                        rename(Arc::make_mut(child));
                    }
                }
            }
        }
        for story in d.stories.values_mut() {
            let story = Arc::make_mut(story);
            story.format_chars(0..story.len(), |f| {
                if f.over.fill.as_deref() == Some("Brand") {
                    f.over.fill = Some("Crimson".into());
                }
            });
            story.format_paras(0..story.len(), |p| {
                if let Some(rule) = &mut p.para.rule_below
                    && rule.color == "Brand"
                {
                    rule.color = "Crimson".into();
                }
            });
            for object in &mut story.objects {
                rename(&mut Arc::make_mut(object).item);
            }
            for table in story.tables.values_mut() {
                for cell in &mut Arc::make_mut(table).cells {
                    if cell.fill == "Brand" {
                        cell.fill = "Crimson".into();
                    }
                }
            }
        }
    });
    assert!(back.swatch("Crimson").is_some() && back.swatch("Brand").is_none());
    check("a swatch added", |d| d.swatches.push(kition_layout_doc::color::Swatch::color("Sea", kition_layout_doc::color::Color::rgb(0.0, 0.3, 0.6))));
    check("a paragraph style added and one changed", |d| {
        let styles = Arc::make_mut(&mut d.styles);
        let mut h = styles.paragraph[0].clone();
        h.name = "Heading".into();
        h.para.set_json("spaceBefore", &serde_json::json!(12)).unwrap();
        styles.paragraph.push(h);
        styles.paragraph[0].chars.size = Some(11.0);
    });
    check("a layer hidden and a layer added", |d| {
        d.layers[0].visible = false;
        let mut l = d.layers[0].clone();
        l.id = kition_layout_doc::LayerId(d.alloc());
        l.name = "Notes".into();
        d.layers.push(l);
    });
    check("settings", |d| {
        d.user_words.push("Kition".into());
        d.settings.facing_pages = !d.settings.facing_pages;
        d.title = "Renamed".into();
    });
}
