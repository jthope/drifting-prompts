//! The rules the camera, group and view lists share: ids, lookups and names. A hand edit can leave
//! an id blank or repeated, so loading gives those a fresh one.

use std::{collections::HashSet, fmt, hash::Hash};

use crate::{
    StoreError,
    camera_config::CameraConfig,
    camera_group::CameraGroup,
    camera_view::CameraView,
    ids::{CameraId, GroupId, ViewId},
};

pub(crate) trait RecordId: Clone + Eq + Hash + fmt::Display {
    const NOUN: &'static str;

    fn generate() -> Self;

    fn is_empty(&self) -> bool;
}

pub(crate) trait Keyed {
    type Id: RecordId;

    fn id(&self) -> &Self::Id;

    fn id_mut(&mut self) -> &mut Self::Id;
}

impl Keyed for CameraConfig {
    type Id = CameraId;

    fn id(&self) -> &CameraId {
        &self.id
    }

    fn id_mut(&mut self) -> &mut CameraId {
        &mut self.id
    }
}

impl Keyed for CameraGroup {
    type Id = GroupId;

    fn id(&self) -> &GroupId {
        &self.id
    }

    fn id_mut(&mut self) -> &mut GroupId {
        &mut self.id
    }
}

impl Keyed for CameraView {
    type Id = ViewId;

    fn id(&self) -> &ViewId {
        &self.id
    }

    fn id_mut(&mut self) -> &mut ViewId {
        &mut self.id
    }
}

/// Gives a fresh id to every blank or repeated one; the first with a repeated id keeps it, so
/// whatever lists that id still means the item it meant.
pub(crate) fn give_unique_ids<T: Keyed>(items: &mut [T]) -> bool {
    let mut seen = HashSet::new();
    let mut changed = false;
    for item in items {
        if item.id().is_empty() || seen.contains(item.id()) {
            *item.id_mut() = T::Id::generate();
            changed = true;
        }
        seen.insert(item.id().clone());
    }
    changed
}

pub(crate) fn find<'a, T: Keyed>(items: &'a [T], id: &T::Id) -> Option<&'a T> {
    items.iter().find(|item| item.id() == id)
}

pub(crate) fn find_mut<'a, T: Keyed>(
    items: &'a mut [T],
    id: &T::Id,
) -> Result<&'a mut T, StoreError> {
    items
        .iter_mut()
        .find(|item| item.id() == id)
        .ok_or_else(|| not_found(id))
}

pub(crate) fn position<T: Keyed>(items: &[T], id: &T::Id) -> Result<usize, StoreError> {
    items
        .iter()
        .position(|item| item.id() == id)
        .ok_or_else(|| not_found(id))
}

pub(crate) fn not_found<I: RecordId>(id: &I) -> StoreError {
    StoreError::NotFound(format!("no {} `{id}`", I::NOUN))
}

pub(crate) fn required_name<I: RecordId>(name: &str) -> Result<String, StoreError> {
    let name = name.trim();
    if name.is_empty() {
        Err(StoreError::Invalid(format!("A {} needs a name.", I::NOUN)))
    } else {
        Ok(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str) -> CameraGroup {
        CameraGroup {
            id: GroupId::from(id),
            ..CameraGroup::default()
        }
    }

    #[test]
    fn blank_and_repeated_ids_get_fresh_ones_and_the_first_keeps_its_own() {
        let mut groups = vec![group("a"), group(""), group("a"), group("b")];

        assert!(give_unique_ids(&mut groups));

        assert_eq!(groups[0].id.as_str(), "a", "the first keeps it");
        assert_eq!(groups[3].id.as_str(), "b");
        let ids: HashSet<_> = groups.iter().map(|group| group.id.clone()).collect();
        assert_eq!(ids.len(), 4, "every id is its own");
        assert!(!give_unique_ids(&mut groups), "nothing left to repair");
    }

    #[test]
    fn an_unknown_id_is_named_with_its_kind() {
        let mut groups = vec![group("a")];

        assert!(find_mut(&mut groups, &GroupId::from("a")).is_ok());
        assert_eq!(
            find_mut(&mut groups, &GroupId::from("z"))
                .unwrap_err()
                .to_string(),
            "no group `z`"
        );
        assert_eq!(
            position(&groups, &GroupId::from("y"))
                .unwrap_err()
                .to_string(),
            "no group `y`"
        );
        assert!(find(&groups, &GroupId::from("y")).is_none());
    }

    #[test]
    fn a_name_is_trimmed_and_must_be_there() {
        assert_eq!(required_name::<ViewId>("  Porch ").unwrap(), "Porch");
        assert_eq!(
            required_name::<GroupId>("  ").unwrap_err().to_string(),
            "A group needs a name."
        );
    }
}
