/// Reports whether `std.http.Response` exposes the requested function/arity pair.
pub(super) fn supports_operation(function: &str, arity: usize) -> bool {
    matches!(
        (function, arity),
        ("json", 1 | 2)
            | ("json_text", 1 | 2)
            | ("text", 1 | 2)
            | ("html", 1 | 2)
            | ("file", 1..=3)
            | ("stream", 1 | 5)
            | ("redirect", 1 | 2)
    )
}
