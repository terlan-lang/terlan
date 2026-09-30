//! Retired request facades must not substitute for source-owned record schemas.

use super::*;
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType};

pub(super) fn map_lookup() -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet),
        args: vec![CoreExpr::Var("values".into()), string("key")],
        return_type: CoreType::Apply {
            constructor: "Option".into(),
            args: vec![CoreType::String],
        },
        effects: CoreEffectSet { effects: vec![] },
        span: crate::terlan_syntax::span::Span::new(0, 0),
    })
}

#[test]
fn request_and_cookie_imports_do_not_install_legacy_layouts() {
    for module in [
        "std.http.Request",
        "std.http.Cookies",
        "app.Request",
        "app.Cookies",
        "std.http.Router",
        "app.Router",
    ] {
        for kind in [CoreImportKind::Module, CoreImportKind::TypeModule] {
            let mut core = http_core();
            core.imports = vec![CoreImport {
                module: module.into(),
                kind,
            }];
            assert!(http_managed_layouts(&core).unwrap().is_empty(), "{module}");
        }
    }
}

#[test]
fn retired_request_operations_have_no_native_lowering_or_inferred_type() {
    for operation in [
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
    ] {
        for arity in 0..=3 {
            let call = CoreExpr::RemoteCall {
                type_args: vec![],
                module: "$terlan.managed.http".into(),
                function: operation.into(),
                args: vec![CoreExpr::Int(-1); arity],
            };
            assert_eq!(
                managed_http_operation_type(&call),
                None,
                "{operation}/{arity}"
            );
            assert_eq!(
                lower_managed_http_operation(&call, |_| {
                    panic!("retired operation must not lower or read its arguments")
                })
                .unwrap(),
                None,
                "{operation}/{arity}"
            );
        }
    }
}
