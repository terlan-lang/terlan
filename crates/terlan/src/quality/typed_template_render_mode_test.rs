use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_typed_template_render_mode, validate_performance_budget_terms};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-typed-template-render-mode-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/src/html/structured.rs",
            r#"
validate_artifact_template_structure ArtifactTemplateTarget::Html
ArtifactTemplateTarget::Markdown ArtifactTemplateTarget::Json
ArtifactTemplateTarget::Toml ArtifactTemplateTarget::Yaml
ArtifactTemplateTarget::Text ArtifactTemplateTarget::Xml
validate_xml_template_structure
"#,
        )?;
        self.write(
            "crates/terlan/src/html/escaping.rs",
            "escape_html_attr escape_html_text ammonia::clean_text",
        )?;
        self.write(
            "crates/terlan/src/validation/template_contract/template_contract_test.rs",
            r#"
Template.Html template_slot_typecheck_rejects_html_fragment_in_attribute_context
template_slot_typecheck_accepts_scalar_struct_field_in_text_context
template_component_prop_accepts_expression_slot_matching_expected_type
"#,
        )?;
        self.write(
            "std/template/Template.terl",
            "pub opaque type Html pub trusted(value: String): Html",
        )?;
        self.write(
            "std/http/Response.terl",
            "pub html(value: std.template.Template.Html",
        )?;
        self.write(
            "crates/terlan/src/compiler/typeck/std_contract_test.rs",
            "Response.html(page())",
        )?;
        self.write(
            "crates/terlan/src/compiler/syntax/formatter/metadata.rs",
            "format_template_decl",
        )?;
        self.write(
            "crates/terlan/src/commands/doc/README.md",
            "Render Mode Parity staticHtml documentationExample structuredArtifact",
        )?;
        self.write(
            "editors/vscode/src/template_links.js",
            "templateRenderModeFromPath documentationExample structuredArtifact",
        )?;
        self.write(
            "editors/vscode/test/template_links_test.js",
            "testTemplateRenderModeFromPath templateRenderModeFromPath(\"templates/readme.terl.md\") renderMode",
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
typed-template-render-mode-check: vm-live-template-client-protocol-check typed-template-interpolation-check
	node editors/vscode/test/template_links_test.js
	$(RUST_TEST) -p terlan --bin terlan-quality typed_template_render_mode_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- typed-template-render-mode
"#;

#[test]
fn typed_template_render_mode_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_typed_template_render_mode(repo.root()).expect("quality check");

    assert_eq!(summary.render_mode_count, 8);
    assert_eq!(summary.implemented_mode_count, 4);
    assert_eq!(summary.escaping_check_count, 4);
    assert_eq!(summary.rejected_mode_combination_count, 8);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-typed-template-render-mode-report-v1"));
    assert!(report.contains("staticHtml"));
    assert!(report.contains("docs/editor render-mode parity"));
    assert!(report.contains("streaming fragment budgets remain rejected"));
    assert!(report.contains("hydration mismatch"));
    assert!(report.contains("staticHtml.maxRenderMs=5"));
    assert!(report.contains("serverRenderedHtml.maxDescriptorBytes=4096"));
    assert!(
        !report.to_ascii_lowercase().contains("placeholder"),
        "report must not carry placeholder budget evidence: {report}"
    );
}

#[test]
fn typed_template_render_mode_rejects_placeholder_budget_terms() {
    let diagnostics = validate_performance_budget_terms(&["static HTML render budget placeholder"]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder language")),
        "expected placeholder budget diagnostic: {diagnostics:?}"
    );
}

#[test]
fn typed_template_render_mode_rejects_missing_structure_anchor() {
    let repo = TestRepo::new("missing-structure-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/html/structured.rs");
    let source = fs::read_to_string(&path).expect("structure source");
    repo.write(
        "crates/terlan/src/html/structured.rs",
        &source.replace("ArtifactTemplateTarget::Xml", ""),
    )
    .expect("rewrite structure source");

    let error = run_typed_template_render_mode(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("ArtifactTemplateTarget::Xml"));
}

#[test]
fn typed_template_render_mode_rejects_missing_escaping_anchor() {
    let repo = TestRepo::new("missing-escaping-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "crates/terlan/src/html/escaping.rs",
        "escape_html_attr escape_html_text",
    )
    .expect("rewrite escaping source");

    let error = run_typed_template_render_mode(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("ammonia::clean_text"));
}

#[test]
fn typed_template_render_mode_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace(" typed-template-interpolation-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_typed_template_render_mode(repo.root()).expect_err("gate should fail");

    assert!(error.contains("typed-template-interpolation-check"));
}

#[test]
fn typed_template_render_mode_rejects_missing_editor_render_mode_anchor() {
    let repo = TestRepo::new("missing-editor-render-mode-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "editors/vscode/src/template_links.js",
        "templateRenderModeFromPath structuredArtifact",
    )
    .expect("rewrite editor source");

    let error = run_typed_template_render_mode(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("documentationExample"));
}
