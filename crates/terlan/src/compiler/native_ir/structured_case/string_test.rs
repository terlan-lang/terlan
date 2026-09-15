use crate::compiler::native_ir::{
    emit_native_application_object,
    native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    },
    status, NativeModule,
};
use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{lower_syntax_module_output_to_core, CoreModule},
};

fn core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse string capture source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

#[test]
fn native_string_capture_case_head_let_and_lambda_execute_with_checked_guards() {
    let source = core(
        r#"
module native_string_capture.
route(value: String): Int.
route("GET /users/${id: Int}") where id > 0 -> id;
route(_) -> -1.
pub run(mode: Int): Int ->
    let text = if {
        mode == 0 -> "GET /users/7";
        mode == 1 -> "GET /users/-2";
        true -> "GET /users/not-int"
    };
    route(text).
pub conversions(): Bool ->
    case "1.5/true/hello" {
        "${value: Float}/${ok: Bool}/${word}" where ok -> value == 1.5 and word == "hello";
        _ -> false
    }.
pub path(): Bool ->
    let "assets/${bucket}/${file}.txt" = "assets/docs/readme.txt";
    bucket + "/" + file == "docs/readme".
pub render_path(): Bool ->
    let render = (("docs/${section}/${slug}.html") -> section + "/" + slug);
    render("docs/api/http.html") == "api/http".
pub failed_conversion(): Bool ->
    case "9223372036854775808" {
        "${value: Int}" -> false;
        _ -> true
    }.
"#,
    );
    let modules = NativeModule::lower_application(&[&source]).expect("lower string captures");
    let object =
        emit_native_application_object("string-captures", &modules).expect("emit captures");
    let mut invocations = Vec::new();
    for (name, arguments, expected) in [
        ("run", vec![0], 7),
        ("run", vec![1], -1),
        ("run", vec![2], -1),
        ("conversions", vec![], 1),
        ("path", vec![], 1),
        ("render_path", vec![], 1),
        ("failed_conversion", vec![], 1),
    ] {
        let export_id = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("capture export")
            .export_id;
        invocations.push(NativeObjectInvocation {
            export_id,
            arguments,
            expected_status: status::OK,
            expected_result: Some(expected),
        });
    }
    assert_managed_native_object_invocations("string-captures", &modules, &object, &invocations);
}

#[test]
fn string_capture_without_native_converter_has_a_stable_compile_error() {
    let source = core(
        r#"
module unsupported_capture.
pub check(value: String): Int ->
    case value { "${items: List[Int]}" -> 0; _ -> 1 }.
"#,
    );
    let error =
        NativeModule::lower_application(&[&source]).expect_err("reject unsupported conversion");
    assert!(
        error.contains("error[native_ir.string_pattern_conversion]"),
        "{error}"
    );
}
