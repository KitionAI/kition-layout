use super::*;
use kition_kit::kdm::{ObjectId, tool_ids::ToolIds};

impl LayoutDocument {
    pub(super) fn table_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<bool, String> {
        let style_source = command == "style.cell.create" && params.get("fromSelection").and_then(Value::as_bool) == Some(true);
        if !style_source && !command.starts_with("table.") && !matches!(command, "style.cell.apply" | "style.table.apply") {
            return Ok(false);
        }
        let Ok((story, table, mut range)) = kition_layout_engine::cmd::table::table_address(&self.session, params, command) else { return Ok(false) };
        let doc = &self.active()?.doc;
        let Some(source) = doc.story(story).and_then(|story| story.tables.get(&table)) else { return Ok(false) };
        if command == "table.merge" {
            range = source.expand_range(range);
        }
        if style_source && (params.get("table").and_then(Value::as_u64).is_some() || self.active()?.selection.cells.is_none()) {
            // Whole-table defaults only copy the first cell; explicit/selected range ends
            // remain premises because the handler validates them before reading the source.
            if params.get("rows").is_none() {
                range.r1 = range.r0;
            }
            if params.get("cols").is_none() {
                range.c1 = range.c0;
            }
        }
        let Some(kdm) = self.session.kdm() else { return Ok(false) };
        let Some(object) = kdm.model().ids().table(story, table) else { return Ok(false) };
        let replica = kdm.replica();
        let ids = ToolIds::of_replica(replica);
        let id = json!(ids.show(object));
        let rows = replica.list(object, "rows").map_err(crate::engine_error)?;
        let columns = replica.list(object, "columns").map_err(crate::engine_error)?;
        let cell_edit = matches!(command, "table.setCell" | "table.textCell" | "style.cell.apply") && params.get("stroke").is_none();
        let row_edit = command == "table.setRowHeight";
        let col_edit = matches!(command, "table.setColumnWidth" | "table.distributeColumns");
        let selecting = style_source || command.starts_with("table.select") || command == "table.get";
        let insert_row = matches!(command, "table.insertRow" | "table.insertRowAbove" | "table.insertRowBelow");
        let insert_col = matches!(command, "table.insertColumn" | "table.insertColumnLeft" | "table.insertColumnRight");
        let delete_row = command == "table.deleteRow";
        let delete_col = command == "table.deleteColumn";
        let region_edit = matches!(command, "table.merge" | "table.unmerge");
        let structural = insert_row || insert_col || delete_row || delete_col || region_edit;
        let narrow = cell_edit || row_edit || col_edit || selecting || structural;
        let removes_anchor = matches!(command, "table.delete" | "table.convertToText")
            || delete_row && range.r0 == 0 && range.r1.saturating_add(1) >= rows.len()
            || delete_col && range.c0 == 0 && range.c1.saturating_add(1) >= columns.len();
        let row_at =
            if command == "table.insertRowAbove" || command == "table.insertRow" && params.get("where").and_then(Value::as_str) == Some("above") {
                range.r0
            } else {
                range.r1.saturating_add(1)
            };
        let left =
            command == "table.insertColumnLeft" || command == "table.insertColumn" && params.get("where").and_then(Value::as_str) == Some("left");
        let rtl = source.options.direction == kition_layout_engine::doc::TextDirection::RightToLeft;
        let col_at = if left != rtl { range.c0 } else { range.c1.saturating_add(1) };
        let template_row = row_at.min(rows.len().saturating_sub(1));
        let template_col = col_at.min(columns.len().saturating_sub(1));
        under.objects.push(id.clone());
        // Removing the anchor edits story text; cell/line edits only need its existence.
        under.premises.push(if removes_anchor {
            Premise::on(story.0)
        } else {
            Premise::on(story.0).fields(Vec::<String>::new()).positions(Vec::<String>::new())
        });
        under.premises.push(if narrow {
            Premise::on(id.clone()).fields(Vec::<String>::new()).positions(["tables"])
        } else {
            Premise::on(id.clone())
        });
        if insert_col {
            under.premises.push(Premise::on(id.clone()).fields(["options.direction"]).positions(Vec::<String>::new()));
        }
        for (field, lines, lo, hi, content) in [("rows", &rows, range.r0, range.r1, row_edit), ("columns", &columns, range.c0, range.c1, col_edit)] {
            let end = if insert_row && field == "rows" {
                row_at.saturating_add(1).min(lines.len())
            } else if insert_col && field == "columns" {
                col_at.saturating_add(1).min(lines.len())
            } else if narrow {
                hi.saturating_add(1)
            } else {
                lines.len()
            };
            under.within.push(Within { parent: id.clone(), field: field.to_owned(), from: 0, to: end, time: None });
            for (index, line) in lines.iter().enumerate().take(end) {
                under.objects.push(json!(ids.show(line)));
                let premise = Premise::on(ids.show(line));
                under.premises.push(
                    if !narrow
                        || content && index >= lo
                        || insert_row && field == "rows" && (index == template_row || index.saturating_add(1) == row_at)
                        || insert_col && field == "columns" && index == range.c0
                    {
                        premise
                    } else {
                        premise.fields(Vec::<String>::new()).positions([field])
                    },
                );
            }
        }
        let regions = if delete_row || delete_col { source.regions() } else { Vec::new() };
        for (key, value) in replica.map_entries(object, "cells").map_err(crate::engine_error)? {
            let Some(cell) = value.as_str().map(ObjectId::from_string).filter(|cell| replica.exists(cell)) else { continue };
            let position = key
                .split_once('/')
                .and_then(|(row, col)| rows.iter().position(|id| id.as_str() == row).zip(columns.iter().position(|id| id.as_str() == col)));
            let addressed = position.is_some_and(|(row, col)| range.contains(row, col));
            if style_source {
                if position == Some((range.r0, range.c0)) {
                    // The values a new cell style copies, held on the cell itself (its declared
                    // `~styleSource`), so another cell's change leaves this premise alone.
                    under.premises.push(Premise::on(ids.show(&cell)).fields([format!("~{STYLE_SOURCE}")]).positions(Vec::<String>::new()));
                }
                continue;
            }
            let removed = position
                .is_some_and(|(row, col)| delete_row && (range.r0..=range.r1).contains(&row) || delete_col && (range.c0..=range.c1).contains(&col));
            let template = position.is_some_and(|(row, col)| insert_row && row == template_row || insert_col && col == template_col);
            let displaced = position.is_some_and(|(row, col)| {
                regions.iter().any(|&(r, c, rs, cs)| {
                    delete_row
                        && r >= range.r0
                        && r <= range.r1
                        && r.saturating_add(rs) > range.r1.saturating_add(1)
                        && row == range.r1.saturating_add(1)
                        && col == c
                        || delete_col
                            && c >= range.c0
                            && c <= range.c1
                            && c.saturating_add(cs) > range.c1.saturating_add(1)
                            && col == range.c1.saturating_add(1)
                            && row == r
                })
            });
            // blank_like copies both cell formatting and the first text runs' formats.
            if !narrow || cell_edit && addressed || region_edit && addressed || removed || template || displaced {
                under.premises.push(Premise::on(ids.show(&cell)));
            } else if structural {
                under.premises.push(Premise::on(ids.show(&cell)).fields(["span"]).positions(Vec::<String>::new()));
            } else if (cell_edit || selecting) && position.is_some_and(|(row, col)| row <= range.r1 && col <= range.c1) {
                // Ownership of a covered slot is determined by spans anywhere in the table.
                under.premises.push(Premise::on(ids.show(&cell)).fields(["span"]).positions(Vec::<String>::new()));
            }
        }
        Ok(true)
    }

    /// The `~styleSource` of every cell (`wanted`: only of those cells), by the cell's tool id: a
    /// digest of exactly the values `style.cell.create` copies from it.
    pub(super) fn cell_style_sources(&self, wanted: Option<&[Value]>) -> Result<Vec<DerivedValues>, String> {
        let doc = &self.active()?.doc;
        let Some(kdm) = self.session.kdm() else { return Ok(Vec::new()) };
        let replica = kdm.replica();
        let ids = ToolIds::of_replica(replica);
        let mut sources = Vec::new();
        for (story, content) in &doc.stories {
            for (number, table) in &content.tables {
                let Some(object) = kdm.model().ids().table(*story, *number) else { continue };
                let rows = replica.list(object, "rows").map_err(crate::engine_error)?;
                let columns = replica.list(object, "columns").map_err(crate::engine_error)?;
                let row_indices: std::collections::HashMap<_, _> = rows.iter().enumerate().map(|(index, id)| (id.as_str(), index)).collect();
                let col_indices: std::collections::HashMap<_, _> = columns.iter().enumerate().map(|(index, id)| (id.as_str(), index)).collect();
                for (key, value) in replica.map_entries(object, "cells").map_err(crate::engine_error)? {
                    let Some(cell) = value.as_str().map(ObjectId::from_string).filter(|cell| replica.exists(cell)) else { continue };
                    let id = json!(ids.show(&cell));
                    if wanted.is_some_and(|wanted| !wanted.contains(&id)) {
                        continue;
                    }
                    let Some((row, col)) =
                        key.split_once('/').and_then(|(row, col)| row_indices.get(row).copied().zip(col_indices.get(col).copied()))
                    else {
                        continue;
                    };
                    let Some(source) = table.cell(row, col) else { continue };
                    // Exactly the values copied by cmd/table.rs::cell_style_create.
                    let digest = json!(digest(&json!({"fill":source.fill,"tint":source.fill_tint,"insets":source.insets,"vj":source.vj,
                        "stroke":source.strokes.first(),"paragraphStyle":source.text.paras.first().map(|para| &para.style)})));
                    sources.push(DerivedValues { id, values: [(STYLE_SOURCE.to_owned(), digest)].into_iter().collect() });
                }
            }
        }
        Ok(sources)
    }
}
