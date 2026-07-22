use std::collections::HashMap;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreExpr, CoreLetBinding,
    CorePattern,
};

use super::scalar_replacement::scalar_replace_fixed_aggregates;
use super::{NativeExpr, NativeModule};

/// Returns the empty constructor table used by structural index tests.
fn layouts() -> super::constructors::NativeConstructorLayouts {
    HashMap::new()
}

/// Builds one indexed local tuple whose identity never escapes.
fn indexed_tuple_sum() -> CoreExpr {
    CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_owned(),
            left: Box::new(CoreExpr::Index {
                base: Box::new(CoreExpr::Var("pair".to_owned())),
                index: Box::new(CoreExpr::Int(0)),
            }),
            right: Box::new(CoreExpr::Index {
                base: Box::new(CoreExpr::Var("pair".to_owned())),
                index: Box::new(CoreExpr::Int(1)),
            }),
        }),
    }
}

/// Replaces all bounded constant indexes into one fixed local tuple.
#[test]
fn constant_indexes_replace_the_local_tuple() {
    assert_eq!(
        scalar_replace_fixed_aggregates(&indexed_tuple_sum(), &layouts()),
        CoreExpr::Let {
            bindings: vec![
                CoreLetBinding {
                    pattern: CorePattern::Var("$native_sroa_0_0".to_owned()),
                    value: CoreExpr::Int(20),
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("$native_sroa_0_1".to_owned()),
                    value: CoreExpr::Int(22),
                },
            ],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_owned(),
                left: Box::new(CoreExpr::Var("$native_sroa_0_0".to_owned())),
                right: Box::new(CoreExpr::Var("$native_sroa_0_1".to_owned())),
            }),
        }
    );
}

/// Replaces a directly indexed tuple while evaluating every element once.
#[test]
fn direct_constant_index_preserves_all_element_evaluation() {
    let expression = CoreExpr::Index {
        base: Box::new(CoreExpr::Tuple(vec![
            CoreExpr::Call {
                function: "first".to_owned(),
                args: Vec::new(),
            },
            CoreExpr::Call {
                function: "second".to_owned(),
                args: Vec::new(),
            },
        ])),
        index: Box::new(CoreExpr::Int(1)),
    };
    let CoreExpr::Let { bindings, body } = scalar_replace_fixed_aggregates(&expression, &layouts())
    else {
        panic!("expected scalar bindings for direct tuple index");
    };

    assert!(matches!(
        bindings.as_slice(),
        [
            CoreLetBinding { value: CoreExpr::Call { function: first, .. }, .. },
            CoreLetBinding { value: CoreExpr::Call { function: second, .. }, .. },
        ] if first == "first" && second == "second"
    ));
    assert_eq!(*body, CoreExpr::Var("$native_sroa_0_1".to_owned()));
}

/// Keeps dynamic indexing under the ordinary checked runtime contract.
#[test]
fn dynamic_index_blocks_local_tuple_replacement() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::Index {
            base: Box::new(CoreExpr::Var("pair".to_owned())),
            index: Box::new(CoreExpr::Var("offset".to_owned())),
        }),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Keeps out-of-range indexing so its failure cannot be optimized away.
#[test]
fn out_of_range_index_blocks_local_tuple_replacement() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::Index {
            base: Box::new(CoreExpr::Var("pair".to_owned())),
            index: Box::new(CoreExpr::Int(2)),
        }),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Keeps negative indexing so signed bounds behavior remains observable.
#[test]
fn negative_index_blocks_local_tuple_replacement() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::Index {
            base: Box::new(CoreExpr::Var("pair".to_owned())),
            index: Box::new(CoreExpr::Int(-1)),
        }),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Lowers direct and local source indexes into allocation-free NativeIR.
#[test]
fn source_application_scalar_replaces_fixed_tuple_indexes() {
    let syntax = parse_module_as_syntax_output(
        "\
module fixed_index.\n\
\n\
pub direct(): Int ->\n\
    {10, 20}[0].\n\
\n\
pub local(): Int ->\n\
    let pair = {30, 40};\n\
    pair[1].\n",
    )
    .expect("fixed-index source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("native application");
    let direct = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "direct")
        .expect("direct native function");
    let local = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "local")
        .expect("local native function");

    assert!(matches!(
        direct.body,
        NativeExpr::Let { ref bindings, ref body }
            if bindings == &[NativeExpr::Int(10), NativeExpr::Int(20)]
                && body.as_ref() == &NativeExpr::Param(0)
    ));
    assert!(matches!(
        local.body,
        NativeExpr::Let { ref bindings, ref body }
            if bindings == &[NativeExpr::Int(30), NativeExpr::Int(40)]
                && body.as_ref() == &NativeExpr::Param(1)
    ));
}
