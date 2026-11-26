//! C8-layout-read-3: what a write reads besides what the families above already hold, as the
//! blind readings name it (`tests/declarations_independent.rs`, rows and projections). A rule
//! names a holder (the root, every story, every top-level item, ...) and the fields or positions
//! the handler reads there. The SDK keeps holding every target and object parameter; these rules
//! only add premises.
use super::*;
use kition_kit::kdm::{ObjectId, tool_ids::ToolIds};
use kition_layout_engine::doc::{SpreadRef, StoryId};

/// Who a rule holds.
#[derive(Clone, Copy)]
enum On {
    /// The object model root.
    Root,
    /// Every story's text (style remapping, change acceptance, variable and condition deletion
    /// rewrite or search every story).
    StoriesText,
    /// The story of the text selection.
    SelectedStory,
    /// Every top-level item of every document spread (not parents).
    TopItems,
    /// Every item of every document spread.
    AllItems,
    /// Every top-level item on every spread that holds a target (`ids` or the selection).
    TargetSpreadItems,
    /// The items a target group holds directly (`ids` or the selection).
    TargetChildren,
    /// The page under each target item (its page-relative coordinates).
    TargetPage,
    /// The items on the pages `pages` names (`one_based` 1-based).
    PageItems { one_based: bool },
    /// The page `spread`/`page` names.
    SpreadPage,
    /// Every page of the spread `spread` names.
    SpreadPages,
    /// The pages of the parents of the pages `pages` names (1-based).
    ParentPages,
    /// The active layer: existence only.
    ActiveLayer,
    /// Every swatch.
    Swatches,
    /// The frames of the story `story` (else the text frame `frame`) names.
    StoryFrames,
    /// Every document page.
    AllPages,
    /// The table the call addresses.
    AddressedTable,
    /// The frames of the selected text's story.
    SelectedStoryFrames,
    /// The paragraph and character styles (and what they are based on) of the formatted text.
    TargetStyles,
    /// The object style new text frames take (`settings.styleDefaults.defaultTextFrame`).
    DefaultTextFrameStyle,
    /// The first document page (a new parent's margins and columns).
    FirstPage,
    /// Every parent spread (a new parent's next free prefix).
    Parents,
    /// The first document spread (objects paste there).
    FirstSpread,
    /// Every top-level table cell.
    AllCells,
    /// Every top-level table.
    AllTables,
    /// The assets the target items' graphics show.
    TargetAssets,
}

/// One rule: whom, which fields, which positions.
struct Rule {
    on: On,
    fields: &'static [&'static str],
    positions: &'static [&'static str],
}

const fn rule(on: On, fields: &'static [&'static str], positions: &'static [&'static str]) -> Rule {
    Rule { on, fields, positions }
}

const STRUCTURE: &[&str] = &["setup", "sections"];
const TRACKING: &[&str] = &["setup.trackChanges"];

/// The rules of `command` with `params`: the reads the handler makes that are not a target, an
/// object parameter or a family's place.
fn rules(command: &str, params: &Value) -> Vec<Rule> {
    let flag = |key: &str| params.get(key).and_then(Value::as_bool).unwrap_or(false);
    match command {
        // Page structure: the setup (size, facing pages, binding) and sections number and place
        // every page; repagination moves the items that follow.
        "layout.pages.insert" | "layout.pages.move" => vec![rule(On::Root, STRUCTURE, &[]), rule(On::TopItems, &["path"], &["xf"])],
        // Every page is resized and laid out again; liquid rules and Adjust Layout move items.
        "layout.documentSetup" => {
            vec![rule(On::Root, STRUCTURE, &[]), rule(On::TopItems, &["path"], &["xf"]), rule(On::AllPages, &["size", "x", "side"], &[])]
        }
        "layout.pages.delete" => vec![rule(On::Root, STRUCTURE, &[])],
        "layout.spreadShuffle" if flag("allow") => vec![rule(On::Root, STRUCTURE, &[]), rule(On::TopItems, &["path"], &["xf"])],
        "layout.pages.toSpread" => vec![rule(On::TopItems, &["path"], &["xf", "items", "spread"])],
        "layout.marginsAndColumns" if flag("adjustLayout") => vec![rule(On::PageItems { one_based: false }, &["path"], &["xf"])],
        "layout.pageSize" => vec![rule(On::PageItems { one_based: true }, &["path"], &["xf"])],
        "layout.section" => vec![rule(On::Root, &["sections"], &[])],
        "layer.selectItems" => vec![rule(On::AllItems, &["layer", "locked"], &[])],
        // The handler refuses to delete the last layer: the count, not the layers' content.
        "layer.delete" => vec![rule(On::Root, &["~layerCount"], &[])],
        "layer.merge" => vec![rule(On::AllItems, &["layer"], &[])],
        "guide.add" | "layout.addGuides" => {
            let mut rules = vec![rule(On::ActiveLayer, &[], &[])];
            if command == "layout.addGuides" {
                rules.push(rule(On::SpreadPage, &["size", "margins", "side", "x"], &["pages"]));
            }
            rules
        }
        // The background goes on the last stored layer (cmd/background.rs:113).
        "page.background" => vec![rule(On::Root, &["setup.bleed", "~lastLayer"], &[]), rule(On::ParentPages, &["size", "x"], &[])],
        "page.transition" if !flag("all") => vec![rule(On::SpreadPages, &["props.transition"], &["pages"])],
        "snippet.place" => vec![rule(On::Root, &["paragraphStyles", "characterStyles", "swatches"], &[])],
        "file.place" => vec![rule(On::SelectedStory, &["text"], &[])],
        "transform.set" => vec![rule(On::TargetPage, &["x", "size"], &[])],
        // Stacking order: the target's siblings' places in the item tree.
        "object.arrange" | "arrange.bringToFront" | "arrange.bringForward" | "arrange.sendBackward" | "arrange.sendToBack" => {
            vec![rule(On::TargetSpreadItems, &[], &["items"])]
        }
        "arrange.ungroup" => vec![rule(On::TargetChildren, &[], &["xf", "items", "spread"])],
        "object.unlockAll" => vec![rule(On::TopItems, &["locked"], &[])],
        "object.showAll" => vec![rule(On::TopItems, &["hidden"], &[])],
        "frame.fitText" => vec![rule(On::Root, &["settings.hyphenationExceptions", "setup.advancedType", "setup.glyphFallback"], &[])],
        "text.insert" | "text.insertSpecial" | "text.delete" => vec![rule(On::Root, TRACKING, &[])],
        "style.exportTag" => vec![rule(On::Root, &["settings.exportTags"], &[])],
        "style.paragraph.delete" | "style.character.delete" | "style.object.apply" | "style.group" => vec![rule(On::StoriesText, &["text"], &[])],
        "swatch.save" => vec![rule(On::Swatches, &["value"], &[])],
        // Replacing a swatch or renaming it (stage 7) points every use at the new name: objects'
        // fills and strokes, text formats, the styles and the tints based on it.
        "swatch.delete" if params.get("replaceWith").is_some() => vec![
            rule(On::AllItems, &["fill", "stroke"], &[]),
            rule(On::StoriesText, &["text"], &[]),
            rule(On::Root, &["paragraphStyles", "characterStyles", "objectStyles"], &[]),
            rule(On::Swatches, &["name", "value", "hidden"], &[]),
        ],
        "swatch.set" if params.get("rename").is_some() || params.get("color").is_some() => vec![
            rule(On::AllItems, &["fill", "stroke"], &[]),
            rule(On::StoriesText, &["text"], &[]),
            rule(On::Root, &["paragraphStyles", "characterStyles", "objectStyles"], &[]),
            rule(On::Swatches, &["name", "value"], &[]),
        ],
        // Redefine Style reads the selected text's formats.
        "style.paragraph.edit" | "style.character.edit" if flag("fromSelection") => vec![rule(On::SelectedStory, &["text"], &[])],
        "swatch.addUnnamed" => vec![rule(On::Swatches, &["hidden"], &[])],
        "swatch.newColorGroup" => vec![rule(On::Root, &["settings.colorGroups"], &[]), rule(On::Swatches, &["locked"], &[])],
        "swatch.moveToGroup" => vec![rule(On::Root, &["settings.colorGroups"], &[])],
        "table.distributeRows" => vec![rule(On::StoryFrames, &["path", "textOptions"], &[])],
        // The drop point is found in the frame's composed table, on the spread.
        "table.dropCells" => vec![rule(On::StoryFrames, &["path", "textOptions"], &["xf"]), rule(On::StoriesText, &["text"], &[])],
        // A selection is clamped to, and a whole row/column/table spans, the table's lines.
        "table.select" | "table.selectTable" | "table.selectRow" | "table.selectColumn" => vec![rule(On::AddressedTable, &["rows", "columns"], &[])],
        "edit.findObjects" => vec![rule(On::AllItems, &["content", "shape"], &[])],
        "edit.findNext" => vec![rule(On::StoriesText, &["text"], &[])],
        "footnote.goToReference" => vec![rule(On::SelectedStory, &["text", "inlines"], &[])],
        "endnote.insert" => vec![rule(On::Root, &["spreads"], &[])],
        "change.acceptAll" | "change.rejectAll" | "condition.delete" => vec![rule(On::StoriesText, &["text"], &[])],
        // The engine's instance stores the variable's index in the list (cmd/variables.rs:81): the
        // order of the variables before it, not just its own entry ("value naming another
        // object"). The model's instance names the variable's object (its `variable` mark).
        "variable.insert" => vec![rule(On::Root, &["textVariables"], &[])],
        "variable.delete" => vec![rule(On::StoriesText, &["text"], &[]), rule(On::Root, &["textVariables"], &[])],
        // Misc family (C8-layout-read-3): settings registers the handlers read whole (names for
        // uniqueness and defaults, counts, the records they patch and shift).
        "article.new" | "article.add" | "article.remove" | "article.options" | "article.delete" => {
            vec![rule(On::Root, &["settings.articles"], &[])]
        }
        "data.source.select" | "data.source.update" => vec![rule(On::Root, &["settings.dataMerge.sources"], &[])],
        "data.placeholder.add" => vec![rule(On::Root, &["placeholders", "hyperlinks"], &[])],
        "data.placeholder.remove" => vec![rule(On::Root, &["placeholders", "hyperlinks"], &[]), rule(On::StoriesText, &["text"], &[])],
        "data.options" => vec![rule(On::Root, &["settings.dataMerge.options"], &[])],
        // A merge copies the whole template; a script's steps name nothing fixed.
        "data.merge" | "script.run" => vec![rule(On::Root, WHOLE, &[])],
        "link.add" | "link.delete" | "link.edit" | "link.goToSource" => vec![rule(On::Root, &["hyperlinks"], &[])],
        "bookmark.rename" | "bookmark.add" | "bookmark.delete" => vec![rule(On::Root, &["settings.bookmarks"], &[])],
        "linkedFile.goTo" => vec![rule(On::AllItems, &["content", "graphic"], &[])],
        // A new frame without x/y goes on the spread's first page margins, on the active layer.
        "math.insert" => vec![rule(On::SpreadPage, &["margins", "size", "x"], &[]), rule(On::ActiveLayer, &[], &[])],
        "object.layerOptions" => vec![rule(On::TargetAssets, &["blob"], &[])],
        "strokeStyle.new" | "strokeStyle.delete" => vec![rule(On::Root, &["settings.strokeStyles"], &[])],
        "xml.newTag" | "xml.tagText" | "xml.tag" | "xml.mapStyle" | "xml.loadDtd" | "xml.deleteDtd" => vec![rule(On::Root, &["settings.xml"], &[])],
        // The tagged items are a `covers` place of the tag (members.rs): an item tagged otherwise does
        // not refuse.
        "xml.deleteTag" => vec![rule(On::Root, &["settings.xml"], &[])],
        "file.exportXml" | "file.importXml" => vec![
            rule(On::Root, &["settings.xml"], &[]),
            rule(On::AllItems, &["props.xmlTag", "content"], &[]),
            // Not `frames`: a premise on a list field holds its members' content at SDK 34dff06, so the
            // threaded frames' paint would refuse it too.
            rule(On::StoriesText, &["text"], &[]),
        ],
        // Creation defaults (C8-layout-read-3 "default" class). A new text frame takes the
        // default paragraph and text-frame styles and the binding's story direction
        // (doc/src/edit.rs:176-190).
        "frame.add" | "frame.grid"
            if params.get("content").and_then(Value::as_str).map_or(params.get("text").is_some(), |content| content == "text") =>
        {
            vec![rule(On::Root, NEW_TEXT_FRAME, &[]), rule(On::DefaultTextFrameStyle, &[], &[])]
        }
        "object.caption" | "story.placeAndLink" => {
            vec![
                rule(On::Root, &["settings.styleDefaults.defaultTextFrame", "setup.rightToLeftBinding"], &[]),
                rule(On::DefaultTextFrameStyle, &[], &[]),
            ]
        }
        "text.onPath" => vec![rule(On::Root, &["setup.rightToLeftBinding"], &[])],
        // A size step or feature toggle starts from the resolved look of the first target;
        // breaking a link writes the resolved look out.
        "format.increaseFontSize" | "format.decreaseFontSize" | "text.openType" | "style.breakLink" => {
            vec![rule(On::TargetStyles, &["attrs", "basedOn"], &[])]
        }
        // A table takes the insertion point's formats and, by default, the caret frame's width.
        "table.insert" | "table.convertFromText" => {
            vec![rule(On::SelectedStory, &["text"], &[]), rule(On::SelectedStoryFrames, &["path", "textOptions"], &["xf"])]
        }
        // A new condition's default color counts the conditions there are.
        "condition.new" => vec![rule(On::Root, &["conditions"], &[])],
        // A new parent copies the setup's size and facing pages and the first page's margins and
        // columns (cmd/layout.rs:185, doc/src/build.rs:233); without a prefix, the next free one.
        "layout.parents.new" => {
            let mut rules = vec![
                rule(On::Root, &["setup.pageWidth", "setup.pageHeight", "setup.facingPages"], &[]),
                rule(On::FirstPage, &["margins", "columns"], &[]),
            ];
            if params.get("prefix").and_then(Value::as_str).is_none() {
                rules.push(rule(On::Parents, &["prefix"], &[]));
            }
            rules
        }
        // Two paths computing one fact ("two paths" class).
        "text.para" => vec![rule(On::TargetStyles, &["attrs", "basedOn"], &[])],
        "style.paragraph.add" if flag("fromSelection") => {
            vec![rule(On::SelectedStory, &["text"], &[]), rule(On::TargetStyles, &["attrs", "basedOn"], &[])]
        }
        // Pasting text inserts into the selected story (tracked when tracking is on); pasting
        // objects puts them on the first document spread.
        "edit.paste" | "edit.pasteInPlace" | "edit.pasteWithoutFormatting" => {
            vec![rule(On::SelectedStory, WHOLE, &[]), rule(On::Root, TRACKING, &[]), rule(On::FirstSpread, &[], &[])]
        }
        // A style edit is applied again to every cell or table using it or a style based on it.
        "style.cell.edit" => vec![rule(On::AllCells, &["style", "props"], &[])],
        "style.table.edit" => vec![rule(On::AllTables, &["style", "options"], &[])],
        _ => vec![],
    }
}

/// Every field of the holder.
const WHOLE: &[&str] = &["*"];

const NEW_TEXT_FRAME: &[&str] = &["settings.styleDefaults.defaultParagraph", "settings.styleDefaults.defaultTextFrame", "setup.rightToLeftBinding"];

/// The replica object of style `name` in family `family` (`FAMILY_FIELDS` order).
fn style_object(replica: &kition_kit::kdm::Replica, family: usize, name: &str) -> Result<Option<ObjectId>, String> {
    let Some(field) = kition_layout_kdm::FAMILY_FIELDS.get(family) else { return Ok(None) };
    for object in replica.list(&ObjectId::from_string(ROOT), field).map_err(crate::engine_error)? {
        if replica.get(&object, "name").map_err(crate::engine_error)? == json!(name) {
            return Ok(Some(object));
        }
    }
    Ok(None)
}

impl LayoutDocument {
    pub(super) fn rest_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        let doc = &state.doc;
        let targets: Vec<ItemId> = params
            .get("ids")
            .and_then(Value::as_array)
            .map(|ids| ids.iter().filter_map(Value::as_u64).map(ItemId).collect())
            .or_else(|| params.get("id").and_then(Value::as_u64).map(|id| vec![ItemId(id)]))
            .unwrap_or_else(|| state.selection.items.clone());
        let numbers = |key: &str| -> Vec<usize> {
            params
                .get(key)
                .into_iter()
                .flat_map(|value| value.as_array().map_or_else(|| vec![value], |values| values.iter().collect()))
                .filter_map(|value| value.as_u64().and_then(|n| usize::try_from(n).ok()))
                .collect()
        };
        let spread_index = || params.get("spread").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
        for Rule { on, fields, positions } in rules(command, params) {
            let mut held: Vec<Value> = Vec::new();
            match on {
                On::Root => held.push(json!(ROOT)),
                On::StoriesText => held.extend(doc.stories.keys().map(|id| json!(id.0))),
                On::SelectedStory => held.extend(state.selection.text.map(|text| json!(text.story.0))),
                On::TopItems => held.extend(doc.spreads.iter().flat_map(|spread| spread.items.iter().map(|item| json!(item.id.0)))),
                On::AllItems => held.extend(doc.all_items().into_iter().map(|id| json!(id.0))),
                On::TargetSpreadItems => {
                    let mut spreads: Vec<SpreadRef> = targets.iter().filter_map(|id| doc.find(*id).map(|loc| loc.spread)).collect();
                    spreads.sort_by_key(|spread| format!("{spread:?}"));
                    spreads.dedup();
                    for spread in spreads.into_iter().filter_map(|spread| doc.spread(spread)) {
                        held.extend(spread.items.iter().map(|item| json!(item.id.0)));
                    }
                }
                On::TargetChildren => {
                    for id in &targets {
                        if let Some(Content::Group { items }) = doc.item(*id).map(|item| &item.content) {
                            held.extend(items.iter().map(|item| json!(item.id.0)));
                        }
                    }
                }
                On::TargetPage => {
                    for id in &targets {
                        if let Some(page) = doc.page_of_item(*id).and_then(|page| doc.page(page)) {
                            held.push(json!(page.id.0));
                        }
                    }
                }
                On::PageItems { one_based } => {
                    for page in numbers("pages") {
                        let page = if one_based { page.saturating_sub(1) } else { page };
                        if let Some((si, pi)) = doc.page_loc(page) {
                            held.extend(doc.page_items(si, pi).into_iter().map(|id| json!(id.0)));
                        }
                    }
                }
                On::SpreadPage => {
                    let page = params.get("page").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
                    if let Some(page) = doc.spreads.get(spread_index()).and_then(|spread| spread.pages.get(page)) {
                        held.push(json!(page.id.0));
                    }
                }
                On::SpreadPages => {
                    if let Some(spread) = doc.spreads.get(spread_index()) {
                        held.extend(spread.pages.iter().map(|page| json!(page.id.0)));
                    }
                }
                On::ParentPages => {
                    let pages = numbers("pages");
                    let pages: Vec<usize> = if pages.is_empty() {
                        (0..doc.page_count()).collect()
                    } else {
                        pages.into_iter().map(|page| page.saturating_sub(1)).collect()
                    };
                    for page in pages {
                        let Some(parent) = doc.page(page).and_then(|page| page.parent) else { continue };
                        if let Some(parent) = doc.parents.iter().find(|spread| spread.id == parent) {
                            held.extend(parent.pages.iter().map(|page| json!(page.id.0)));
                        }
                    }
                }
                On::ActiveLayer => held.push(json!(state.active_layer.0)),
                On::Swatches => {
                    if let Some(kdm) = self.session.kdm() {
                        let replica = kdm.replica();
                        let ids = ToolIds::of_replica(replica);
                        let swatches = replica.list(&ObjectId::from_string(ROOT), "swatches").map_err(crate::engine_error)?;
                        held.extend(swatches.iter().map(|swatch| json!(ids.show(swatch))));
                    }
                }
                On::StoryFrames => {
                    let story = params.get("story").and_then(Value::as_u64).map(StoryId).or_else(|| {
                        let frame = params.get("frame").and_then(Value::as_u64).map(ItemId)?;
                        doc.item(frame)?.text_frame().map(|frame| frame.story)
                    });
                    if let Some(story) = story.and_then(|story| doc.story(story)) {
                        held.extend(story.frames.iter().map(|id| json!(id.0)));
                    }
                }
                On::SelectedStoryFrames => {
                    if let Some(story) = state.selection.text.and_then(|text| doc.story(text.story)) {
                        held.extend(story.frames.iter().map(|id| json!(id.0)));
                    }
                }
                On::TargetStyles => {
                    if let Some(kdm) = self.session.kdm() {
                        let replica = kdm.replica();
                        let ids = ToolIds::of_replica(replica);
                        let mut names: Vec<(usize, String)> = Vec::new();
                        for target in kition_layout_engine::cmd::text::format_targets_pub(&self.session) {
                            let Some(story) = doc.text_story(target.story, target.cell) else { continue };
                            for (para, range) in story.paras.iter().zip(story.para_ranges()) {
                                if range.start <= target.range.end && target.range.start <= range.end {
                                    names.push((0, para.style.clone()));
                                }
                            }
                            let mut at = 0usize;
                            for run in &story.chars {
                                let end = at.saturating_add(run.len);
                                if at <= target.range.end && target.range.start <= end {
                                    names.push((1, run.format.style.clone()));
                                }
                                at = end;
                            }
                        }
                        let mut seen = std::collections::BTreeSet::new();
                        while let Some((family, name)) = names.pop() {
                            if !seen.insert((family, name.clone())) {
                                continue;
                            }
                            let based = if family == 0 {
                                doc.styles.paragraph.iter().find(|style| style.name == name).and_then(|style| style.based_on.clone())
                            } else {
                                doc.styles.character.iter().find(|style| style.name == name).and_then(|style| style.based_on.clone())
                            };
                            names.extend(based.map(|based| (family, based)));
                            if let Some(object) = style_object(replica, family, &name)? {
                                held.push(json!(ids.show(&object)));
                            }
                        }
                    }
                }
                On::DefaultTextFrameStyle => {
                    if let Some(kdm) = self.session.kdm()
                        && let Some(object) = style_object(kdm.replica(), 2, &doc.styles.default_text_frame)?
                    {
                        held.push(json!(ToolIds::of_replica(kdm.replica()).show(&object)));
                    }
                }
                On::FirstPage => held.extend(doc.page(0).map(|page| json!(page.id.0))),
                On::Parents => {
                    held.extend(doc.parents.iter().map(|parent| json!(parent.id.0)));
                    // A parent another writer adds takes a prefix too.
                    under.within.push(Within {
                        parent: json!(ROOT),
                        field: "parents".to_owned(),
                        from: doc.parents.len(),
                        to: doc.parents.len(),
                        time: None,
                    });
                }
                On::FirstSpread => {
                    if state.selection.text.is_none() {
                        held.extend(doc.spreads.first().map(|spread| json!(spread.id.0)));
                    }
                }
                On::AllCells | On::AllTables => {
                    if let Some(kdm) = self.session.kdm() {
                        let replica = kdm.replica();
                        let ids = ToolIds::of_replica(replica);
                        for story in replica.list(&ObjectId::from_string(ROOT), "stories").map_err(crate::engine_error)? {
                            for table in replica.list(&story, "tables").map_err(crate::engine_error)? {
                                if matches!(on, On::AllTables) {
                                    held.push(json!(ids.show(&table)));
                                    continue;
                                }
                                for (_, cell) in replica.map_entries(&table, "cells").map_err(crate::engine_error)? {
                                    if let Some(cell) = cell.as_str().map(ObjectId::from_string).filter(|cell| replica.exists(cell)) {
                                        held.push(json!(ids.show(&cell)));
                                    }
                                }
                            }
                        }
                    }
                }
                On::TargetAssets => {
                    if let Some(kdm) = self.session.kdm() {
                        let ids = ToolIds::of_replica(kdm.replica());
                        for id in &targets {
                            if let Some(Content::Graphic(graphic)) = doc.item(*id).map(|item| &item.content)
                                && let Some(object) = kdm.model().ids().assets.object(graphic.asset.0)
                            {
                                held.push(json!(ids.show(object)));
                            }
                        }
                    }
                }
                On::AllPages => held.extend(doc.spreads.iter().flat_map(|spread| spread.pages.iter().map(|page| json!(page.id.0)))),
                On::AddressedTable => {
                    if let Ok((story, table, _)) = kition_layout_engine::cmd::table::table_address(&self.session, params, command)
                        && let Some(kdm) = self.session.kdm()
                        && let Some(object) = kdm.model().ids().table(story, table)
                    {
                        held.push(json!(ToolIds::of_replica(kdm.replica()).show(object)));
                    }
                }
            }
            held.sort_by_key(Value::to_string);
            held.dedup();
            for id in held {
                under.premises.push(if fields == WHOLE {
                    Premise::on(id)
                } else {
                    Premise::on(id).fields(fields.iter().copied()).positions(positions.iter().copied())
                });
            }
        }
        Ok(())
    }
}
