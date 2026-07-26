//! Direct-AOT replacements for portable `native_record_SUITE` contracts.

use std::collections::{HashMap, HashSet};

use crate::runtime::native_image::managed::decode_aggregate_layout;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status, NativeModule};

fn native_record_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let big_fields = (1..=64)
        .map(|index| format!("f{index}: Int"))
        .collect::<Vec<_>>()
        .join(", ");
    let big_values = (1..=64)
        .map(|index| format!("f{index}: {index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let big_parameters = (1..=64)
        .map(|index| format!("f{index}: Int"))
        .collect::<Vec<_>>()
        .join(", ");
    let big_assignments = (1..=64)
        .map(|index| format!("f{index}: f{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "\
module native_record_suite_native.\n\
\n\
pub struct Pair {{ left: Int, right: Int }}.\n\
pub struct Big {{ {big_fields} }}.\n\
\n\
pub constructor Pair {{\n\
    (left: Int, right: Int): Pair -> Pair {{ left: left, right: right }}\n\
}}.\n\
\n\
pub constructor Big {{\n\
    ({big_parameters}): Big -> Big {{ {big_assignments} }}\n\
}}.\n\
\n\
churn(value: Pair, remaining: Int): Pair ->\n\
    if {{\n\
        remaining == 0 -> value;\n\
        true -> churn(value#Pair {{ left: value.left + 1, right: value.right - 1 }}, remaining - 1)\n\
    }}.\n\
\n\
big_sum(value: Big): Int -> value.f1 + value.f64.\n\
\n\
pub create_update_access(): Int ->\n\
    let original = Pair {{ right: 2, left: 1 }};\n\
    let updated = original#Pair {{ left: 40 }};\n\
    original.left * 100 + updated.left + updated.right.\n\
\n\
pub pattern_contract(): Int ->\n\
    case Pair {{ left: 20, right: 22 }} {{\n\
        Pair {{ left: found, right: _ }} -> found + 22;\n\
        _ -> 0\n\
    }}.\n\
\n\
pub churn_contract(): Int ->\n\
    let result = churn(Pair {{ left: 0, right: 10000 }}, 10000);\n\
    result.left + result.right.\n\
\n\
pub big_contract(): Int -> big_sum(Big {{ {big_values} }}).\n"
    );
    let syntax = parse_module_as_syntax_output(&source).expect("parse native-record source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower native-record application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn native_record_suite_constructs_updates_patterns_and_churns_in_aot_code() {
    let (modules, exports) = native_record_module();
    let object =
        emit_native_application_object("native_record_suite_native", &modules).expect("object");
    let invocations = [
        ("create_update_access", 142),
        ("pattern_contract", 42),
        ("churn_contract", 10_000),
        ("big_contract", 65),
    ]
    .into_iter()
    .map(|(function, expected)| NativeObjectInvocation {
        export_id: exports[function],
        arguments: Vec::new(),
        expected_status: status::OK,
        expected_result: Some(expected),
    })
    .collect::<Vec<_>>();

    assert_managed_native_object_invocations(
        "native-record-suite-native",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn native_record_suite_layouts_keep_nominal_identity_and_source_field_order() {
    let (modules, _) = native_record_module();
    let layouts = modules
        .iter()
        .flat_map(|module| &module.managed_layouts)
        .map(|layout| decode_aggregate_layout(layout).expect("record layout"))
        .collect::<Vec<_>>();
    let canonical = layouts
        .iter()
        .map(|layout| layout.canonical_type())
        .collect::<HashSet<_>>();

    assert!(canonical.contains("Named(native_record_suite_native.Pair)"));
    assert!(canonical.contains("Named(native_record_suite_native.Big)"));
    let big = layouts
        .iter()
        .find(|layout| layout.canonical_type() == "Named(native_record_suite_native.Big)")
        .expect("big record layout");
    assert_eq!(big.fields().len(), 64);
    assert_eq!(big.fields()[0].name(), Some("f1"));
    assert_eq!(big.fields()[63].name(), Some("f64"));
    assert_eq!(
        layouts
            .iter()
            .map(|layout| layout.managed().semantic_id())
            .collect::<HashSet<_>>()
            .len(),
        layouts.len()
    );
}

#[test]
fn native_record_suite_same_named_records_in_distinct_modules_do_not_alias() {
    let sources = [
        "\
module native_record_alpha.\n\
pub struct Pair { left: Int, right: Int }.\n\
pub constructor Pair {\n\
    (left: Int, right: Int): Pair -> Pair { left: left, right: right }\n\
}.\n\
pub make(): Pair -> Pair { left: 1, right: 2 }.\n",
        "\
module native_record_beta.\n\
pub struct Pair { left: Int, right: Int }.\n\
pub constructor Pair {\n\
    (left: Int, right: Int): Pair -> Pair { left: left, right: right }\n\
}.\n\
pub make(): Pair -> Pair { left: 3, right: 4 }.\n",
    ];
    let cores = sources
        .iter()
        .map(|source| {
            let syntax = parse_module_as_syntax_output(source).expect("parse nominal module");
            let resolved = resolve_syntax_module_output(&syntax).module;
            let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
            assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
            lower_syntax_module_output_to_core(&syntax, &resolved)
        })
        .collect::<Vec<_>>();
    let references = cores.iter().collect::<Vec<_>>();
    let modules =
        NativeModule::lower_application(&references).expect("lower distinct nominal records");
    let layouts = modules
        .iter()
        .flat_map(|module| &module.managed_layouts)
        .map(|layout| decode_aggregate_layout(layout).expect("record layout"))
        .filter(|layout| layout.variant_name() == Some("Pair"))
        .map(|layout| {
            (
                layout.canonical_type().to_string(),
                layout.managed().semantic_id(),
            )
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(layouts.len(), 2);
    assert_eq!(
        layouts.keys().map(String::as_str).collect::<HashSet<_>>(),
        HashSet::from([
            "Named(native_record_alpha.Pair)",
            "Named(native_record_beta.Pair)",
        ])
    );
    assert_ne!(
        layouts["Named(native_record_alpha.Pair)"],
        layouts["Named(native_record_beta.Pair)"]
    );
}
