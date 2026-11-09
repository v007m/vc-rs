//! The merge rules of `vc sync`, as pure functions over three versions: base (the snapshot),
//! theirs (the spec now) and mine (the api file). Each row of the CLI doc's table is one arm.

/// What sync does with one field it owns (method, path, a parameter var, the content type).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field<T> {
    /// The api file keeps what it has: the spec did not change it, or both agree.
    Keep,
    /// The api file takes the spec's value: only the spec changed it.
    Take(T),
    /// Both changed it, differently: reported, and the api is left as it is.
    Conflict,
}

/// What sync does with one operation, matched by `operationId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// New in the spec: the api is added.
    Add,
    /// Gone from the spec while the api file has it: the api is marked `deprecated: true`.
    Deprecate,
    /// In all three, or new in both spec and file: its fields merge one by one.
    Merge,
    /// Nothing to do: the user removed it, or it was never the spec's.
    Leave,
}

/// One field, from its three versions; `base` is `None` when the snapshot lacks it.
pub fn field<T: PartialEq + Clone>(base: Option<&T>, theirs: &T, mine: &T) -> Field<T> {
    let spec_changed = base != Some(theirs);
    let mine_changed = base != Some(mine);
    match (spec_changed, mine_changed) {
        (false, _) => Field::Keep,
        _ if mine == theirs => Field::Keep,
        (true, false) => Field::Take(theirs.clone()),
        (true, true) => Field::Conflict,
    }
}

/// One operation, from whether the snapshot, the spec and the api file have it.
pub const fn operation(base: bool, theirs: bool, mine: bool) -> Operation {
    match (base, theirs, mine) {
        (false, true, false) => Operation::Add,
        (true, false, true) => Operation::Deprecate,
        (_, true, true) => Operation::Merge,
        _ => Operation::Leave,
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, Operation, field, operation};

    #[test]
    fn spec_unchanged_and_mine_changed_keeps_mine() {
        assert_eq!(field(Some(&"/a"), &"/a", &"/mine"), Field::Keep);
    }

    #[test]
    fn spec_changed_and_mine_unchanged_takes_the_spec() {
        assert_eq!(field(Some(&"/a"), &"/b", &"/a"), Field::Take("/b"));
    }

    #[test]
    fn both_changed_the_same_way_is_nothing_to_do() {
        assert_eq!(field(Some(&"/a"), &"/b", &"/b"), Field::Keep);
    }

    #[test]
    fn both_changed_differently_is_a_conflict() {
        assert_eq!(field(Some(&"/a"), &"/b", &"/c"), Field::Conflict);
    }

    #[test]
    fn nothing_changed_keeps_it() {
        assert_eq!(field(Some(&"/a"), &"/a", &"/a"), Field::Keep);
    }

    #[test]
    fn a_field_the_snapshot_lacks_is_taken_only_when_both_agree() {
        assert_eq!(field(None, &"/b", &"/b"), Field::Keep);
        assert_eq!(field(None, &"/b", &"/c"), Field::Conflict);
    }

    #[test]
    fn a_new_operation_is_added() {
        assert_eq!(operation(false, true, false), Operation::Add);
    }

    #[test]
    fn a_removed_operation_is_deprecated() {
        assert_eq!(operation(true, false, true), Operation::Deprecate);
    }

    #[test]
    fn an_operation_in_spec_and_file_merges() {
        assert_eq!(operation(true, true, true), Operation::Merge);
        assert_eq!(operation(false, true, true), Operation::Merge);
    }

    #[test]
    fn an_api_the_user_removed_or_owns_is_left() {
        assert_eq!(operation(true, true, false), Operation::Leave);
        assert_eq!(operation(true, false, false), Operation::Leave);
        assert_eq!(operation(false, false, true), Operation::Leave);
    }
}
