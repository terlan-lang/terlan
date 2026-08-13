use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::{
    emit_structured_template_browser_renderer, render_structured_template,
    structured_template_telemetry, structured_template_telemetry_for_target,
};
use crate::terlan_html::ArtifactTemplateTarget;

fn temp_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "terlan_structured_template_{label}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ))
}

#[test]
fn structured_template_telemetry_classifies_every_non_html_target() {
    let fixtures = [
        ("data.terl.json", r#"{"title":"${title}"}"#, "json-string"),
        ("data.terl.yaml", "title: \"${title}\"\n", "yaml-string"),
        ("data.terl.yml", "title: ${title}\n", "yaml-value"),
        ("data.terl.toml", "title = \"${title}\"\n", "toml-string"),
        (
            "data.terl.xml",
            "<data title=\"${title}\"/>",
            "xml-attribute",
        ),
        ("data.terl.txt", "Hello ${title}", "text"),
    ];
    for (path, source, context) in fixtures {
        let telemetry = structured_template_telemetry(source, Path::new(path))
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        assert_eq!(telemetry.slots.len(), 1, "{path}");
        assert_eq!(telemetry.slots[0].expression, "title", "{path}");
        assert_eq!(telemetry.slots[0].context, context, "{path}");
        assert_eq!(telemetry.slots[0].line, 1, "{path}");
        assert!(telemetry.slots[0].column > 1, "{path}");
    }
}

#[test]
fn structured_template_rust_and_browser_renderers_are_isomorphic() {
    let fixtures = [
        (
            "backend.terl.json",
            r#"{"title":"${title}","count":${count},"features":${features}}"#,
            r#"{"title":"Ada & Bob","count":7,"features":["vm","js"]}"#,
        ),
        (
            "backend.terl.xml",
            r#"<card title="${title}">${title}</card>"#,
            r#"<card title="Ada &amp; Bob">Ada &amp; Bob</card>"#,
        ),
        (
            "backend.terl.yaml",
            "title: \"${title}\"\ncount: ${count}\n",
            "title: \"Ada & Bob\"\ncount: 7\n",
        ),
        (
            "backend.terl.toml",
            "title = \"${title}\"\ncount = ${count}\n",
            "title = \"Ada & Bob\"\ncount = 7\n",
        ),
        ("backend.terl.txt", "${title}: ${count}\n", "Ada & Bob: 7\n"),
    ];
    for (index, (path_text, source, expected)) in fixtures.into_iter().enumerate() {
        let path = Path::new(path_text);
        let mut values = BTreeMap::new();
        values.insert("title".to_string(), json!("Ada & Bob"));
        values.insert("count".to_string(), json!(7));
        values.insert("features".to_string(), json!(["vm", "js"]));
        let rust = render_structured_template(source, path, &values)
            .unwrap_or_else(|error| panic!("Rust renderer {path_text}: {error}"));
        assert_eq!(rust.output, expected, "{path_text}");

        let export = format!("render{index}");
        let js = emit_structured_template_browser_renderer(source, path, &export)
            .unwrap_or_else(|error| panic!("JS renderer {path_text}: {error}"));
        let dir = temp_dir(path_text);
        fs::create_dir_all(&dir).expect("create structured browser fixture");
        fs::write(dir.join("fixture.mjs"), js).expect("write structured browser module");
        fs::write(
            dir.join("runner.mjs"),
            format!(
                "import {{ {export} }} from './fixture.mjs';\nprocess.stdout.write(JSON.stringify({export}({{ title: 'Ada & Bob', count: 7, features: ['vm', 'js'] }})));\n"
            ),
        )
        .expect("write structured browser runner");
        let output = match Command::new("node").arg(dir.join("runner.mjs")).output() {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("run structured browser renderer: {error}"),
        };
        assert!(
            output.status.success(),
            "{path_text}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let browser: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("decode structured browser output");
        assert_eq!(browser["output"], rust.output, "{path_text}");
        assert_eq!(
            browser["telemetry"],
            serde_json::to_value(&rust.telemetry).unwrap()
        );
    }
}

#[test]
fn structured_template_backends_share_canonical_slot_failures() {
    let path = Path::new("backend.terl.json");
    let source = r#"{"title":"${title}"}"#;
    let mut values = BTreeMap::new();
    values.insert("title".to_string(), json!(["not", "text"]));
    let rust = render_structured_template(source, path, &values)
        .expect_err("Rust renderer rejects unsupported string-slot values");

    let js = emit_structured_template_browser_renderer(source, path, "render")
        .expect("emit rejecting browser renderer");
    let dir = temp_dir("rejection");
    fs::create_dir_all(&dir).expect("create rejection fixture");
    fs::write(dir.join("fixture.mjs"), js).expect("write rejection module");
    fs::write(
        dir.join("runner.mjs"),
        "import { render } from './fixture.mjs';\ntry { render({ title: ['not', 'text'] }); } catch (error) { process.stdout.write(error.message); }\n",
    )
    .expect("write rejection runner");
    let output = match Command::new("node").arg(dir.join("runner.mjs")).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run rejection renderer: {error}"),
    };
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), rust.context());
    assert!(rust
        .context()
        .starts_with("error[template_backend_slot_type]: backend.terl.json:1:"));
}

#[test]
fn structured_template_target_errors_include_interpolation_location() {
    let error = structured_template_telemetry("prefix\n${title}", Path::new("backend.data"))
        .expect_err("unknown target must fail");
    assert_eq!(
        error.domain(),
        terlan_runtime_abi::ErrorDomain::TemplateRendering
    );
    assert_eq!(error.code(), "template_target_unknown");
    assert_eq!(
        error.context(),
        "error[template_target_unknown]: backend.data:2:1: unsupported template target suffix"
    );
    let mismatch = structured_template_telemetry("<p>${title}</p>", Path::new("backend.terl.html"))
        .expect_err("HTML target must use the HTML renderer");
    assert_eq!(mismatch.code(), "template_target_mismatch");
    assert_eq!(
        mismatch.context(),
        "error[template_target_mismatch]: backend.terl.html:1:4: structured renderer cannot render html templates"
    );
    let inferred_mismatch = structured_template_telemetry_for_target(
        "prefix\n${title}",
        Path::new("backend.terl.json"),
        ArtifactTemplateTarget::Xml,
    )
    .expect_err("explicit target metadata must match the template suffix");
    assert_eq!(inferred_mismatch.code(), "template_target_mismatch");
    assert_eq!(
        inferred_mismatch.context(),
        "error[template_target_mismatch]: backend.terl.json:2:1: expected xml template, inferred json from suffix"
    );
}
