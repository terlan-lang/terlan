//! Source-to-NativeIR checks for persistent collection values.

use crate::{
    terlan_hir::resolve_syntax_module_output, terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::lower_syntax_module_output_to_core,
};

use super::{NativeExpr, NativeModule};

fn lower(source: &str) -> Vec<NativeModule> {
    let syntax = parse_module_as_syntax_output(source).expect("parse collection source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core]).expect("collection NativeIR")
}

#[test]
fn aggregate_literal_equality_retains_operand_types_and_executes() {
    let modules = lower(
        "module aggregate_equality.\n\
         identity(value: Int): Int -> value.\n\
         pub maps(left: Int, right: Int): Bool ->\n\
             true == ({name: \"Ada\", age: identity(left)} == {name: \"Ada\", age: right}).\n\
         pub reordered(left: Int, right: Int): Bool ->\n\
             {name: \"Ada\", age: left} != {age: right, name: \"Ada\"}.\n\
         pub nested(left: Int, right: Int): Bool ->\n\
             {item: {left, [1, 2]}} == {item: {right, [1, 2]}}.\n\
         pub nested_records(left: Int, right: Int): Bool ->\n\
             {item: {name: \"Ada\", age: left}, enabled: true}\n\
             == {enabled: true, item: {age: right, name: \"Ada\"}}.\n\
         pub from_case(left: Int, right: Int): Bool ->\n\
             case {name: \"Ada\", age: left} {\n\
                 {age: value} where value == right -> true;\n\
                 _ -> false\n\
             }.\n",
    );
    let object = super::emit_native_application_object("aggregate-equality", &modules)
        .expect("emit aggregate comparisons");
    let mut invocations = Vec::new();
    for (name, equal, different) in [
        ("maps", 1, 0),
        ("reordered", 0, 1),
        ("nested", 1, 0),
        ("nested_records", 1, 0),
        ("from_case", 1, 0),
    ] {
        let export_id = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("comparison export")
            .export_id;
        for (right, expected) in [(7, equal), (8, different)] {
            invocations.push(super::native_object_test_support::NativeObjectInvocation {
                export_id,
                arguments: vec![7, right],
                expected_status: super::status::OK,
                expected_result: Some(expected),
            });
        }
    }
    super::native_object_test_support::assert_managed_native_object_invocations(
        "aggregate-equality",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn structural_map_layout_reordering_retains_source_call_order() {
    use std::collections::HashMap;

    use super::NativeType;
    use crate::terlan_typeck::{CoreExpr, CoreMapExprField, CoreMapTypeField, CoreType};

    let expected = CoreType::Map(
        ["first", "second"]
            .map(|key| CoreMapTypeField {
                key: key.into(),
                operator: ":".into(),
                value: CoreType::Int,
            })
            .to_vec(),
    );
    let body = CoreExpr::Map(
        [("second", 2), ("first", 1)]
            .map(|(key, value)| CoreMapExprField {
                key: key.into(),
                required: true,
                value: CoreExpr::Call {
                    function: "mark".into(),
                    args: vec![CoreExpr::Int(value)],
                },
            })
            .to_vec(),
    );
    let lowered = super::collection_values::lower_boundary_collection_value(
        &body,
        Some(&expected),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::from([(("mark".into(), 1), 7)]),
        &HashMap::from([(("mark".into(), 1), NativeType::Int)]),
        &HashMap::new(),
    )
    .expect("lower structural map")
    .expect("record expression");
    let NativeExpr::Let { bindings, body } = lowered else {
        panic!("source-order bindings");
    };
    assert!(matches!(&bindings[..], [
        NativeExpr::Call { function: 7, args: first },
        NativeExpr::Call { function: 7, args: second },
    ] if matches!(&first[..], [NativeExpr::Int(2)]) && matches!(&second[..], [NativeExpr::Int(1)])));
    let NativeExpr::Construct { fields, .. } = *body else {
        panic!("record construction");
    };
    assert!(matches!(
        &fields[..],
        [NativeExpr::Param(1), NativeExpr::Param(0)]
    ));
}

#[test]
fn inferred_record_list_operands_admit_their_collection_schema() {
    let modules = lower(
        "module native_record_list.\n\
         import std.collections.List.\n\
         pub struct Entry { value: Int }.\n\
         make(value: Int): Entry -> Entry(value = value).\n\
         pub count(): Int -> List.length([make(1), make(2)]).\n",
    );
    let expected = "List(Struct(native_record_list.Entry;value:Int))";
    assert!(
        modules
            .iter()
            .flat_map(|module| &module.managed_collections)
            .any(
                |encoded| crate::runtime::native_image::managed::decode_collection_layout(encoded)
                    .expect("valid collection descriptor")
                    .canonical_type()
                    == expected
            ),
        "inferred operand list must be admitted even without a List parameter or result"
    );
    let object = super::emit_native_application_object("inferred-record-list", &modules)
        .expect("emit inferred record list");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "count")
        .expect("count export")
        .export_id;
    super::native_object_test_support::assert_managed_native_object_invocations(
        "inferred-record-list",
        &modules,
        &object,
        &[super::native_object_test_support::NativeObjectInvocation {
            export_id,
            arguments: Vec::new(),
            expected_status: super::status::OK,
            expected_result: Some(2),
        }],
    );
}

#[test]
fn list_literal_and_cons_lower_to_managed_collection_operations() {
    let modules = lower(
        "module native_collections.\n\n\
         pub values(): List[Int] -> [1, 2, 3].\n\n\
         pub prepend(tail: List[Int]): List[Int] -> [0 | tail].\n",
    );
    let values = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "values")
        .expect("list literal function");
    let prepend = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "prepend")
        .expect("list cons function");
    assert!(matches!(
        values.body,
        NativeExpr::ManagedOperation { ref encoded, ref args }
            if encoded.starts_with(b"TVMC") && encoded[6] == 1 && args.len() == 3
    ));
    assert!(matches!(
        prepend.body,
        NativeExpr::ManagedOperation { ref encoded, ref args }
            if encoded.starts_with(b"TVMC") && encoded[6] == 2 && args.len() == 2
    ));
}

#[test]
fn typed_map_literal_lowers_in_source_field_order() {
    let modules = lower(
        "module native_map.\n\n\
         pub values(): Map[String, Int] -> {first: 1, second: 2}.\n",
    );
    let values = &modules[0].functions[0];
    assert!(matches!(
        values.body,
        NativeExpr::ManagedOperation { ref encoded, ref args }
            if encoded.starts_with(b"TVMC") && encoded[6] == 3 && args.len() == 4
    ));
}

#[test]
fn typed_map_constructor_cast_lowers_from_return_context() {
    let modules = lower(
        "module native_map_constructor.\n\n\
         pub values(): Map[String, Int] -> Map({\"answer\", 42}).\n",
    );
    let values = &modules[0].functions[0];
    assert!(matches!(
        values.body,
        NativeExpr::ManagedOperation { ref encoded, ref args }
            if encoded.starts_with(b"TVMC") && encoded[6] == 3 && args.len() == 2
    ));
}

#[test]
fn typed_map_constructor_lowers_as_direct_receiver_argument() {
    let modules = lower(
        "module native_map_argument.\n\n\
         import std.collections.Map.\n\
         import std.core.Option.{None, Some}.\n\n\
         pub lookup(): Bool ->\n\
             case Map.get(Map({\"answer\", 42}), \"answer\") {\n\
                 Some(value) -> value == 42;\n\
                 None -> false\n\
             }.\n",
    );
    let lookup = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "lookup")
        .expect("map lookup function");
    assert!(contains_collection_tag(&lookup.body, 3));
}

#[test]
fn managed_locals_recover_checked_element_type_for_list_literals() {
    let modules = lower(
        "module native_managed_local_list.\n\n\
         pub type Item = { value: Int }.\n\n\
         pub collect(item: Item): List[Item] ->\n\
             let retained = item;\n\
             [retained].\n",
    );
    let collect = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "collect")
        .expect("managed list function");
    assert!(contains_collection_tag(&collect.body, 1));
}

fn contains_collection_tag(expr: &NativeExpr, tag: u8) -> bool {
    match expr {
        NativeExpr::ManagedOperation { encoded, args } => {
            (encoded.starts_with(b"TVMC") && encoded.get(6) == Some(&tag))
                || args.iter().any(|arg| contains_collection_tag(arg, tag))
        }
        NativeExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| contains_collection_tag(binding, tag))
                || contains_collection_tag(body, tag)
        }
        NativeExpr::If { clauses } => clauses.iter().any(|(condition, body)| {
            contains_collection_tag(condition, tag) || contains_collection_tag(body, tag)
        }),
        NativeExpr::Call { args, .. } | NativeExpr::TailCall { args, .. } => {
            args.iter().any(|arg| contains_collection_tag(arg, tag))
        }
        NativeExpr::CallThen { args, values, .. } => args
            .iter()
            .chain(values)
            .any(|arg| contains_collection_tag(arg, tag)),
        NativeExpr::Binary { left, right, .. } => {
            contains_collection_tag(left, tag) || contains_collection_tag(right, tag)
        }
        _ => false,
    }
}

#[test]
fn pure_single_generator_comprehension_expands_to_private_native_recursion() {
    let modules = lower(
        "module native_comprehension.\n\n\
         pub increment_positive(values: List[Int]): List[Int] ->\n\
             [value + 1 | value <- values, value > 0].\n",
    );
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let public = functions
        .iter()
        .find(|function| function.name == "increment_positive")
        .expect("public comprehension");
    let helper = functions
        .iter()
        .find(|function| function.name.starts_with("$aot_comprehension_"))
        .expect("private comprehension helper");
    assert!(public.public);
    assert!(!helper.public);
    for tag in [22, 25] {
        assert!(
            functions
                .iter()
                .any(|function| contains_collection_tag(&function.body, tag))
                || modules
                    .iter()
                    .flat_map(|module| &module.continuations)
                    .any(|continuation| contains_collection_tag(&continuation.body, tag)),
            "missing collection operation {tag}"
        );
    }
}

#[test]
fn multiple_generator_comprehension_expands_to_ordered_native_collectors() {
    let modules = lower(
        "module comprehension_shape.\n\n\
         pub pairs(left: List[Int], right: List[Int]): List[Int] ->\n\
             [first + second | first <- left, second <- right].\n",
    );
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();

    assert_eq!(
        functions
            .iter()
            .filter(|function| function.name.starts_with("$aot_comprehension_"))
            .count(),
        2
    );
    assert!(functions
        .iter()
        .all(|function| !function.name.starts_with("$aot_comprehension_") || !function.public));
    for tag in [22, 25] {
        assert!(functions
            .iter()
            .any(|function| contains_collection_tag(&function.body, tag)));
    }
}

#[test]
fn comprehension_expansion_budget_has_stable_prelink_rejection() {
    let mut source = String::from("module comprehension_budget.\n\n");
    for index in 0..=128 {
        source.push_str(&format!(
            "pub map_{index}(values: List[Int]): List[Int] ->\n\
                 [value + {index} | value <- values].\n\n"
        ));
    }
    let syntax = parse_module_as_syntax_output(&source).expect("parse comprehension budget source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let error =
        NativeModule::lower_application(&[&core]).expect_err("reject comprehension explosion");

    assert!(
        error.starts_with("error[native_ir.comprehension_budget]"),
        "{error}"
    );
}
