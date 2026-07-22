use super::test_support::check_syntax_output;
use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::DiagSeverity;

const RESULT_DECL: &str = "pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
                           pub type Err[E] = {Atom[\"error\"], reason: E}.\n\
                           pub type Result[T, E] = Ok[T] | Err[E].\n";

fn result_source(body: &str) -> String {
    format!(
        "module let_else_typecheck.\n{RESULT_DECL}\n\
         pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->\n\
             {body}.\n"
    )
}

#[test]
fn grouped_let_else_typechecks_success_and_fallback_paths() {
    let source = result_source(
        "let {\n\
             Ok(left) <- first;\n\
             Ok(right) <- second\n\
         } else {\n\
             Err(reason) -> Err(reason)\n\
         };\n\
         Ok(left + right)",
    );
    let diagnostics = check_syntax_output(&source);

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !matches!(diagnostic.severity, DiagSeverity::Error)),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn grouped_let_else_does_not_expose_success_bindings_to_fallback() {
    let source = result_source(
        "let {\n\
             Ok(left) <- first;\n\
             Ok(right) <- second\n\
         } else {\n\
             Err(_) -> Ok(left)\n\
         };\n\
         Ok(left + right)",
    );
    let diagnostics = check_syntax_output(&source);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic.severity, DiagSeverity::Error)
                && diagnostic
                    .message
                    .contains("let else fallback cannot reference success binding `left`")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn grouped_let_else_lowers_left_to_right_to_nested_cases() {
    let source = result_source(
        "let {\n\
             Ok(left) <- first;\n\
             Ok(right) <- second\n\
         } else {\n\
             Err(reason) -> Err(reason)\n\
         };\n\
         Ok(left + right)",
    );
    let module = parse_module_as_syntax_output(&source).expect("parse let else source");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "resolve")
        .expect("resolve function");
    let Some(CoreExpr::Case { scrutinee, clauses }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected outer case: {:?}",
            function.clauses[0].body.core_expr
        );
    };

    assert_eq!(scrutinee.as_ref(), &CoreExpr::Var("first".to_string()));
    assert_eq!(clauses.len(), 2);
    let CoreExpr::Case {
        scrutinee,
        clauses: nested_clauses,
    } = &clauses[0].body
    else {
        panic!("expected nested success case: {:?}", clauses[0].body);
    };
    assert_eq!(scrutinee.as_ref(), &CoreExpr::Var("second".to_string()));
    assert_eq!(nested_clauses.len(), 2);
    assert_eq!(clauses[1], nested_clauses[1]);
}

#[test]
fn guarded_fallback_is_not_treated_as_exhaustive() {
    let source = result_source(
        "let {\n\
             Ok(left) <- first\n\
         } else {\n\
             Err(reason) where reason == \"retry\" -> Err(reason)\n\
         };\n\
         Ok(left)",
    );
    let diagnostics = check_syntax_output(&source);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic.severity, DiagSeverity::Error)
                && diagnostic
                    .message
                    .contains("non-exhaustive case expression")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn guarded_shape_grouped_let_typechecks_and_lowers_success_guards() {
    let source = "module guarded_shape_grouped_let.\n\
                  shape Positive(value) = value where value > 0.\n\
                  pub add(left: Int, right: Int): Int ->\n\
                      let {\n\
                          Positive(first) <- left;\n\
                          Positive(second) <- right\n\
                      } else {\n\
                          _ -> 0\n\
                      };\n\
                      first + second.\n";
    let diagnostics = check_syntax_output(source);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !matches!(diagnostic.severity, DiagSeverity::Error)),
        "diagnostics: {diagnostics:?}"
    );

    let module = parse_module_as_syntax_output(source).expect("parse guarded grouped let");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "add")
        .expect("add function");
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!("expected outer case");
    };
    assert!(clauses[0].guard.is_some());
    let CoreExpr::Case {
        clauses: nested_clauses,
        ..
    } = &clauses[0].body
    else {
        panic!("expected nested case");
    };
    assert!(nested_clauses[0].guard.is_some());
}

#[test]
fn guarded_shape_grouped_let_keeps_fallback_outside_success_scope() {
    let source = "module guarded_shape_grouped_let_scope.\n\
                  shape Positive(value) = value where value > 0.\n\
                  pub read(input: Int): Int ->\n\
                      let {\n\
                          Positive(value) <- input\n\
                      } else {\n\
                          _ -> value\n\
                      };\n\
                      value.\n";
    let diagnostics = check_syntax_output(source);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic.severity, DiagSeverity::Error)
                && diagnostic
                    .message
                    .contains("let else fallback cannot reference success binding `value`")
        }),
        "diagnostics: {diagnostics:?}"
    );
}
