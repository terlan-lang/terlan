use super::test_support::check_syntax_output_with_std_interfaces;

/// Verifies shape expansion cannot hide an effectful helper call from guard
/// purity validation.
#[test]
fn syntax_output_rejects_effectful_helper_in_shape_guard_after_expansion() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module effectful_shape_guard.\n\
\n\
import std.io.File.{exists}.\n\
\n\
shape Existing(path) =\n\
    path where exists(path).\n\
\n\
pub classify(path: String): Bool ->\n\
    case path {\n\
        Existing(found) -> found == path;\n\
        _ -> false\n\
    }.\n\
",
        "std/io/File.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("case guard must be pure; found effectful imported function call")),
        "diagnostics: {diagnostics:?}"
    );
}
