//! Ordered lists: making a replica list hold exactly the objects a view holds with as few
//! operations as possible ([`sync_list`]), and pairing up the elements of two versions of a list
//! whose elements have no ids of their own ([`align`]: swatches, styles, symbols, patterns).

use std::collections::{HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};

use crate::KdmResult;

/// Makes `owner.field` list exactly `target` (distinct objects): entries not in `target`, and
/// repeated entries, are removed (their objects are not deleted: the caller decides that); the
/// entries already there keep their place as far as possible (the longest run already in
/// `target`'s order stays put) and the rest are moved; new entries are inserted. So moving one
/// element is one `list_move`. Answers how many list operations it wrote.
pub(crate) fn sync_list(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, target: &[ObjectId]) -> KdmResult<usize> {
    let mut current = tx.list_items(owner, field)?;
    if current == target {
        return Ok(0);
    }
    let wanted: HashSet<&ObjectId> = target.iter().collect();
    let mut ops = 0;
    let mut seen = HashSet::new();
    let keep: Vec<bool> = current.iter().map(|o| wanted.contains(o) && seen.insert(o.clone())).collect();
    for index in (0..current.len()).rev() {
        if !keep[index] {
            tx.list_remove(owner, field, index)?;
            current.remove(index);
            ops += 1;
        }
    }
    // Target positions of what is already there, in its current order; the longest increasing
    // run of them stays.
    let position: HashMap<&ObjectId, usize> = target.iter().enumerate().map(|(i, o)| (o, i)).collect();
    let order: Vec<usize> = current.iter().filter_map(|o| position.get(o).copied()).collect();
    let stays: HashSet<usize> = longest_increasing(&order).into_iter().collect();
    for (j, object) in target.iter().enumerate() {
        if stays.contains(&j) {
            continue;
        }
        let after = match j.checked_sub(1).and_then(|p| target.get(p)) {
            Some(prev) => current.iter().position(|o| o == prev).map_or(0, |i| i + 1),
            None => 0,
        };
        match current.iter().position(|o| o == object) {
            Some(from) => {
                let to = if from < after { after - 1 } else { after };
                if from != to {
                    tx.list_move(owner, field, from, to)?;
                    let moved = current.remove(from);
                    current.insert(to, moved);
                    ops += 1;
                }
            }
            None => {
                tx.list_insert(owner, field, after, object)?;
                current.insert(after, object.clone());
                ops += 1;
            }
        }
    }
    Ok(ops)
}

/// Makes `owner.field` hold exactly `target` as a set, for lists whose order means nothing (the
/// engine keeps those collections by local id, which differs between replicas): entries not in
/// `target` (and repeated ones) are removed, new ones appended in `target`'s order, and the rest
/// keep their places, so no replica reorders what another listed. Answers how many list
/// operations it wrote.
pub(crate) fn sync_set(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, target: &[ObjectId]) -> KdmResult<usize> {
    let mut current = tx.list_items(owner, field)?;
    let wanted: HashSet<&ObjectId> = target.iter().collect();
    let mut ops = 0;
    let mut seen = HashSet::new();
    let keep: Vec<bool> = current.iter().map(|o| wanted.contains(o) && seen.insert(o.clone())).collect();
    for index in (0..current.len()).rev() {
        if !keep[index] {
            tx.list_remove(owner, field, index)?;
            current.remove(index);
            ops += 1;
        }
    }
    let present: HashSet<ObjectId> = current.iter().cloned().collect();
    for object in target {
        if !present.contains(object) {
            tx.list_insert(owner, field, current.len(), object)?;
            current.push(object.clone());
            ops += 1;
        }
    }
    Ok(ops)
}

/// The values of one longest strictly increasing subsequence of `values`.
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // tails[k]: index into `values` of the smallest tail of an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; values.len()];
    for (i, &v) in values.iter().enumerate() {
        let k = tails.partition_point(|&t| values[t] < v);
        prev[i] = k.checked_sub(1).map(|p| tails[p]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(values[i]);
        at = prev[i];
    }
    out.reverse();
    out
}

/// Pairs the elements of `before` and `after` (each with a value and a key): first the longest
/// common subsequence of equal values (an element this edit did not touch), then, between
/// consecutive pairs of that, the remaining elements with equal keys in order (an element this edit
/// changed), and last a lone element left on each side of such a gap (one element renamed and
/// changed: a swatch renamed keeps its object, so references to it follow the rename). Answers,
/// for each `after` element, its `before` partner.
pub(crate) fn align<V: PartialEq, K: PartialEq>(before: &[(V, K)], after: &[(V, K)]) -> Vec<Option<usize>> {
    let (n, m) = (before.len(), after.len());
    let mut partner: Vec<Option<usize>> = vec![None; m];
    let mut anchors: Vec<(usize, usize)> = Vec::new();
    // O(n·m): capped, as the lists are file data.
    if n.saturating_mul(m) <= 1_000_000 {
        let mut dp = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i][j] = if before[i].0 == after[j].0 { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if before[i].0 == after[j].0 {
                anchors.push((i, j));
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let mut start = (0, 0);
    for &(i, j) in anchors.iter().chain(std::iter::once(&(n, m))) {
        let mut free: Vec<usize> = (start.0..i).collect();
        for b in start.1..j {
            if let Some(k) = free.iter().position(|&a| before[a].1 == after[b].1) {
                partner[b] = Some(free.remove(k));
            }
        }
        // One element left on each side of the gap: the same element, renamed and edited.
        let unpaired: Vec<usize> = (start.1..j).filter(|&b| partner[b].is_none()).collect();
        if let ([a], [b]) = (free.as_slice(), unpaired.as_slice()) {
            partner[*b] = Some(*a);
        }
        if i < n && j < m {
            partner[j] = Some(i);
        }
        start = (i + 1, j + 1);
    }
    partner
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::kdm::{ActorId, DocId, Origin, Replica};
    use kition_kit::session::KitModel;

    fn with_list(n: usize) -> (Replica, Vec<ObjectId>) {
        let mut r = Replica::create(crate::LayoutModel::schema(), DocId::new(), ActorId::from_u64(1).unwrap()).unwrap();
        let root = crate::root_id();
        let objects = r
            .transact(&Origin::system("t"), |tx| {
                tx.create_with_id("Layout", &root, crate::write::placeholder("Layout"))?;
                let mut out = Vec::new();
                for i in 0..n {
                    let o = tx.create("Layer", crate::write::placeholder("Layer"))?;
                    tx.list_insert(&root, "layers", i, &o)?;
                    out.push(o);
                }
                Ok(out)
            })
            .unwrap();
        (r, objects)
    }

    fn sync(r: &mut Replica, target: &[ObjectId]) -> usize {
        let root = crate::root_id();
        let ops = r.transact(&Origin::system("t"), |tx| sync_list(tx, &root, "layers", target)).unwrap();
        assert_eq!(r.list(&root, "layers").unwrap(), target);
        ops
    }

    #[test]
    fn one_move_is_one_operation() {
        let (mut r, o) = with_list(5);
        assert_eq!(sync(&mut r, &[o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone(), o[0].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[0].clone(), o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 4);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 0);
        assert_eq!(sync(&mut r, &[]), 5);
    }

    #[test]
    fn align_pairs_untouched_then_changed_elements() {
        let before = [(1, ()), (2, ()), (3, ())];
        let after = [(1, ()), (9, ()), (2, ()), (30, ())];
        assert_eq!(align(&before, &after), vec![Some(0), None, Some(1), Some(2)]);
        let renamed_before = [(1, 'a'), (2, 'b'), (3, 'c')];
        let renamed_after = [(1, 'a'), (20, 'x'), (3, 'c')];
        assert_eq!(align(&renamed_before, &renamed_after), vec![Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn longest_increasing_run() {
        assert_eq!(longest_increasing(&[3, 0, 1, 2]), vec![0, 1, 2]);
        assert_eq!(longest_increasing(&[]), Vec::<usize>::new());
    }
}
