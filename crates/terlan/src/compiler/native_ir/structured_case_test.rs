//! Native application checks for bounded structured pattern matching.

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{
        lower_syntax_module_output_to_core, CoreCaseClause, CoreExpr, CorePattern,
        CoreTupleTypeElem, CoreType,
    },
};

use super::{NativeExpr, NativeModule};

fn lower(source: &str) -> Vec<NativeModule> {
    let syntax = parse_module_as_syntax_output(source).expect("parse structured case source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core]).expect("structured case NativeIR")
}

fn contains_managed_operation(expr: &NativeExpr, magic: &[u8; 4], tag: u8) -> bool {
    match expr {
        NativeExpr::ManagedOperation { encoded, args } => {
            (encoded.starts_with(magic) && encoded.get(6) == Some(&tag))
                || args
                    .iter()
                    .any(|argument| contains_managed_operation(argument, magic, tag))
        }
        NativeExpr::Construct { fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. }
        | NativeExpr::ContinuationTailCall { args: fields, .. } => fields
            .iter()
            .any(|field| contains_managed_operation(field, magic, tag)),
        NativeExpr::InvokeClosure { callee, args, .. } => {
            contains_managed_operation(callee, magic, tag)
                || args
                    .iter()
                    .any(|argument| contains_managed_operation(argument, magic, tag))
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            values,
            ..
        } => {
            contains_managed_operation(callee, magic, tag)
                || args
                    .iter()
                    .chain(values)
                    .any(|argument| contains_managed_operation(argument, magic, tag))
        }
        NativeExpr::CallThen { args, values, .. } => args
            .iter()
            .chain(values)
            .any(|value| contains_managed_operation(value, magic, tag)),
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => contains_managed_operation(value, magic, tag),
        NativeExpr::Binary { left, right, .. } => {
            contains_managed_operation(left, magic, tag)
                || contains_managed_operation(right, magic, tag)
        }
        NativeExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| contains_managed_operation(binding, magic, tag))
                || contains_managed_operation(body, magic, tag)
        }
        NativeExpr::If { clauses } => clauses.iter().any(|(condition, body)| {
            contains_managed_operation(condition, magic, tag)
                || contains_managed_operation(body, magic, tag)
        }),
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            contains_managed_operation(protected, magic, tag)
                || contains_managed_operation(success, magic, tag)
                || contains_managed_operation(failure, magic, tag)
                || cleanup
                    .iter()
                    .any(|value| contains_managed_operation(value, magic, tag))
        }
        NativeExpr::Suspend {
            arguments, values, ..
        } => arguments
            .iter()
            .chain(values)
            .any(|value| contains_managed_operation(value, magic, tag)),
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::ManagedLiteral { .. }
        | NativeExpr::Param(_) => false,
    }
}

#[test]
fn tuple_list_and_map_patterns_lower_to_bounded_managed_matchers() {
    let modules = lower(
        "module structured_case_source.\n\n\
         pub tuple_sum(value: {Int, Int}): Int ->\n\
             case value { {left, right} -> left + right }.\n\n\
         pub list_head(values: List[Int]): Int ->\n\
             case values { [head | _tail] -> head; [] -> 0 }.\n\n\
         pub map_value(values: Map[String, Int]): Int ->\n\
             case values { {answer: found} -> found; _ -> 0 }.\n",
    );

    let function = |name: &str| {
        modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    assert!(contains_managed_operation(
        &function("tuple_sum").body,
        b"TVMP",
        1
    ));
    assert!(contains_managed_operation(
        &function("list_head").body,
        b"TVMC",
        5
    ));
    assert!(contains_managed_operation(
        &function("map_value").body,
        b"TVMC",
        8
    ));
}

#[test]
fn none_pattern_accepts_immediate_and_managed_zero_field_option_variants() {
    let modules = lower(
        "module structured_option_source.\n\n\
         import std.core.Option.{None, Option, Some}.\n\n\
         pub is_none(value: Option[Int]): Bool ->\n\
             case value { None -> true; Some(_value) -> false }.\n",
    );
    let function = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "is_none")
        .expect("missing option matcher");

    assert!(contains_managed_operation(&function.body, b"TVMP", 2));
}

#[test]
fn option_pattern_recovers_the_checked_type_of_a_record_field() {
    let modules = lower(
        "module structured_record_option_source.\n\n\
         import std.core.Option.{None, Option, Some}.\n\n\
         pub struct Request { selected: Option[String] }.\n\n\
         pub selected(request: Request): Bool ->\n\
             case request.selected { Some(_value) -> true; None -> false }.\n",
    );
    let function = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "selected")
        .expect("missing record-field option matcher");

    assert!(contains_managed_operation(&function.body, b"TVMP", 2));
}

#[test]
fn binary_layout_patterns_lower_to_one_checked_plan_and_field_extractors() {
    let modules = lower(
        "module native_binary_pattern.\n\n\
         pub port(value: Binary): Int ->\n\
             case value {\n\
                 Binary[big] { port: UInt[8], _: Rest } -> port;\n\
                 _ -> 0\n\
             }.\n",
    );
    let body = &modules[0]
        .functions
        .iter()
        .find(|function| function.name == "port")
        .expect("port function")
        .body;
    assert!(contains_managed_operation(body, b"TVPB", 1));
    assert!(contains_managed_operation(body, b"TVPB", 2));
}

#[test]
fn record_and_constructor_patterns_lower_to_checked_aggregate_matchers() {
    let modules = lower(
        "module native_aggregate_patterns.\n\n\
         pub struct Point { x: Int, y: Int }.\n\n\
         pub constructor Point {\n\
             (x: Int, y: Int): Point -> Point { x: x, y: y }\n\
         }.\n\n\
         pub constructor Ok {\n\
             (value: Int): Result[Int, Int] -> value\n\
         }.\n\n\
         pub constructor Error {\n\
             (reason: Int): Result[Int, Int] -> reason\n\
         }.\n\n\
         pub point_x(value: Point): Int ->\n\
             case value { Point { x: found } -> found; _ -> 0 }.\n\n\
         pub unwrap(value: Result[Int, Int]): Int ->\n\
             case value { Ok(found) -> found; _ -> 0 }.\n",
    );
    let function = |name: &str| {
        modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };

    assert!(contains_managed_operation(
        &function("point_x").body,
        b"TVMP",
        1
    ));
    assert!(contains_managed_operation(
        &function("unwrap").body,
        b"TVMP",
        2
    ));
}

#[test]
fn structured_pattern_depth_budget_has_stable_prelink_rejection() {
    let syntax = parse_module_as_syntax_output(
        "module structured_depth.\n\npub run(value: Int): Int -> value.\n",
    )
    .expect("parse structured-depth source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut ty = CoreType::Int;
    let mut pattern = CorePattern::Var("found".to_string());
    for _ in 0..=64 {
        ty = CoreType::Tuple(vec![CoreTupleTypeElem::Type(ty)]);
        pattern = CorePattern::Tuple(vec![pattern]);
    }
    let function = &mut core.functions[0];
    function.params[0].ty = ty.contract_text();
    function.params[0].core_ty = Some(ty);
    function.clauses[0].body.core_expr = Some(CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Var("value".to_string())),
        clauses: vec![CoreCaseClause {
            pattern,
            guard: None,
            body: CoreExpr::Var("found".to_string()),
        }],
    });
    let error = NativeModule::lower_application(&[&core]).expect_err("reject deep pattern");

    assert_eq!(
        error,
        "error[native_ir.structured_pattern_depth]: pattern exceeds 64 layers; while lowering \
         `structured_depth.run/1`"
    );
}
