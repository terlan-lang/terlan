# std.regex

`std.regex` provides portable regular-expression helpers backed by the
maintained Rust `regex` crate on the VM path. Source code sees opaque `Regex`
values, typed `RegexError` failures, and ordinary `Option`, `Result`, and
`List` return shapes.

Use `std.regex.Regex.compile` when a pattern can fail, then pass the compiled
value to matching helpers. Use `escape` when user text must be treated as a
literal pattern.
