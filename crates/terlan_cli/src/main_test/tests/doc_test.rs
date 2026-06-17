use super::*;

/// Guards doctest validation against direct syntax-output Erlang lowering.
///
/// Inputs:
/// - The local `commands/doc/validation.rs` source file.
///
/// Output:
/// - Test success when doctest validation uses the CoreIR-gated backend entry
///   point and does not call the direct syntax-output Erlang emitter.
///
/// Transformation:
/// - Reads the doc validation source as text and checks the CoreIR
///   transition invariant for doctest compiler execution.
#[test]
fn doctest_validation_uses_core_ir_gated_erlang_lowering() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/doc/validation.rs"),
    )
    .expect("read doc validation source");

    assert!(
        source.contains("try_emit_core_module_to_erlang_with_syntax_bridge"),
        "doctest validation must use the CoreIR-gated Erlang backend"
    );
    assert!(
            !source.contains(
                "try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown"
            ),
            "doctest validation must not call direct syntax-output Erlang lowering"
        );
}

/// Verifies the documentation command emits Markdown from syntax-output
/// module documentation and public function comments.
///
/// Inputs:
/// - A temporary `.terl` source file with module documentation and one
///   documented public function.
///
/// Output:
/// - Test success when `terlc doc --format markdown` writes a Markdown file
///   containing the module title, module docs, function heading, and
///   signature.
///
/// Transformation:
/// - Runs the documentation command against a temporary source file and
///   inspects the generated Markdown artifact.
#[test]
fn formal_doc_markdown_generates_from_syntax_output() {
    let dir = make_temp_dir("formal_doc_markdown");
    let path = fixture(
            &dir,
            "//! Formal docs.\nmodule formal_docs.\n\n/// Adds one.\npub add(X: Int): Int ->\n    X + 1.\n",
        );
    let out_dir = dir.join("docs");

    let exit = commands::doc::run(
        CliCommand {
            verb: Some("doc".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            doc_format: DocFormat::Markdown,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let markdown = fs::read_to_string(out_dir.join("formal_docs.md")).expect("read docs");
    assert!(markdown.contains("# `formal_docs`"));
    assert!(markdown.contains("Formal docs."));
    assert!(markdown.contains("### `add/1`"));
    assert!(markdown.contains("pub add(X: Int): Int."));
}

/// Verifies Terlan code blocks in source documentation compile through the
/// syntax-output documentation path.
///
/// Inputs:
/// - A synthetic module source containing a fenced `terlan` documentation
///   example.
///
/// Output:
/// - Test success when the documentation compiler accepts the fenced example.
///
/// Transformation:
/// - Parses the source as syntax output and feeds the original source into the
///   documentation doctest compiler for syntax-output code blocks.
#[test]
fn formal_doctest_compiles_terlan_blocks_from_syntax_output() {
    let source = "module docs.\n\n/// Module example.\n///\n/// ```terlan\n/// module docs_example.\n///\n/// pub value(): Int ->\n///     1 + 0.\n/// ```\npub add(X: Int): Int ->\n    X + 1.\n";
    let syntax_output =
        parse_module_as_syntax_output(source).expect("syntax-output module should parse");

    commands::doc::compile_syntax_terlan_doctests(&syntax_output, source, "docs.terl")
        .expect("syntax-output doctest should compile");
}
