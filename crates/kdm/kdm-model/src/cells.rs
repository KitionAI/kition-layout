//! Table cells (design note section 2.2): a `Table`'s `cells` map keys `<rowId>/<columnId>` to a
//! `Cell` object created under an id derived from the table, row and column objects (the same
//! on every replica, kition-sheet's rule).
//! Every cell has its object, an empty one too, created with its row or column: two people
//! typing into one empty cell at once type into one text and both keep theirs (two concurrent
//! creations of one object would keep only one side's text). An emptied cell is cleared, not
//! deleted (a deleted id cannot be created again); an empty `Cell` reads as the engine's default
//! cell. A cell of a row and a column inserted concurrently by two people has no object until
//! someone types into it.

use kition_kit::kdm::ObjectId;
use kition_kit::session::sha256_hex;

/// The `cells` map key of the cell in `row` and `column`.
pub(crate) fn cell_key(row: &ObjectId, column: &ObjectId) -> String {
    format!("{}/{}", row.as_str(), column.as_str())
}

/// The object id of the cell keyed `key` in `table`: `c` and 96 bits of a SHA-256 in base 62.
pub(crate) fn cell_id(table: &ObjectId, key: &str) -> ObjectId {
    const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let hash = sha256_hex(format!("{}/{key}", table.as_str()).as_bytes());
    let mut n = hash.get(..24).and_then(|h| u128::from_str_radix(h, 16).ok()).unwrap_or_default();
    let mut id = [b'0'; 18];
    id[0] = b'c';
    for digit in id[1..].iter_mut().rev() {
        *digit = DIGITS[(n % 62) as usize];
        n /= 62;
    }
    ObjectId::from_string(String::from_utf8_lossy(&id).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_ids_are_valid_stable_and_distinct() {
        let table = ObjectId::from_string("0192f0aa-0000-7000-8000-000000000001");
        let (r, c) = (ObjectId::from_string("r"), ObjectId::from_string("c"));
        let a = cell_id(&table, &cell_key(&r, &c));
        assert_eq!(a, cell_id(&table, &cell_key(&r, &c)));
        assert_ne!(a, cell_id(&ObjectId::from_string("other"), &cell_key(&r, &c)));
        assert!(a.as_str().len() == 18 && a.as_str().chars().all(|c| c.is_ascii_alphanumeric()));
        let ids: std::collections::HashSet<ObjectId> = (0..10_000).map(|k| cell_id(&table, &format!("r{k}/c"))).collect();
        assert_eq!(ids.len(), 10_000);
    }
}
