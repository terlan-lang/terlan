use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

#[test]
fn runtime_is_attached_only_when_referenced() {
    assert_eq!(
        attach("export const value = 1;".to_string()),
        "export const value = 1;"
    );

    let attached = attach("export const value = $terlan$equal([], []);".to_string());
    assert!(attached.starts_with("function $terlan$equal(left, right)"));
    assert_eq!(attached.matches("function $terlan$equal").count(), 1);
    assert!(attached.ends_with("export const value = $terlan$equal([], []);"));
}

#[test]
fn equality_operator_classes_are_complete() {
    for operator in ["==", "=:="] {
        assert!(is_equality(operator));
        assert!(!is_inequality(operator));
    }
    for operator in ["!=", "/=", "=/="] {
        assert!(is_equality(operator));
        assert!(is_inequality(operator));
    }
    for operator in ["<", ">", "and"] {
        assert!(!is_equality(operator));
        assert!(!is_inequality(operator));
    }
}

#[test]
fn emitted_equality_is_structural_for_scalars_products_lists_and_maps() {
    let source = r#"module js_structural_equality.

pub scalar_equal(): Bool ->
    7 == 7.

pub product_equal(): Bool ->
    {1, [2, 3]} == {1, [2, 3]}.

pub list_different(): Bool ->
    [{1, 2}] != [{1, 3}].

pub map_equal(): Bool ->
    {name: "Ada", values: [1, 2]} == {name: "Ada", values: [1, 2]}.

pub map_different(): Bool ->
    {name: "Ada", values: [1, 2]} != {name: "Ada", values: [1, 2, 3]}.

pub literal_case(): Bool ->
    case true {
        false -> false;
        true -> true
    }.
"#;
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_structural_equality.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile structural equality fixture");
    let javascript =
        super::super::direct_ast::emit_core_module_with_direct_oxc_ast(&artifacts.core)
            .expect("emit structural equality fixture");

    assert_eq!(javascript.matches("function $terlan$equal").count(), 1);
    assert!(javascript.contains("return $terlan$equal(7, 7);"));
    assert!(javascript.contains("return !$terlan$equal("));
    let script = format!(
        "{javascript}\nif (!scalar_equal() || !product_equal() || !list_different() || !map_equal() || !map_different() || !literal_case()) process.exit(1);"
    );
    let output = match std::process::Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("execute structural equality fixture: {error}"),
    };
    assert!(
        output.status.success(),
        "structural equality JavaScript failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn runtime_rejects_mixed_arrays_nulls_and_host_objects() {
    let script = format!(
        "{SOURCE}\n\
         if ($terlan$equal([], {{}})) process.exit(1);\n\
         if ($terlan$equal(null, {{}})) process.exit(2);\n\
         if ($terlan$equal({{}}, null)) process.exit(3);\n\
         if ($terlan$equal({{}}, new Date(0))) process.exit(4);\n\
         if ($terlan$equal(new Date(0), {{}})) process.exit(5);\n\
         if ($terlan$equal({{left: 1}}, {{right: 1}})) process.exit(6);\n\
         if (!$terlan$equal({{nested: [1, {{value: true}}]}}, {{nested: [1, {{value: true}}]}})) process.exit(7);"
    );
    let output = match std::process::Command::new("node")
        .args(["--input-type=commonjs", "--eval", &script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("execute equality runtime: {error}"),
    };
    assert!(
        output.status.success(),
        "equality runtime failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
