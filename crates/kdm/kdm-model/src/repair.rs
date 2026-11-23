//! The read repair (design note section 2.6, rule V6): what validation needs a story's text marks
//! for, which a transaction cannot read. [`read_repairs`] reads the replica and lists
//!
//! - `Inline` and `Table` objects a story lists that no character of its text marks (the anchor
//!   character was deleted): they are deleted;
//! - placeholder characters whose `inline` mark names an object that is gone (a merge deleted the
//!   inline object), and text variable instances whose `variable` mark names a variable that is
//!   gone (one person deleted the variable while another inserted it), in any text: they are
//!   deleted;
//! - `Hyperlink` and `Placeholder` objects that start from no item and that no character of a
//!   story marks (their text was deleted): they are deleted;
//!
//! and [`apply`] writes them in one `system` transaction (not an undo step), after an import,
//! undo or redo and when a replica opens. Both are deterministic and idempotent (deleting an
//! object or a character twice is the same as once), so replicas repairing the same state at once
//! converge.

use std::collections::HashSet;

use kition_kit::kdm::{ObjectId, Replica, Tx};
use serde_json::Value;

use crate::marks::{INLINE, RANGE_MARKS, TABLE, VARIABLE};
use crate::validate::delete_table;
use crate::{KdmResult, root_id};

/// Unnumbered and locally numbered nouns use their immutable object id at the tool boundary.
/// Written in the same transaction as creation, including model batches and derived cells.
pub(crate) fn store_addresses(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let mut wrote = false;
    for ty in [
        "Guide",
        "Section",
        "Inline",
        "Table",
        "TableRow",
        "TableColumn",
        "Cell",
        "Style",
        "Swatch",
        "Condition",
        "TextVariable",
        "XrefFormat",
        "CommentThread",
        "Comment",
    ] {
        for object in tx.objects_of_type(ty) {
            let address = Value::String(object.as_str().to_owned());
            if tx.get(&object, "address")? != address {
                tx.set(&object, "address", address)?;
                wrote = true;
            }
        }
    }
    Ok(wrote)
}

/// What [`read_repairs`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Repairs {
    /// `(story, inline)`: inline objects no character anchors.
    pub inlines: Vec<(ObjectId, ObjectId)>,
    /// `(story, table)`: tables no paragraph anchors.
    pub tables: Vec<(ObjectId, ObjectId)>,
    /// `(object, character index)` in the object's `text`: anchor characters naming a deleted
    /// inline object and variable instances naming a deleted variable, in descending index order
    /// per object.
    pub chars: Vec<(ObjectId, usize)>,
    /// `(Layout list, object)`: hyperlinks and placeholders that start from nothing.
    pub unanchored: Vec<(&'static str, ObjectId)>,
}

impl Repairs {
    pub fn is_empty(&self) -> bool {
        self.inlines.is_empty() && self.tables.is_empty() && self.chars.is_empty() && self.unanchored.is_empty()
    }
}

/// The indexes of the characters of `object`'s text that are variable instances naming a variable
/// that is gone.
fn gone_variables(replica: &Replica, object: &ObjectId) -> KdmResult<Vec<usize>> {
    let mut out = Vec::new();
    let mut index = 0usize;
    for run in replica.text_delta(object, "text")? {
        let gone = run.attributes.get(VARIABLE).and_then(Value::as_str).is_some_and(|v| !replica.exists(&ObjectId::from_string(v)));
        for c in run.insert.chars() {
            if gone && kition_layout_doc::vars::var_index(c).is_some() {
                out.push(index);
            }
            index += 1;
        }
    }
    Ok(out)
}

/// Finds the V6 repairs of `replica`.
pub fn read_repairs(replica: &Replica) -> KdmResult<Repairs> {
    let mut out = Repairs::default();
    let root = root_id();
    if !replica.exists(&root) {
        return Ok(out);
    }
    let mut ranged = HashSet::new();
    for story in replica.list(&root, "stories")? {
        if !replica.exists(&story) {
            continue;
        }
        let mut marked_inlines = HashSet::new();
        let mut marked_tables = HashSet::new();
        let mut index = 0usize;
        let mut gone = gone_variables(replica, &story)?;
        for run in replica.text_delta(&story, "text")? {
            let len = run.insert.chars().count();
            for mark in RANGE_MARKS {
                if let Some(Value::String(object)) = run.attributes.get(mark) {
                    ranged.insert(ObjectId::from_string(object.as_str()));
                }
            }
            if let Some(Value::String(inline)) = run.attributes.get(INLINE) {
                let inline = ObjectId::from_string(inline.as_str());
                if replica.exists(&inline) {
                    marked_inlines.insert(inline);
                } else {
                    gone.extend(index..index + len);
                }
            }
            if let Some(Value::String(table)) = run.attributes.get(TABLE) {
                marked_tables.insert(ObjectId::from_string(table.as_str()));
            }
            index += len;
        }
        for inline in replica.list(&story, "inlines")? {
            if replica.exists(&inline) && !marked_inlines.contains(&inline) {
                out.inlines.push((story.clone(), inline));
            }
        }
        for table in replica.list(&story, "tables")? {
            if replica.exists(&table) && !marked_tables.contains(&table) {
                out.tables.push((story.clone(), table));
            }
        }
        gone.sort_unstable();
        gone.dedup();
        out.chars.extend(gone.into_iter().rev().map(|i| (story.clone(), i)));
    }
    for ty in ["Cell", "Inline"] {
        for object in replica.objects_of_type(ty) {
            let gone = gone_variables(replica, &object)?;
            out.chars.extend(gone.into_iter().rev().map(|i| (object.clone(), i)));
        }
    }
    for field in ["hyperlinks", "placeholders"] {
        for object in replica.list(&root, field)? {
            if replica.exists(&object) && replica.get(&object, "item")?.is_null() && !ranged.contains(&object) {
                out.unanchored.push((field, object));
            }
        }
    }
    Ok(out)
}

/// Stores engine ids ([`crate::read`]'s numbering of the state `tx` starts from) on the objects
/// that store none: one a script made through the object model, or every object of a file written
/// before the numbers were stored, which so keep the ids they always read with. A deterministic
/// function of the state, as every repair here. Answers whether it wrote anything.
pub fn store_numbers(tx: &mut Tx<'_>, numbers: &[(ObjectId, u64)]) -> KdmResult<bool> {
    let mut wrote = false;
    for (object, number) in numbers {
        if tx.exists(object) {
            tx.set(object, "number", Value::from(*number))?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// Writes `repairs` (found on the state `tx` starts from). Answers whether it wrote anything.
pub fn apply(tx: &mut Tx<'_>, repairs: &Repairs) -> KdmResult<bool> {
    let mut wrote = false;
    for (story, inline) in &repairs.inlines {
        if tx.exists(inline) {
            if let Some(i) = tx.list_items(story, "inlines")?.iter().position(|o| o == inline) {
                tx.list_remove(story, "inlines", i)?;
            }
            tx.delete(inline)?;
            wrote = true;
        }
    }
    for (story, table) in &repairs.tables {
        if tx.exists(table) {
            if let Some(i) = tx.list_items(story, "tables")?.iter().position(|o| o == table) {
                tx.list_remove(story, "tables", i)?;
            }
            delete_table(tx, table)?;
            wrote = true;
        }
    }
    for (object, index) in &repairs.chars {
        if tx.exists(object) && *index < tx.text_len(object, "text")? {
            tx.text_delete(object, "text", *index, 1)?;
            wrote = true;
        }
    }
    for (field, object) in &repairs.unanchored {
        if tx.exists(object) {
            if let Some(i) = tx.list_items(&root_id(), field)?.iter().position(|o| o == object) {
                tx.list_remove(&root_id(), field, i)?;
            }
            tx.delete(object)?;
            wrote = true;
        }
    }
    Ok(wrote)
}
