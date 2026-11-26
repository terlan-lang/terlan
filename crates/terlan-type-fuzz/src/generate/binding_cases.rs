//! Composed lexical and pattern-bound payload generation.

use serde_json::json;

use crate::checked::{CheckedType, GeneratedBody};

pub(super) fn payload_matrix(value: i64) -> GeneratedBody {
    let offset = value + 1;
    let expected = value + offset;
    GeneratedBody::expression(
        CheckedType::Int,
        json!(expected),
        format!(
            concat!(
                "let input = {{[{value}], Some(Payload {{value: {offset}}})}};\n",
                "case input {{\n",
                "    {{[first | _rest], Some(Payload {{value: captured}})}} ->\n",
                "        apply(first, (item: Int) -> item + captured)\n",
                "}}"
            ),
            value = value,
            offset = offset,
        ),
        expected.to_string(),
    )
    .with_prelude(concat!(
        "import std.core.Option.{Some}.\n\n",
        "import type std.collections.List.\n",
        "import type std.core.Option.{Option}.\n\n",
        "pub struct Payload {\n",
        "    value: Int,\n",
        "}.\n\n",
        "apply(value: Int, callback: (Int) -> Int): Int ->\n",
        "    callback(value)."
    ))
}
