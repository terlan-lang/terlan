//! Standard list storage normalization must not erase user nominal identity.

use super::*;

#[test]
fn only_the_qualified_standard_list_uses_builtin_storage() {
    let resolve_type =
        |ty: &CoreType| resolve(ty, "app", &[], &HashMap::new(), &mut HashSet::new());
    for constructor in ["app.Containers.List", "app.List", "List"] {
        let user_type = CoreType::Apply {
            constructor: constructor.into(),
            args: vec![CoreType::Int],
        };
        assert_eq!(resolve_type(&user_type), user_type);
    }
    let standard = CoreType::Apply {
        constructor: "std.collections.List.List".into(),
        args: vec![CoreType::Int],
    };
    assert_eq!(
        resolve_type(&standard),
        CoreType::List(Box::new(CoreType::Int))
    );
    let nested = CoreType::Apply {
        constructor: "std.collections.List.List".into(),
        args: vec![standard],
    };
    let resolved = resolve_type(&nested);
    assert_eq!(
        resolved,
        CoreType::List(Box::new(CoreType::List(Box::new(CoreType::Int))))
    );
    assert_eq!(resolve_type(&resolved), resolved);
}

#[test]
fn effect_run_keeps_its_declared_result_after_alias_expansion() {
    use crate::terlan_typeck::{CoreExpr, CoreIntrinsicId, CorePrimitiveIntrinsic};

    let sources = [
        r#"
module effect_result_types.
import std.core.Effect.
pub integer(plan: Effect[Int]): Int -> Effect.run(plan).
pub text(plan: Effect[String]): String -> Effect.run(plan).
pub nested(plan: Effect[List[String]]): List[String] -> Effect.run(plan).
pub alias(plan: Effect[Bool]): Bool -> let saved = plan; Effect.run(saved).
pub generic[T](plan: Effect[T]): T -> Effect.run(plan).
"#,
        include_str!("../../../../../../std/core/Effect.terl"),
    ];
    let mut cores = sources
        .iter()
        .map(|source| {
            let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source).unwrap();
            let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
            let resolved = crate::terlan_hir::resolve_syntax_module_output_with_interfaces(
                &syntax,
                &interfaces,
            )
            .module;
            assert!(
                crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved)
                    .is_empty()
            );
            crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved)
        })
        .collect::<Vec<_>>();
    let roots = cores[0]
        .functions
        .iter()
        .map(|function| {
            (
                cores[0].module.clone(),
                function.name.clone(),
                function.arity,
            )
        })
        .collect::<Vec<_>>();
    super::super::super::open_std_pruning::prune_application_to_function_roots(&mut cores, &roots)
        .unwrap();
    assert!(cores[1]
        .functions
        .iter()
        .all(|function| function.name != "run"));
    super::super::super::nominal_identity::qualify_application_nominal_types(&mut cores);
    expand_transparent_aliases(&mut cores);
    for _ in 0..2 {
        super::super::super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(&mut cores);
        for function in &cores[0].functions {
            let mut body = function.clauses[0].body.core_expr.as_ref().unwrap();
            if let CoreExpr::Let { body: nested, .. } = body {
                body = nested;
            }
            let CoreExpr::Intrinsic(call) = body else {
                panic!("expected intrinsic for {}: {body:?}", function.name);
            };
            assert_eq!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun)
            );
            assert_eq!(
                Some(&call.return_type),
                function.core_return_type.as_ref(),
                "{}",
                function.name
            );
            assert_eq!(call.effects.effects, ["vm_effect_execution"]);
        }
    }
    for unrelated in [
        CoreType::Int,
        CoreType::Dynamic,
        CoreType::Apply {
            constructor: "app.Effect".to_string(),
            args: vec![CoreType::Int],
        },
    ] {
        let mut invalid = cores.clone();
        let index = invalid[0]
            .functions
            .iter()
            .position(|function| function.name == "integer")
            .unwrap();
        let function = &mut invalid[0].functions[index];
        function.params[0].core_ty = Some(unrelated);
        let Some(CoreExpr::Intrinsic(call)) = function.clauses[0].body.core_expr.as_mut() else {
            panic!("expected intrinsic");
        };
        call.return_type = CoreType::Dynamic;
        super::super::super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(&mut invalid);
        let Some(CoreExpr::Intrinsic(call)) = invalid[0].functions[index].clauses[0]
            .body
            .core_expr
            .as_ref()
        else {
            panic!("expected intrinsic");
        };
        assert_eq!(call.return_type, CoreType::Dynamic);
    }
}
