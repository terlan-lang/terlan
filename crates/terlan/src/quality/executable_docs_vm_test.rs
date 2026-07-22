use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies Terlan fenced blocks are extracted with source locations.
///
/// Inputs:
/// - Markdown containing one complete module example and one shell block.
///
/// Output:
/// - One Terlan documentation block with its opening fence line.
///
/// Transformation:
/// - Ignores non-Terlan fences while preserving the Terlan body verbatim.
#[test]
fn extracts_terlan_fenced_blocks_with_locations() {
    let markdown = "# Example\n\n```terlan\nmodule docs.Example.\n\npub value(): Int ->\n    1.\n```\n\n```sh\nterlc run .\n```\n";

    let blocks = extract_terlan_doc_blocks(Path::new("README.md"), markdown);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].path, PathBuf::from("README.md"));
    assert_eq!(blocks[0].line, 3);
    assert_eq!(blocks[0].language, "terlan");
    assert!(blocks[0].is_complete_module());
}

/// Verifies Markdown list indentation is not treated as Terlan source
/// indentation.
///
/// Inputs:
/// - A Terlan fenced block nested under a Markdown bullet.
///
/// Output:
/// - The extracted body starts at column one while preserving source-internal
///   indentation.
///
/// Transformation:
/// - Dedents fenced bodies before formatter checks so roadmap list examples can
///   remain canonical Terlan examples.
#[test]
fn extracts_indented_terlan_fences_without_markdown_indent() {
    let markdown = "- Example:\n  ```terlan\n  module docs.Example.\n\n  pub value(): Int ->\n      1.\n  ```\n";

    let blocks = extract_terlan_doc_blocks(Path::new("docs/roadmap.md"), markdown);

    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].body.starts_with("module docs.Example."));
    assert!(blocks[0].body.contains("pub value(): Int ->\n    1."));
}

/// Verifies grammar fragments are inventoried without being promoted to full
/// module examples.
///
/// Inputs:
/// - Markdown containing a Terlan expression fragment.
///
/// Output:
/// - One block classified as a fragment.
///
/// Transformation:
/// - Keeps language-design snippets visible to the gate while avoiding false
///   compiler checks for intentionally incomplete examples.
#[test]
fn classifies_non_module_blocks_as_fragments() {
    let markdown = "```terlan\n{ name: \"Ada\" }\n```\n";

    let blocks = extract_terlan_doc_blocks(Path::new("docs/grammar/README.md"), markdown);

    assert_eq!(blocks.len(), 1);
    assert!(!blocks[0].is_complete_module());
    assert!(!blocks[0].is_project_level_module());
}

/// Verifies the active 0.0.7 roadmap is part of executable-doc inventory.
///
/// Inputs:
/// - A synthetic checkout root with README/changelog docs plus sibling
///   roadmap and release-notes files.
///
/// Output:
/// - Markdown inventory includes active release docs and the sibling roadmap
///   paths.
///
/// Transformation:
/// - Keeps the active roadmap covered without sweeping historical baseline
///   roadmaps into the executable-doc gate.
#[test]
fn collect_markdown_files_includes_active_roadmap() {
    let workspace = unique_temp_root("terlan_executable_docs_vm_roadmap_workspace");
    let root = workspace.join("terlan");
    fs::create_dir_all(root.join("docs")).expect("create repo docs");
    fs::create_dir_all(workspace.join("docs/roadmap")).expect("create roadmap docs");
    fs::write(root.join("README.md"), "# Readme\n").expect("write readme");
    fs::write(root.join("CHANGELOG.md"), "# Changelog\n").expect("write changelog");
    fs::write(
        workspace.join("docs/roadmap/ROADMAP_0_0_7.md"),
        "# Roadmap\n",
    )
    .expect("write roadmap");
    fs::write(
        workspace.join("docs/roadmap/RELEASE_NOTES_0_0_7.md"),
        "# Release notes\n",
    )
    .expect("write release notes");

    let files = collect_markdown_files(&root).expect("collect markdown files");

    assert!(files.contains(&PathBuf::from("README.md")));
    assert!(files.contains(&PathBuf::from("CHANGELOG.md")));
    assert!(files.contains(&PathBuf::from("../docs/roadmap/ROADMAP_0_0_7.md")));
    assert!(files.contains(&PathBuf::from("../docs/roadmap/RELEASE_NOTES_0_0_7.md")));
}

/// Verifies browser and project examples are classified separately from pure
/// compiler/runtime examples.
///
/// Inputs:
/// - Complete module examples covering pure code plus CSS, JS, HTTP, and
///   template runtime dependencies.
///
/// Output:
/// - Only dependency-bearing modules are marked as project-level.
///
/// Transformation:
/// - Keeps examples that need asset pipelines, browser ambient APIs, HTTP
///   runtime wiring, or template runtime support out of pure release gates
///   unless those dependencies are adopted explicitly.
#[test]
fn classifies_project_level_complete_modules() {
    let pure = TerlanDocBlock {
        path: PathBuf::from("README.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Pure.\n\npub value(): Int ->\n    1.\n".to_string(),
    };
    let css_asset = TerlanDocBlock {
        path: PathBuf::from("docs/static.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Static.\n\nimport css \"./app.css\" as AppCss.\n".to_string(),
    };
    let browser = TerlanDocBlock {
        path: PathBuf::from("docs/browser.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Browser.\n\nimport std.js.Dom.Window.\n".to_string(),
    };
    let http = TerlanDocBlock {
        path: PathBuf::from("docs/http.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Http.\n\nimport std.http.Request.\n".to_string(),
    };
    let template = TerlanDocBlock {
        path: PathBuf::from("docs/template.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Template.\n\nimport std.template.Template.\n".to_string(),
    };

    assert!(!pure.is_project_level_module());
    assert!(css_asset.is_project_level_module());
    assert!(browser.is_project_level_module());
    assert!(http.is_project_level_module());
    assert!(template.is_project_level_module());
}

/// Verifies stale VM-pivot terms are rejected inside Terlan examples.
///
/// Inputs:
/// - A Terlan block containing removed runtime syntax.
///
/// Output:
/// - Diagnostics naming the stale term.
///
/// Transformation:
/// - Prevents examples from reintroducing BEAM/OTP-era source contracts.
#[test]
fn rejects_stale_runtime_terms_in_terlan_examples() {
    let blocks = vec![TerlanDocBlock {
        path: PathBuf::from("README.md"),
        line: 10,
        language: "terlan".to_string(),
        body: "import std.beam.Agent.\n\npub value(): Unit ->\n    Unit().\n".to_string(),
    }];

    let diagnostics = validate_terlan_doc_blocks(&blocks);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code[docs_codeblock.stale_term]")),
        "expected stale-term diagnostic code: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("std.beam")),
        "expected std.beam diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Unit()")),
        "expected Unit() diagnostic: {diagnostics:?}"
    );
}

/// Verifies the retired `.tl` fence spelling is rejected.
///
/// Inputs:
/// - A Terlan block extracted from an old `tl` fence.
///
/// Output:
/// - One diagnostic requiring the current fence spelling.
///
/// Transformation:
/// - Keeps documentation aligned with the `.terl` source extension pivot.
#[test]
fn rejects_stale_tl_fence_language() {
    let blocks = vec![TerlanDocBlock {
        path: PathBuf::from("docs/guide.md"),
        line: 4,
        language: "tl".to_string(),
        body: "module docs.Guide.\n".to_string(),
    }];

    let diagnostics = validate_terlan_doc_blocks(&blocks);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code[docs_codeblock.stale_fence_language]")),
        "expected tl fence diagnostic code: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("fence language `tl`")),
        "expected tl fence diagnostic: {diagnostics:?}"
    );
}

/// Verifies removed target-profile spellings are rejected with a dedicated
/// diagnostic code.
///
/// Inputs:
/// - A Terlan-fenced documentation block containing an old `core-v0` target
///   profile command.
///
/// Output:
/// - A stable unsupported-target-profile diagnostic and intentionally-stale
///   block classification.
///
/// Transformation:
/// - Prevents executable docs from silently preserving old target-profile
///   command forms after the VM-default pivot.
#[test]
fn rejects_unsupported_target_profile_examples() {
    let block = TerlanDocBlock {
        path: PathBuf::from("docs/targets.md"),
        line: 12,
        language: "terlan".to_string(),
        body: "terlc build --target-profile core-v0\n".to_string(),
    };
    let diagnostics = validate_terlan_doc_blocks(std::slice::from_ref(&block));

    assert!(
        diagnostics.iter().any(
            |diagnostic| diagnostic.contains("code[docs_codeblock.unsupported_target_profile]")
        ),
        "expected unsupported target-profile diagnostic code: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("--target-profile core-v0")),
        "expected unsupported target-profile spelling: {diagnostics:?}"
    );
    assert_eq!(block_classification(&block), "intentionally_stale");
}

/// Verifies retired language syntax is rejected explicitly.
///
/// Inputs:
/// - A Terlan documentation block using older constructor and Option matching
///   forms.
///
/// Output:
/// - A stable old-syntax diagnostic and intentionally-stale block
///   classification.
///
/// Transformation:
/// - Prevents public examples from drifting back to parser shapes that were
///   removed during the VM/default syntax pivot.
#[test]
fn rejects_old_syntax_forms_in_examples() {
    let block = TerlanDocBlock {
        path: PathBuf::from("docs/vector.md"),
        line: 21,
        language: "terlan".to_string(),
        body: "module docs.Vector.\n\npub value(): Unit ->\n    let users = Vector.new[String]();\n    users.first().match {\n        None -> Unit()\n    }.\n".to_string(),
    };
    let diagnostics = validate_terlan_doc_blocks(std::slice::from_ref(&block));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code[docs_codeblock.old_syntax_form]")),
        "expected old-syntax diagnostic code: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Vector.new[")),
        "expected retired Vector.new syntax: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains(".match {")),
        "expected retired .match syntax: {diagnostics:?}"
    );
    assert_eq!(block_classification(&block), "intentionally_stale");
}

/// Verifies complete examples cannot rely on hidden imports.
///
/// Inputs:
/// - A complete module that calls `println` without importing or qualifying it.
///
/// Output:
/// - A stable hidden-import diagnostic and intentionally-stale block
///   classification.
///
/// Transformation:
/// - Prevents public examples from passing only because surrounding prose or
///   previous snippets introduced imports that are absent from the fenced block.
#[test]
fn rejects_hidden_imports_in_complete_examples() {
    let block = TerlanDocBlock {
        path: PathBuf::from("docs/console.md"),
        line: 3,
        language: "terlan".to_string(),
        body: "module docs.Console.\n\npub main(): Unit ->\n    println(\"hello\").\n".to_string(),
    };
    let diagnostics = validate_terlan_doc_blocks(std::slice::from_ref(&block));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code[docs_codeblock.hidden_import]")),
        "expected hidden-import diagnostic code: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("println(")),
        "expected println call in diagnostic: {diagnostics:?}"
    );
    assert_eq!(block_classification(&block), "intentionally_stale");
}

/// Verifies fully qualified calls do not require extra imports.
///
/// Inputs:
/// - A complete module that calls `std.io.Console.println` directly.
///
/// Output:
/// - No hidden-import diagnostic.
///
/// Transformation:
/// - Allows self-contained docs examples to avoid local imports when they use
///   fully qualified names.
#[test]
fn accepts_fully_qualified_calls_without_imports() {
    let block = TerlanDocBlock {
        path: PathBuf::from("docs/console.md"),
        line: 3,
        language: "terlan".to_string(),
        body:
            "module docs.Console.\n\npub main(): Unit ->\n    std.io.Console.println(\"hello\").\n"
                .to_string(),
    };
    let diagnostics = validate_terlan_doc_blocks(std::slice::from_ref(&block));

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code[docs_codeblock.hidden_import]")),
        "qualified call should not need an import: {diagnostics:?}"
    );
}

/// Verifies stale install snippets are rejected with a dedicated diagnostic.
///
/// Inputs:
/// - Shell-fenced documentation containing crate-root install commands that do
///   not match the current package layout or release installer.
///
/// Output:
/// - A stable misleading-install-command diagnostic.
///
/// Transformation:
/// - Keeps public setup instructions aligned with the release artifact and
///   `crates/terlan` local-install contract.
#[test]
fn rejects_misleading_install_commands() {
    let markdown = "```sh\ncargo install terlan\ncargo install --path .\n```\n";

    let diagnostics = validate_install_command_blocks(Path::new("README.md"), markdown);

    assert!(
        diagnostics.iter().any(
            |diagnostic| diagnostic.contains("code[docs_codeblock.misleading_install_command]")
        ),
        "expected misleading install diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("cargo install terlan")),
        "expected published-crate install warning: {diagnostics:?}"
    );
}

/// Verifies current install snippets are accepted.
///
/// Inputs:
/// - Shell and PowerShell fences matching the current README installer forms.
///
/// Output:
/// - No misleading-install-command diagnostics.
///
/// Transformation:
/// - Prevents the install-command adversarial gate from rejecting the intended
///   platform artifact and local checkout flows.
#[test]
fn accepts_current_install_commands() {
    let markdown = "```sh\ncurl -fsSL https://raw.githubusercontent.com/terlan-lang/terlan/main/install.sh | sh\ncurl -fsSL https://raw.githubusercontent.com/terlan-lang/terlan/main/install.sh | env TERLAN_VERSION=v0.0.7 sh\ncargo install --path crates/terlan --bin terlc --force\n```\n\n```powershell\niwr https://raw.githubusercontent.com/terlan-lang/terlan/main/install.ps1 -UseBasicParsing | iex\n```\n";

    let diagnostics = validate_install_command_blocks(Path::new("README.md"), markdown);

    assert!(
        diagnostics.is_empty(),
        "current install commands should be accepted: {diagnostics:?}"
    );
}

/// Verifies report classifications use the roadmap-owned vocabulary.
///
/// Inputs:
/// - Complete executable, project-level compile-only, diagnostic-only,
///   fragmentary illustrative, and stale documentation blocks.
///
/// Output:
/// - Classifications are limited to the roadmap categories.
///
/// Transformation:
/// - Prevents temporary implementation status labels from leaking into release
///   artifacts consumed by docs gates.
#[test]
fn report_classification_uses_roadmap_vocabulary() {
    let executable = TerlanDocBlock {
        path: PathBuf::from("README.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Example.\n\npub value(): Int ->\n    1.\n".to_string(),
    };
    let compile_only = TerlanDocBlock {
        path: PathBuf::from("docs/http.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Http.\n\nimport std.http.Request.\n".to_string(),
    };
    let illustrative = TerlanDocBlock {
        path: PathBuf::from("docs/grammar.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "let value = 1.\n".to_string(),
    };
    let diagnostic_only = TerlanDocBlock {
        path: PathBuf::from("docs/diagnostic.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "error[type_error]: diagnostic\n  --> docs/Main.terl:1:1\n".to_string(),
    };
    let stale = TerlanDocBlock {
        path: PathBuf::from("docs/old.md"),
        line: 1,
        language: "tl".to_string(),
        body: "module docs.Old.\n\npub value(): Unit ->\n    Unit().\n".to_string(),
    };

    assert_eq!(block_classification(&executable), "executable");
    assert_eq!(block_classification(&compile_only), "compile_only");
    assert_eq!(block_classification(&diagnostic_only), "diagnostic_only");
    assert_eq!(block_classification(&illustrative), "illustrative");
    assert_eq!(block_classification(&stale), "intentionally_stale");
}

/// Verifies executable-docs report vocabulary cannot use placeholder labels.
///
/// Inputs:
/// - Current report vocabulary plus an injected placeholder reason.
///
/// Output:
/// - Current vocabulary is clean and the injected placeholder is rejected.
///
/// Transformation:
/// - Prevents temporary TODO/TBD wording from leaking into release documentation
///   report statuses, classifications, skip reasons, or policy labels.
#[test]
fn rejects_placeholder_terms_in_report_vocabulary() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "executable docs report vocabulary must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("documentation skip reasons", &["todo docs reason"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}

/// Verifies complete module examples report formatter status.
///
/// Inputs:
/// - One canonical module and one module that changes under `terlc fmt`.
///
/// Output:
/// - Canonical examples receive formatter success, while non-canonical examples
///   receive a stable diagnostic code.
///
/// Transformation:
/// - Keeps documentation examples under the same formatter contract as source
///   files without applying formatting inside the quality gate.
#[test]
fn validates_complete_module_examples_are_fmt_canonical() {
    let canonical = TerlanDocBlock {
        path: PathBuf::from("README.md"),
        line: 1,
        language: "terlan".to_string(),
        body: "module docs.Ok.\n\npub value(): Int -> 1.\n".to_string(),
    };
    let unformatted = TerlanDocBlock {
        path: PathBuf::from("docs/bad.md"),
        line: 7,
        language: "terlan".to_string(),
        body: "module docs.Bad.\npub value(): Int -> 1.\n".to_string(),
    };

    assert_eq!(
        formatter_result_for_block(&canonical)
            .expect("canonical formatter result")
            .status,
        "canonical"
    );
    let diagnostics = validate_terlan_doc_blocks(&[unformatted]);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code[docs_codeblock.unformatted_example]")),
        "expected unformatted diagnostic code: {diagnostics:?}"
    );
}

/// Verifies the gate persists the machine-readable code-block report.
///
/// Inputs:
/// - A synthetic repository with one executable module, one project-level HTTP
///   module, and one diagnostic-only block.
///
/// Output:
/// - `target/quality/docs-codeblock-executable-report.json` containing
///   inventory counts, classifications, and skipped-example reasons.
///
/// Transformation:
/// - Runs the public quality gate so release tooling can rely on the same
///   artifact rather than a test-only helper.
#[test]
fn run_executable_docs_vm_writes_report_artifact() {
    let workspace = unique_temp_root("terlan_executable_docs_vm_report_workspace");
    let root = workspace.join("terlan");
    fs::create_dir_all(&root).expect("create repo root");
    fs::create_dir_all(root.join("docs")).expect("create docs");
    fs::write(
        root.join("README.md"),
        "```terlan\nmodule docs.Readme.\n\npub value(): Int -> 1.\n```\n",
    )
    .expect("write readme");
    fs::write(
        root.join("docs/http.md"),
        "```terlan\nmodule docs.Http.\n\nimport std.http.Request.\n\npub value(): Int -> 1.\n```\n",
    )
    .expect("write docs");
    fs::write(
        root.join("docs/diagnostic.md"),
        "```terlan\nerror[type_error]: diagnostic\n  --> docs/Main.terl:1:1\n```\n",
    )
    .expect("write diagnostic docs");

    let summary = run_executable_docs_vm(&root).expect("gate succeeds");

    assert_eq!(summary.markdown_file_count, 3);
    assert_eq!(summary.terlan_block_count, 3);
    assert_eq!(summary.complete_module_count, 2);
    assert_eq!(summary.project_level_module_count, 1);
    assert!(summary.report_path.ends_with(REPORT_PATH));

    let report_text = fs::read_to_string(&summary.report_path).expect("read report");
    let report: serde_json::Value = serde_json::from_str(&report_text).expect("parse report");
    assert_eq!(report["markdown_file_count"], 3);
    assert_eq!(report["terlan_block_count"], 3);
    assert_eq!(report["complete_module_count"], 2);
    assert_eq!(report["project_level_module_count"], 1);
    let classifications = report["blocks"]
        .as_array()
        .expect("blocks array")
        .iter()
        .map(|block| block["classification"].as_str().expect("classification"))
        .collect::<Vec<_>>();
    assert!(classifications.contains(&"executable"));
    assert!(classifications.contains(&"compile_only"));
    assert!(classifications.contains(&"diagnostic_only"));
    assert_eq!(report["executed_examples"][0]["path"], "README.md");
    assert_eq!(
        report["executed_examples"][0]["status"],
        "compile_smoke_passed"
    );
    assert!(!report["formatter_results"]
        .as_array()
        .expect("formatter results")
        .is_empty());
    let skipped_codes = report["skipped_examples"]
        .as_array()
        .expect("skipped examples")
        .iter()
        .map(|entry| entry["code"].as_str().expect("skip code"))
        .collect::<Vec<_>>();
    assert!(skipped_codes.contains(&"docs_codeblock.missing_manifest_context"));
    assert!(skipped_codes.contains(&"docs_codeblock.diagnostic_only_example"));
}

/// Verifies project-level examples make missing manifest context explicit.
///
/// Inputs:
/// - A complete module that imports a project asset, which cannot be compiled
///   as a standalone public documentation snippet.
///
/// Output:
/// - The persisted report classifies the snippet as compile-only and records a
///   stable missing-manifest skip code.
///
/// Transformation:
/// - Keeps package/browser examples visible without letting hidden manifest or
///   asset context masquerade as an executable docs example.
#[test]
fn project_level_examples_report_missing_manifest_context() {
    let root = unique_temp_root("terlan_executable_docs_vm_manifest_context");
    fs::write(
        root.join("README.md"),
        "```terlan\nmodule docs.Readme.\n\npub value(): Int -> 1.\n```\n",
    )
    .expect("write readme");
    fs::create_dir_all(root.join("docs")).expect("create docs");
    fs::write(
        root.join("docs/assets.md"),
        "```terlan\nmodule docs.Assets.\n\nimport file \"./logo.txt\" as Logo.\n\npub value(): Int -> 1.\n```\n",
    )
    .expect("write docs");

    run_executable_docs_vm(&root).expect("gate succeeds");

    let report_text =
        fs::read_to_string(root.join(REPORT_PATH)).expect("read executable docs report");
    let report: serde_json::Value = serde_json::from_str(&report_text).expect("parse report");
    let skipped = report["skipped_examples"]
        .as_array()
        .expect("skipped examples");
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["path"], "docs/assets.md");
    assert_eq!(
        skipped[0]["code"],
        "docs_codeblock.missing_manifest_context"
    );
    assert_eq!(report["blocks"][1]["classification"], "compile_only");
}

/// Verifies failure reports preserve stable diagnostic assertion codes.
///
/// Inputs:
/// - A synthetic repository containing an intentionally stale Terlan fence.
///
/// Output:
/// - The gate fails and still writes a report with diagnostic assertion codes
///   and stale-example reasons.
///
/// Transformation:
/// - Keeps diagnostic-only docs examples auditable even when the quality gate
///   rejects the stale source.
#[test]
fn run_executable_docs_vm_writes_diagnostic_codes_on_failure() {
    let root = unique_temp_root("terlan_executable_docs_vm_diagnostics");
    fs::write(
        root.join("README.md"),
        "```tl\nmodule docs.Old.\n\npub value(): Unit ->\n    Unit().\n```\n",
    )
    .expect("write readme");

    let result = run_executable_docs_vm(&root);

    assert!(result.is_err(), "stale docs must fail the gate");
    let report_path = root.join(REPORT_PATH);
    let report_text = fs::read_to_string(&report_path).expect("read report");
    let report: serde_json::Value = serde_json::from_str(&report_text).expect("parse report");
    let codes = report["diagnostic_assertions"]
        .as_array()
        .expect("diagnostic assertions")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("diagnostic code"))
        .collect::<Vec<_>>();
    assert!(codes.contains(&"docs_codeblock.stale_fence_language"));
    assert!(codes.contains(&"docs_codeblock.stale_term"));
    let first_diagnostic = &report["diagnostic_assertions"][0];
    assert_eq!(first_diagnostic["path"], "README.md");
    assert_eq!(first_diagnostic["span"]["start_line"], 1);
    assert_eq!(first_diagnostic["span"]["start_column"], 1);
    assert_eq!(first_diagnostic["span"]["end_line"], 1);
    assert_eq!(first_diagnostic["span"]["end_column"], 1);
    assert_eq!(
        first_diagnostic["message_text_policy"],
        "stable_code_with_message_substring"
    );
    assert_eq!(
        first_diagnostic["json_shape"],
        "code_path_line_span_policy_diagnostic"
    );
    assert_eq!(
        first_diagnostic["redaction_policy"],
        "repository_relative_paths_only"
    );
    assert_eq!(report["blocks"][0]["classification"], "intentionally_stale");
    assert!(!report["stale_example_reasons"]
        .as_array()
        .expect("stale reasons")
        .is_empty());
}

fn unique_temp_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moves forward")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}_{nanos}"));
    fs::create_dir_all(&path).expect("create temp root");
    path
}
