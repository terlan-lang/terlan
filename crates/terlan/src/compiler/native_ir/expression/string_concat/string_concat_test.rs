use super::*;
use crate::compiler::native_ir::source_constructor_test::{check_sources, checked_provider};
use crate::compiler::native_ir::NativeModule;

#[test]
fn prefix_fusion_decodes_literals_and_does_not_allocate_a_literal_operand() {
    for literal in ["prefix", "\"prefix:\\u03bb\\n\"", ""] {
        let expected = core_string_runtime_value(literal).unwrap();
        let right = CoreExpr::Var("value".into());
        let lowered = lower(&CoreExpr::Binary(literal.into()), &right, |expression| {
            assert_eq!(expression, &right);
            Ok(NativeExpr::Param(0))
        })
        .unwrap();
        assert_eq!(
            lowered,
            NativeExpr::ManagedOperation {
                encoded: encode_string_prepend_literal_operation(&expected)
                    .unwrap()
                    .into(),
                args: vec![NativeExpr::Param(0)],
            }
        );
    }
    assert!(lower(
        &CoreExpr::Binary("\"bad\\q\"".into()),
        &CoreExpr::Var("value".into()),
        |_| panic!("invalid literal must reject before lowering its operand"),
    )
    .is_err());
}

#[test]
fn concatenation_flattens_only_append_nodes_in_evaluation_order() {
    let append = encode_string_append_operation();
    let concat = encode_string_concat_operation();
    for left_code in [&append, &concat] {
        for right_code in [&append, &concat] {
            let mut visited = Vec::new();
            let lowered = lower(
                &CoreExpr::Var("left".into()),
                &CoreExpr::Var("right".into()),
                |expression| {
                    let CoreExpr::Var(name) = expression else {
                        unreachable!()
                    };
                    visited.push(name.clone());
                    let (encoded, start) = if name == "left" {
                        (left_code, 0)
                    } else {
                        (right_code, 2)
                    };
                    Ok(NativeExpr::ManagedOperation {
                        encoded: encoded.clone().into(),
                        args: vec![NativeExpr::Param(start), NativeExpr::Param(start + 1)],
                    })
                },
            )
            .unwrap();
            assert_eq!(visited, ["left", "right"]);
            assert_eq!(
                lowered,
                NativeExpr::ManagedOperation {
                    encoded: concat.clone().into(),
                    args: (0..4).map(NativeExpr::Param).collect(),
                }
            );
        }
    }
    let other = NativeExpr::ManagedOperation {
        encoded: encode_string_prepend_literal_operation("prefix")
            .unwrap()
            .into(),
        args: vec![NativeExpr::Param(0)],
    };
    let lowered = lower(
        &CoreExpr::Var("a".into()),
        &CoreExpr::Var("b".into()),
        |_| Ok(other.clone()),
    )
    .unwrap();
    assert_eq!(
        lowered,
        NativeExpr::ManagedOperation {
            encoded: append.into(),
            args: vec![other.clone(), other],
        }
    );
}

#[test]
fn typed_string_lowering_is_identical_with_or_without_http_imports() {
    let source =
        "module app.Strings. pub join(a: String, b: String, c: String): String -> a + b + c.";
    let ordinary = checked_provider(source);
    let mut imported = ordinary.clone();
    imported.imports.extend(checked_provider(
        "module app.Imports. import std.http.Request. import std.http.Response. pub value(): Int -> 1.",
    ).imports);
    let plain = NativeModule::lower_application(&[&ordinary]).unwrap();
    let http = NativeModule::lower_application(&[&imported]).unwrap();
    assert_eq!(plain, http);
}

#[test]
fn concatenation_propagates_operand_errors_without_reordering_or_retrying() {
    for failing in ["left", "right"] {
        let mut visited = Vec::new();
        let result = lower(
            &CoreExpr::Var("left".into()),
            &CoreExpr::Var("right".into()),
            |expression| {
                let CoreExpr::Var(name) = expression else {
                    unreachable!()
                };
                visited.push(name.clone());
                if name == failing {
                    Err("operand failed".into())
                } else {
                    Ok(NativeExpr::Param(0))
                }
            },
        );
        assert_eq!(result, Err("operand failed".into()));
        assert_eq!(
            visited,
            if failing == "left" {
                vec!["left"]
            } else {
                vec!["left", "right"]
            }
        );
    }
}

#[test]
fn source_concatenation_preserves_branches_scopes_unicode_and_integer_addition() {
    for module in ["app.Strings", "std.sample.Strings"] {
        check_sources(&[&format!(
            r#"
module {module}.
import std.vm.Process.
prefix(value: String): String -> "prefix:" + value.
join(a: String, b: String, c: String): String -> a + b + c.
choose(value: Bool): String -> if {{ value -> "a"; true -> "b" }}.
delayed(value: String): String -> let _parked = Process.yield_now(); value.
classify(value: String): Int -> case value {{
    "" -> 0;
    "ab" -> 1;
    _ -> 2
}}.
pub check(): Bool ->
    let saved = "left";
    let scoped = (let saved = "right"; saved + "!");
    prefix("value") == "prefix:value"
        and join("a", "", "b") == "ab"
        and join("a", "b", "c") == "abc"
        and (choose(true) + choose(false)) == "ab"
        and classify(join("a", "", "b")) == 1
        and classify("") == 0 and classify("other") == 2
        and (delayed("first") + delayed("second") + delayed("third")) == "firstsecondthird"
        and saved == "left" and scoped == "right!"
        and ("\u03bb" + "\n") == "\u03bb\n"
        and 1 + 2 + 3 == 6.
"#
        )]);
    }
}
