pub(super) fn starts_with_ascii_lowercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
}

pub(super) fn starts_with_ascii_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}
