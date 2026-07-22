use super::{caret_underline, line_column, render_text_diagnostic};
use crate::support::test_fs::{temp_path, write_file};
use crate::ColorChoice;

/// Keeps mixed tab and UTF-8 source positions stable in rendered diagnostics.
#[test]
fn col_utf8_parity_preserves_character_columns_and_tab_alignment() {
    let source = "module col_utf8.\n\npub run(): Int ->\n\tlet value = \"åäö\"; missing.\n";
    let start = source.find("missing").expect("diagnostic token offset");
    let end = start + "missing".len();
    assert_eq!(line_column(source, start), (4, 21));

    let path = temp_path("col_utf8_parity", "tab_alignment").with_extension("terl");
    write_file(&path, source);
    let rendered = render_text_diagnostic(
        "parse_error",
        "synthetic location check",
        path.to_str().expect("UTF-8 fixture path"),
        start,
        end,
        ColorChoice::Never,
    );
    let underline = rendered
        .lines()
        .find(|line| line.contains('^'))
        .expect("diagnostic underline");

    assert!(rendered.contains(&format!("{}:4:21", path.display())));
    assert!(rendered.contains("4 | \tlet value = \"åäö\"; missing."));
    assert!(
        underline.contains("| \t"),
        "underline must preserve the source tab:\n{rendered}"
    );
    assert!(underline.ends_with("^^^^^^^"));
}

/// Measures underline width in characters rather than UTF-8 bytes.
#[test]
fn col_utf8_parity_uses_character_width_for_multibyte_spans() {
    assert_eq!(
        caret_underline("\tåäö", "\tåäö", 2, 1, "\tåäö".len()),
        "\t^^^"
    );
}
