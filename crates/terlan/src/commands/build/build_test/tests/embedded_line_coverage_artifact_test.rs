use super::*;
use object::{Object, ObjectSection};

/// Emits exact executable declaration ranges into VM source maps.
#[test]
fn embedded_line_coverage_parity_emits_checksum_covered_function_spans() {
    let dir = make_temp_dir("embedded_line_coverage_artifact");
    let source_path = dir.join("embedded_line_coverage.terl");
    let out_dir = dir.join("build");
    let source = concat!(
        "module embedded_line_coverage.\n\n",
        "prefix(): String -> \"alpha: α\".\n\n",
        "pub type Counter = Int.\n\n",
        "pub bump(value: Int, by: Int): Int -> value + by.\n\n",
        "pub add(left: Int, right: Int): Int ->\n",
        "    left + right.\n",
    );
    fs::write(&source_path, source).expect("write executable-line fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let bytes = fs::read(out_dir.join("vm/embedded_line_coverage.tvm"))
        .expect("read executable native image");
    let image = object::File::parse(&*bytes).expect("parse executable native image");
    let section_name = if cfg!(target_os = "windows") {
        ".tdbg"
    } else if cfg!(target_os = "macos") {
        "__terlan"
    } else {
        ".debug_terlan"
    };
    let records = crate::runtime::native_image::debug::decode_tvm_native_debug(
        image
            .section_by_name(section_name)
            .expect("native debug section")
            .data()
            .expect("native debug bytes"),
    )
    .expect("decode native debug section");

    assert_source_span(&records, source, "bump", 2, 7, "bump(value: Int");
    assert_source_span(&records, source, "add", 2, 9, "add(left: Int");
}

/// Checks one emitted range against UTF-8 source text and its one-based line.
fn assert_source_span(
    source_maps: &[crate::runtime::native_image::debug::TvmNativeDebugRecord],
    source: &str,
    function: &str,
    arity: u64,
    expected_line: usize,
    expected_prefix: &str,
) {
    let entry = source_maps
        .iter()
        .find(|entry| entry.function == function && entry.arity == arity as usize)
        .unwrap_or_else(|| panic!("function source map `{function}/{arity}` in {source_maps:?}"));
    let start = entry.span_start;
    let end = entry.span_end;
    let declaration = source.get(start..end).expect("UTF-8 declaration span");
    let line = source[..start].chars().filter(|ch| *ch == '\n').count() + 1;

    assert_eq!(line, expected_line);
    assert!(declaration.starts_with(expected_prefix), "{declaration:?}");
    assert!(source[end..].starts_with('.'), "{declaration:?}");
}
