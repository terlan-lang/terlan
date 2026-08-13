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
