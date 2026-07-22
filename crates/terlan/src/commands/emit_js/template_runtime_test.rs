use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

fn fixture_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "terlan_js_template_{label}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ))
}

/// Proves that JS/browser template instantiation executes the shared typed
/// attribute matrix instead of returning an untyped props object.
#[test]
fn generated_js_template_runtime_renders_and_rejects_typed_slots() {
    let dir = fixture_dir("typed_slots");
    let templates = dir.join("templates");
    fs::create_dir_all(&templates).expect("create template fixture directory");
    fs::write(
        templates.join("card.terl.html"),
        "<a href={href} class={classes} disabled={disabled}>{title}:{count * 2}</a>",
    )
    .expect("write template fixture");
    let source_path = dir.join("app.terl");
    let source = r#"module app.

template Card from "./templates/card.terl.html" {
    title: String,
    href: String,
    classes: List[String],
    disabled: Bool,
    count: Int
}.

pub view(title: String, href: String, classes: List[String], disabled: Bool, count: Int): Html ->
    Card(title = title, href = href, classes = classes, disabled = disabled, count = count).
"#;
    fs::write(&source_path, source).expect("write Terlan fixture");
    let source_path_text = source_path.to_string_lossy();
    let compiled = compile_syntax_module_through_phases_with_profile(
        &source_path_text,
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsBrowser,
    )
    .expect("compile typed template fixture");
    let js = crate::commands::emit_js::emit_core_module_with_template_runtime(
        &compiled.core,
        &compiled.syntax_output,
        &source_path,
    )
    .expect("generate template runtime")
    .expect("direct JS template lowering");

    assert!(
        js.contains("function __terlan_template_Card(props)"),
        "{js}"
    );
    assert!(js.contains("\"url\""), "{js}");
    assert!(js.contains("\"tokens\""), "{js}");
    assert!(js.contains("\"boolean\""), "{js}");
    crate::commands::emit_js::validate_js_module_with_oxc(&js)
        .expect("Oxc accepts generated typed template module");

    let module_path = dir.join("app.mjs");
    fs::write(&module_path, js).expect("write JS module fixture");
    let runner_path = dir.join("runner.mjs");
    fs::write(
        &runner_path,
        r#"import { view } from "./app.mjs";
const rendered = view("Ada <Admin>", "/users?x=1&y=2", ["card", "active"], true, 7);
let unsafeUrl = null;
let invalidToken = null;
try { view("Ada", "javascript:alert(1)", ["card"], false, 7); }
catch (error) { unsafeUrl = error.message; }
try { view("Ada", "/users", ["two words"], false, 7); }
catch (error) { invalidToken = error.message; }
process.stdout.write(JSON.stringify({ rendered, unsafeUrl, invalidToken }));
"#,
    )
    .expect("write Node runner fixture");
    let output = match Command::new("node").arg(&runner_path).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run Node template fixture: {error}"),
    };
    assert!(
        output.status.success(),
        "Node template fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("decode Node result");
    assert_eq!(
        result["rendered"],
        "<a href=\"/users?x=1&amp;y=2\" class=\"card active\" disabled>Ada&#32;&lt;Admin&gt;:14</a>"
    );
    assert_eq!(
        result["unsafeUrl"],
        "template URL attribute `href` rejects an unsafe URL"
    );
    assert_eq!(
        result["invalidToken"],
        "template token-list attribute `class` has invalid token at index 0"
    );
}

/// Executes the same external template files used by the VM acceptance test
/// through both supported JavaScript render profiles.
#[test]
fn generated_js_template_runtime_matches_vm_shared_fixture_corpus() {
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("std/template/BackendJsFixture.terl");
    let source = r#"module std.template.BackendJsFixture.

import std.template.Template.
import type std.core.Option.Option.

struct User {
    id: Int,
    name: String
}.

template BackendPage from "./template-interpolation-backend-page.terl.html" {
    title: String,
    href: String,
    disabled: Bool,
    classes: List[String],
    tooltip: Option[String],
    count: Int,
    trusted: Template.Html,
    user: User
}.

template BackendBadge from "./template-interpolation-backend-badge.terl.html" {
    label: String
}.

pub render(title: String, href: String, disabled: Bool, classes: List[String], tooltip: Option[String], count: Int, trusted: Template.Html, user: User): Template.Html ->
    BackendPage(title = title, href = href, disabled = disabled, classes = classes, tooltip = tooltip, count = count, trusted = trusted, user = user).
"#;

    for profile in [TargetProfile::JsShared, TargetProfile::JsBrowser] {
        let compiled = compile_syntax_module_through_phases_with_profile(
            &source_path.to_string_lossy(),
            source,
            DiagnosticFormat::default(),
            None,
            NativePolicy::default(),
            profile,
        )
        .unwrap_or_else(|_| panic!("compile shared fixture for {}", profile.as_str()));
        let js = crate::commands::emit_js::emit_core_module_with_template_runtime(
            &compiled.core,
            &compiled.syntax_output,
            &source_path,
        )
        .expect("generate shared fixture runtime")
        .expect("direct shared fixture lowering");
        crate::commands::emit_js::validate_js_module_with_oxc(&js)
            .expect("Oxc accepts shared fixture module");

        let dir = fixture_dir(profile.as_str());
        fs::create_dir_all(&dir).expect("create shared JS fixture directory");
        fs::write(dir.join("fixture.mjs"), js).expect("write shared JS fixture module");
        fs::write(
            dir.join("runner.mjs"),
            r#"import { render } from "./fixture.mjs";
const user = { id: 7, name: "Ada & Bob" };
const some = render("<Terlan>", "/users/7?x=1&y=2", true, ["hero", "wide"], ["Some", "profile"], 7, ["Html", "<em>trusted</em>"], user);
const none = render("Terlan", "/users/7", false, ["hero"], ["None"], 3, ["Html", "<i>safe</i>"], { id: 7, name: "Ada" });
let unsafeUrl = null;
let unsafeHtml = null;
try { render("Terlan", "javascript:alert(1)", false, ["hero"], ["None"], 3, ["Html", "<i>safe</i>"], user); }
catch (error) { unsafeUrl = error.message; }
try { render("Terlan", "/users/7", false, ["hero"], ["None"], 3, "<i>unsafe</i>", user); }
catch (error) { unsafeHtml = error.message; }
process.stdout.write(JSON.stringify({ some, none, unsafeUrl, unsafeHtml }));
"#,
        )
        .expect("write shared JS fixture runner");
        let output = match Command::new("node").arg(dir.join("runner.mjs")).output() {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("run shared Node fixture: {error}"),
        };
        assert!(
            output.status.success(),
            "{} shared fixture failed: {}",
            profile.as_str(),
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("decode shared fixture output");
        assert_eq!(result["some"], "<main class=\"hero wide\" title=\"profile\" data-count=\"7\"><h1>&lt;Terlan&gt;</h1><a href=\"/users/7?x=1&amp;y=2\">Ada&#32;&amp;&#32;Bob</a><button disabled>14</button><em>trusted</em><strong>&lt;Terlan&gt;:<span>child</span></strong>\n</main>\n");
        assert_eq!(result["none"], "<main class=\"hero\" data-count=\"3\"><h1>Terlan</h1><a href=\"/users/7\">Ada</a><button>6</button><i>safe</i><strong>Terlan:<span>child</span></strong>\n</main>\n");
        assert_eq!(
            result["unsafeUrl"],
            "template URL attribute `href` rejects an unsafe URL"
        );
        assert_eq!(
            result["unsafeHtml"],
            "template trusted slot `trusted` requires Template.Html"
        );
    }
}
