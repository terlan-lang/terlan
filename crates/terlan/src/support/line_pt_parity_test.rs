use super::{caret_underline, line_column, render_text_diagnostic};
use crate::support::test_fs::{temp_path, write_file};
use crate::terlan_syntax::{parse_module_as_syntax_output, EbnfCompileError};
use crate::ColorChoice;

/// Preserves the parser's exact source span through human-readable diagnostics.
#[test]
fn line_pt_parity_reports_one_based_utf8_source_columns() {
    let source = "module line_pt.\n\npub run(): Int ->\n    let label = \"α\";@.\n";
    let EbnfCompileError::Parse(message, span) =
        parse_module_as_syntax_output(source).expect_err("invalid expression must fail")
    else {
        panic!("invalid expression must produce a parse diagnostic");
    };
    let expected_start = source.find(';').expect("statement delimiter offset");
    assert_eq!(span.start, expected_start);

    let path = temp_path("line_pt_parity", "utf8_column").with_extension("terl");
    write_file(&path, source);
    let rendered = render_text_diagnostic(
        "parse_error",
        &message,
        path.to_str().expect("UTF-8 fixture path"),
        span.start,
        span.end,
        ColorChoice::Never,
    );

    assert!(
        rendered.contains(&format!("{}:4:20", path.display())),
        "diagnostic omitted the character-based source position:\n{rendered}"
    );
    assert!(rendered.contains("4 |     let label = \"α\";@."));
}

/// Keeps malformed or multibyte spans bounded to visible source characters.
#[test]
fn line_pt_parity_clamps_offsets_and_uses_character_width_underlines() {
    let source = "αβ\nγ";

    assert_eq!(line_column(source, 0), (1, 1));
    assert_eq!(line_column(source, "α".len()), (1, 2));
    assert_eq!(line_column(source, "αβ\n".len()), (2, 1));
    assert_eq!(line_column(source, usize::MAX), (2, 2));
    assert_eq!(caret_underline("α", "α", 1, 0, "α".len()), "^");
}
