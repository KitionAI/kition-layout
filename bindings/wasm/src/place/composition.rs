use super::*;
use kition_kit::kdm::{ObjectId, tool_ids::ToolIds};
use kition_layout_engine::doc::{ParaAttrs, StoryId};

pub(super) fn frame_facts(doc: &kition_layout_engine::doc::Document, sid: StoryId) -> Value {
    let rect = |r: Rect| [r.x0, r.y0, r.x1, r.y1];
    json!(
        kition_layout_compose::frame_specs(doc, sid)
            .into_iter()
            .map(|frame| json!({
                "id":frame.id.0, "area":rect(frame.area), "options":frame.opts,
                "vertical":frame.vertical,
                "exclusions":frame.exclusions.into_iter().map(|e| json!({"rect":rect(e.rect),"mode":e.mode})).collect::<Vec<_>>(),
                "pageName":frame.page_name, "page":frame.page, "grid":frame.grid,
                "leftPage":frame.left_page, "pageRect":frame.page_rect.map(|(a,b)| [rect(a),rect(b)])
            }))
            .collect::<Vec<_>>()
    )
}

fn nested(attrs: &ParaAttrs, names: &mut Vec<(usize, String)>) {
    names.extend(attrs.nested_styles.iter().flatten().map(|s| (1, s.style.clone())));
    names.extend(attrs.grep_styles.iter().flatten().map(|s| (1, s.style.clone())));
    names.extend(attrs.nested_line_styles.iter().flatten().map(|s| (1, s.style.clone())));
}

impl LayoutDocument {
    pub(super) fn composition_premises(&self, sid: StoryId, under: &mut Under) -> Result<(), String> {
        let doc = &self.active()?.doc;
        under.premises.push(Premise::on(sid.0).fields(["~compositionFrames"]).positions(Vec::<String>::new()));
        // Re-read after another agent's write on this server too (see `root_derived`).
        under.premises.push(Premise::on(ROOT).fields([format!("~compositionFrames.{}", sid.0)]).positions(Vec::<String>::new()));
        let Some(kdm) = self.session.kdm() else { return Ok(()) };
        let replica = kdm.replica();
        let ids = ToolIds::of_replica(replica);
        let mut names = Vec::new();
        let mut stories = doc.story(sid).into_iter().collect::<Vec<_>>();
        while let Some(story) = stories.pop() {
            for para in &story.paras {
                names.push((0, para.style.clone()));
                nested(&para.para, &mut names);
            }
            names.extend(story.chars.iter().map(|run| (1, run.format.style.clone())));
            for table in story.tables.values() {
                names.push((3, table.style.clone()));
                for cell in &table.cells {
                    names.push((4, cell.style.clone()));
                    stories.push(&cell.text);
                }
            }
            stories.extend(story.notes.iter().map(|note| &note.text));
        }
        let mut seen = std::collections::HashSet::new();
        let root = ObjectId::from_string("layout");
        while let Some((family, name)) = names.pop() {
            if !seen.insert((family, name.clone())) {
                continue;
            }
            let parent = match family {
                0 => doc.styles.paragraph.iter().find(|s| s.name == name).and_then(|s| {
                    nested(&s.para, &mut names);
                    s.based_on.clone()
                }),
                1 => doc.styles.character.iter().find(|s| s.name == name).and_then(|s| s.based_on.clone()),
                3 => doc.styles.table.iter().find(|s| s.name == name).and_then(|s| {
                    names.extend([&s.header, &s.body, &s.footer, &s.left_column, &s.right_column].into_iter().flatten().map(|n| (4, n.clone())));
                    s.based_on.clone()
                }),
                4 => doc.styles.cell.iter().find(|s| s.name == name).and_then(|s| {
                    names.extend(s.paragraph_style.iter().map(|n| (0, n.clone())));
                    s.based_on.clone()
                }),
                _ => None,
            };
            if let Some(parent) = parent {
                names.push((family, parent));
            }
            let Some(field) = kition_layout_kdm::FAMILY_FIELDS.get(family) else { continue };
            for object in replica.list(&root, field).map_err(crate::engine_error)? {
                if replica.get(&object, "name").map_err(crate::engine_error)? == name {
                    under.premises.push(Premise::on(ids.show(&object)));
                    break;
                }
            }
        }
        Ok(())
    }
}
