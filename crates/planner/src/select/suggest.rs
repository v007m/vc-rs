//! "Did you mean": the known IDs within a few edits of an unknown selector.

/// At most this many suggestions are given.
const MAX: usize = 3;

/// The names within `len / 3` edits (at least one) of `wanted`, ignoring case, closest first.
pub fn closest<'n>(wanted: &str, names: impl Iterator<Item = &'n str>) -> Vec<String> {
    let wanted = wanted.to_lowercase();
    let reach = (wanted.chars().count() / 3).max(1);
    let mut near: Vec<(usize, &str)> = names
        .map(|name| (strsim::osa_distance(&wanted, &name.to_lowercase()), name))
        .filter(|(edits, _)| *edits <= reach)
        .collect();
    near.sort_unstable();
    near.dedup();
    near.into_iter().take(MAX).map(|(_, name)| name.to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::closest;

    #[test]
    fn suggests_near_names_closest_first_and_at_most_three() {
        let names = ["users.crud", "users.crud#getsUser", "users.curd", "orders.crud", "users.cru", "users.crux"];
        assert_eq!(closest("users.crd", names.into_iter()), ["users.cru", "users.crud", "users.curd"]);
        assert_eq!(closest("USERS.CRUD#GETSUSR", names.into_iter()), ["users.crud#getsUser"]);
        assert_eq!(closest("zzzz", names.into_iter()), Vec::<String>::new());
        assert_eq!(closest("ab", ["ac", "abcd"].into_iter()), ["ac"]);
    }
}
