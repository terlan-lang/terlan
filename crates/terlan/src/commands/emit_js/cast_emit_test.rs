use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Verifies direct Oxc AST lowering treats checked casts as identity boundaries.
///
/// Inputs:
/// - A checked Terlan module with one assignment-compatible cast.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, preserves the `as` boundary
///   in CoreIR, then checks direct Oxc lowering emits the wrapped expression
///   without inventing JavaScript coercion.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_assignable_cast() {
    let source = "\
module js_core_direct_cast.

pub answer(): Int ->
    42 as Int.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_cast.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits assignment-compatible cast CoreIR");

    assert!(js.contains("export function answer()"));
    assert!(js.contains("return 42;"));
}

/// Verifies trait-backed casts do not lower as unchecked JavaScript identity.
///
/// Inputs:
/// - A checked Terlan module declaring a `Convertable[String, Int]`
///   implementation and using `text as Int`.
///
/// Output:
/// - Assertions that direct Oxc lowering refuses the cast and the fallback
///   emits a stub rather than `return text;`.
///
/// Transformation:
/// - Compiles an accepted conversion cast through the formal pipeline, then
///   confirms JS emission keeps the conversion boundary unsupported until an
///   explicit conversion-call lowering exists.
#[test]
fn emit_core_module_refuses_trait_backed_cast_identity_lowering() {
    let source = "\
module js_core_conversion_cast.

pub trait Convertable[From, To] {
    convert(value: From): To.
}.

pub impl Convertable[String, Int] for Int {
    convert(value: String): Int ->
        1.
}.

pub parse(text: String): Int ->
    text as Int.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_conversion_cast.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile trait-backed conversion cast to CoreIR");

    assert!(
        oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none(),
        "direct Oxc lowering must not erase conversion casts"
    );

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc codegen emits unsupported conversion stub");

    assert!(js.contains("throw new Error(\"Terlan JS backend stub\")"));
    assert!(!js.contains("return text;"));
}

/// Verifies generated-wrapper conversion casts stay explicit for JS emission.
///
/// Inputs:
/// - Three source fixtures shaped like future generated JS wrappers: string,
///   task, and collection conversions.
///
/// Output:
/// - Each fixture compiles through typechecking but direct JS lowering refuses
///   to erase the conversion as an identity expression.
///
/// Transformation:
/// - Exercises the L0.1 backend guard across representative wrapper families
///   without requiring the generated `std.js` library to exist yet.
#[test]
fn emit_core_module_refuses_js_wrapper_conversion_identity_lowering() {
    for (path, source) in [
        (
            "js_string_wrapper_cast.terl",
            "\
module js_string_wrapper_cast.

pub trait Convertable[From, To] {
    convert(value: From): To.
}.

pub struct JsString {
    value: String
}.

pub impl Convertable[String, JsString] for JsString {
    convert(value: String): JsString ->
        value as JsString.
}.

pub wrap(value: String): JsString ->
    value as JsString.
",
        ),
        (
            "js_task_wrapper_cast.terl",
            "\
module js_task_wrapper_cast.

pub trait Convertable[From, To] {
    convert(value: From): To.
}.

pub struct AppTask {
    id: Int
}.

pub struct JsPromise {
    id: Int
}.

pub impl Convertable[AppTask, JsPromise] for JsPromise {
    convert(value: AppTask): JsPromise ->
        value as JsPromise.
}.

pub wrap(value: AppTask): JsPromise ->
    value as JsPromise.
",
        ),
        (
            "js_collection_wrapper_cast.terl",
            "\
module js_collection_wrapper_cast.

pub trait Convertable[From, To] {
    convert(value: From): To.
}.

pub struct AppMap {
    size: Int
}.

pub struct JsMap {
    size: Int
}.

pub impl Convertable[AppMap, JsMap] for JsMap {
    convert(value: AppMap): JsMap ->
        value as JsMap.
}.

pub wrap(value: AppMap): JsMap ->
    value as JsMap.
",
        ),
    ] {
        let artifacts = compile_syntax_module_through_phases_with_profile(
            path,
            source,
            DiagnosticFormat::default(),
            None,
            NativePolicy::default(),
            TargetProfile::default(),
        )
        .unwrap_or_else(|_| panic!("compile JS wrapper conversion fixture {path}"));

        assert!(
            oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none(),
            "direct Oxc lowering must not erase wrapper conversion casts for {path}"
        );

        let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
            .unwrap_or_else(|err| panic!("fallback Oxc codegen for {path}: {err}"));
        assert!(
            js.contains("throw new Error(\"Terlan JS backend stub\")"),
            "wrapper conversion fixture should emit a stub for {path}: {js}"
        );
    }
}
