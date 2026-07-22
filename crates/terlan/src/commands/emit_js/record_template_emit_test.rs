use std::fs;

use super::oxc_backend;
use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Verifies that direct Oxc AST lowering handles explicit record updates.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_record_update() {
    let source = "\
module js_core_direct_record_update.

pub struct Point {
    x: Int,
    y: Int
}.

pub set_x(point: Point): Point ->
    point#Point { x: 1, y: point.y }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_record_update.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits record-update CoreIR");

    assert!(js.contains("export function set_x(point)"));
    assert!(js.contains("...point"), "{js}");
    assert!(js.contains("x: 1"), "{js}");
    assert!(js.contains("y: point.y"), "{js}");
}

/// Verifies template instantiation dispatches to a generated typed renderer.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_template_instantiate() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "terlan_emit_js_template_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let template_dir = dir.join("templates");
    fs::create_dir_all(&template_dir).expect("create template dir");
    fs::write(template_dir.join("page.terl.html"), "<h1>{title}</h1>").expect("write template");
    let source_path = dir.join("js_core_direct_template_instantiate.terl");
    let source_path = source_path.to_string_lossy().to_string();
    let source = "\
module js_core_direct_template_instantiate.

template Page from \"./templates/page.terl.html\" {
    title: Binary
}.

pub view(title: Binary): Html ->
    Page(title = title).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        &source_path,
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits template-instantiation CoreIR");

    assert!(js.contains("export function view(title)"));
    assert!(
        js.contains("return __terlan_template_Page({ title });"),
        "{js}"
    );
}
