use super::*;
use std::path::PathBuf;

const PUBLIC_MARKDOWN_ROOTS: &[&str] =
    &["README.md", "docs", "std", "editors", "tree-sitter-terlan"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct PublicTerlanModuleExample {
    path: PathBuf,
    line: usize,
    source: String,
}

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

/// Verifies the top-level README Terlan module example compiles.
///
/// Inputs:
/// - The repository `README.md` file.
///
/// Output:
/// - Test success when the first complete `terlan` fenced module example is
///   accepted by `terlc check`.
///
/// Transformation:
/// - Extracts a public documentation example, writes it as an isolated source
///   file, and runs the normal check command so stale README snippets cannot
///   drift away from the compiler.
#[test]
fn readme_hello_world_terlan_block_compiles() {
    let readme_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    let readme = fs::read_to_string(&readme_path).expect("read top-level README");
    let source = complete_terlan_fences(Path::new("README.md"), &readme)
        .into_iter()
        .map(|example| example.source)
        .next()
        .expect("README Terlan module example");
    let dir = make_temp_dir("readme_hello_world_terlan_block");
    let source_path = dir.join("Main.terl");
    fs::write(&source_path, source).expect("write README example source");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![source_path.to_string_lossy().into()],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies every complete public Markdown Terlan module example compiles.
///
/// Inputs:
/// - Public Markdown files under the repository README, docs, std, editor,
///   and tree-sitter surfaces.
///
/// Output:
/// - Test success when every fenced complete Terlan module is accepted by
///   `terlc check`.
///
/// Transformation:
/// - Promotes public examples from documentation text into compiler-owned
///   executable checks while leaving grammar fragments to the inventory gate.
#[test]
fn public_terlan_module_doc_blocks_compile() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let examples = public_terlan_module_examples(&repo_root);
    assert!(
        !examples.is_empty(),
        "expected at least one public Terlan module example"
    );

    let dir = make_temp_dir("public_terlan_module_doc_blocks");
    for (index, example) in examples.iter().enumerate() {
        let source_path = dir.join(format!("public_doc_example_{index}.terl"));
        fs::write(&source_path, &example.source).expect("write public doc example");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![source_path.to_string_lossy().into()],
            },
            CliState::default(),
        );

        assert_eq!(
            exit,
            ExitCode::SUCCESS,
            "{}:{} failed to compile",
            example.path.display(),
            example.line
        );
    }
}

/// Returns complete public Terlan module examples from Markdown files.
fn public_terlan_module_examples(repo_root: &Path) -> Vec<PublicTerlanModuleExample> {
    let mut examples = Vec::new();
    for relative in public_markdown_files(repo_root) {
        let text = fs::read_to_string(repo_root.join(&relative)).unwrap_or_else(|err| {
            panic!(
                "{}: failed to read public Markdown: {err}",
                relative.display()
            )
        });
        examples.extend(complete_terlan_fences(&relative, &text));
    }
    examples
}

/// Returns public Markdown files that may contain executable Terlan examples.
fn public_markdown_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in PUBLIC_MARKDOWN_ROOTS {
        let relative = Path::new(root);
        let full_path = repo_root.join(relative);
        if full_path.is_file() {
            if is_markdown_file(&full_path) {
                files.push(relative.to_path_buf());
            }
        } else if full_path.is_dir() {
            collect_public_markdown_files(repo_root, relative, &mut files);
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Recursively collects public Markdown files from one documentation root.
fn collect_public_markdown_files(repo_root: &Path, relative: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(repo_root.join(relative))
        .unwrap_or_else(|err| panic!("{}: failed to read docs dir: {err}", relative.display()))
    {
        let entry = entry.expect("read docs dir entry");
        let child = relative.join(entry.file_name());
        let child_full_path = repo_root.join(&child);
        if child_full_path.is_dir() {
            if should_skip_public_docs_dir(&child) {
                continue;
            }
            collect_public_markdown_files(repo_root, &child, files);
        } else if child_full_path.is_file() && is_markdown_file(&child_full_path) {
            files.push(child);
        }
    }
}

/// Returns complete Terlan fenced modules from Markdown text.
fn complete_terlan_fences(path: &Path, markdown: &str) -> Vec<PublicTerlanModuleExample> {
    let mut examples = Vec::new();
    let mut active_language = None::<String>;
    let mut start_line = 0_usize;
    let mut body = Vec::<String>::new();
    for (index, line) in markdown.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();
        if let Some(language) = active_language.as_ref() {
            if trimmed.starts_with("```") {
                let source = body.join("\n");
                if matches!(language.as_str(), "terlan" | "terl")
                    && source.trim_start().starts_with("module ")
                {
                    examples.push(PublicTerlanModuleExample {
                        path: path.to_path_buf(),
                        line: start_line,
                        source,
                    });
                }
                active_language = None;
                body.clear();
            } else {
                body.push(line.to_string());
            }
            continue;
        }

        if let Some(language) = trimmed.strip_prefix("```") {
            if !language.starts_with('`') {
                start_line = line_number;
                active_language = Some(
                    language
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
        }
    }
    examples
}

/// Returns whether a path is a Markdown file.
fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Returns whether a public documentation directory should be skipped.
fn should_skip_public_docs_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), "_build" | "target" | "node_modules")
    })
}
