//! [`State`]: the reads the repairs and the numbering need, from a replica or from a transaction
//! (which reads its own writes, text and marks included). So the read repair (V6) and the
//! numbering of the objects that store no number run inside a draft's merge or a restore, in its
//! one change and undo step, as well as on a replica opened from a file.

use kition_kit::kdm::{KdmError, ObjectId, Replica, TreeEntry, Tx};
use kition_kit::session::TextRun;
use serde_json::Value;

type KdmResult<T> = std::result::Result<T, KdmError>;

/// A document state to read: a [`Replica`] or a [`Tx`] on one.
pub(crate) trait State {
    fn exists(&self, id: &ObjectId) -> bool;
    fn get(&self, id: &ObjectId, field: &str) -> KdmResult<Value>;
    fn list(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>>;
    fn tree(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<TreeEntry>>;
    fn text_delta(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<TextRun>>;
    fn objects_of_type(&self, ty: &str) -> Vec<ObjectId>;
}

impl State for Replica {
    fn exists(&self, id: &ObjectId) -> bool {
        Replica::exists(self, id)
    }
    fn get(&self, id: &ObjectId, field: &str) -> KdmResult<Value> {
        Replica::get(self, id, field)
    }
    fn list(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        Replica::list(self, id, field)
    }
    fn tree(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<TreeEntry>> {
        Replica::tree(self, id, field)
    }
    fn text_delta(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<TextRun>> {
        Replica::text_delta(self, id, field)
    }
    fn objects_of_type(&self, ty: &str) -> Vec<ObjectId> {
        Replica::objects_of_type(self, ty)
    }
}

impl State for Tx<'_> {
    fn exists(&self, id: &ObjectId) -> bool {
        Tx::exists(self, id)
    }
    fn get(&self, id: &ObjectId, field: &str) -> KdmResult<Value> {
        Tx::get(self, id, field)
    }
    fn list(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        Tx::list_items(self, id, field)
    }
    fn tree(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<TreeEntry>> {
        Tx::tree(self, id, field)
    }
    fn text_delta(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<TextRun>> {
        Tx::text_delta(self, id, field)
    }
    fn objects_of_type(&self, ty: &str) -> Vec<ObjectId> {
        Tx::objects_of_type(self, ty)
    }
}
