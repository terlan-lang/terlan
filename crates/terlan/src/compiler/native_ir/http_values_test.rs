//! HTTP packages use ordinary source lowering and typed native capabilities.

use std::collections::HashMap;

use super::source_constructor_test::checked_provider;
use super::{expression, NativeModule};
use crate::terlan_typeck::{CoreExpr, CoreImport, CoreImportKind};

#[path = "http_response_builder_test.rs"]
mod builder_authority_test;
#[path = "http_middleware_library_test.rs"]
mod middleware_authority_test;
#[path = "http_option_library_test.rs"]
mod option_authority_test;
#[path = "http_session_library_test.rs"]
mod session_authority_test;

#[test]
fn http_imports_do_not_install_runtime_layouts() {
    for module in [
        "std.http.Request",
        "std.http.Response",
        "std.http.Cookies",
        "std.http.Session",
        "std.http.Error",
        "std.http.Router",
        "app.Request",
        "app.Response",
        "app.Cookies",
        "app.Session",
    ] {
        for kind in [CoreImportKind::Module, CoreImportKind::TypeModule] {
            let mut core = checked_provider("module imports_only. pub check(): Int -> 1.");
            core.imports.push(CoreImport {
                module: module.into(),
                kind,
            });
            let modules = NativeModule::lower_application(&[&core]).unwrap();
            assert!(modules
                .iter()
                .all(|module| module.managed_layouts.is_empty()));
            assert!(modules
                .iter()
                .all(|module| module.managed_collections.is_empty()));
        }
    }
}

#[test]
fn retired_http_calls_have_no_inferred_type_or_executable_lowering() {
    for name in [
        "method",
        "path",
        "query_string",
        "body_text",
        "body_file_path",
        "param",
        "query",
        "header",
        "cookie",
        "cookies",
        "json_payload",
        "option_is_none",
        "option_some",
        "session_get_is_none",
        "session_get_some",
        "session_current",
        "session_get",
        "session_set",
        "session_delete",
        "session_rotate",
        "session_expire",
        "session_is_live",
        "session_with_response",
        "session_response_cookie",
        "session_live_cookie",
        "session_live_identity",
        "response_build_0",
        "response_build_1",
        "response_build_2",
        "response_build_3",
        "response_status",
        "response_header",
        "empty_headers",
        "empty_chunks",
        "string_equal",
        "string_append",
        "string_concat",
        "string_prepend_literal",
    ] {
        for arity in 0..=4 {
            let expression = CoreExpr::RemoteCall {
                module: "$terlan.managed.http".into(),
                function: name.into(),
                type_args: vec![],
                args: vec![CoreExpr::Int(-1); arity],
            };
            assert_eq!(
                expression::infer_native_type(&expression, &HashMap::new(), &HashMap::new()),
                None,
                "{name}/{arity}"
            );
            assert!(
                expression::lower_expr_with_constructors(
                    &expression,
                    &HashMap::new(),
                    &HashMap::new(),
                    &HashMap::new(),
                    &HashMap::new(),
                    &Default::default(),
                )
                .is_err(),
                "{name}/{arity}"
            );
            let mut core = checked_provider("module retired_http. pub check(): Int -> 1.");
            core.functions[0].clauses[0].body.core_expr = Some(expression);
            assert!(
                NativeModule::lower_application(&[&core]).is_err(),
                "{name}/{arity}"
            );
        }
    }
}
