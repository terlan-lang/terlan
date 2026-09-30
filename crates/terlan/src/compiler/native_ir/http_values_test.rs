//! Tests for compiler-owned managed HTTP value lowering.

use crate::runtime::native_image::managed::{decode_aggregate_layout, SemanticTypeId};
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, CoreCaseClause, CoreEffectSet, CoreExpr, CoreImport,
    CoreImportKind, CoreModule, CorePattern,
};

use super::http_values::{
    http_managed_layouts, lower_http_values, lower_managed_http_operation,
    managed_http_operation_type,
};
use super::NativeExpr;

#[path = "http_request_boundary_test.rs"]
mod request_boundary_test;
use request_boundary_test::map_lookup;

#[path = "http_session_library_test.rs"]
mod session_authority_test;

#[path = "http_option_library_test.rs"]
mod option_authority_test;

#[path = "http_response_builder_test.rs"]
mod builder_authority_test;
use builder_authority_test::primitive as response_primitive;

#[path = "http_middleware_library_test.rs"]
mod middleware_authority_test;

/// Creates one checked module with the standard HTTP value imports.
fn http_core() -> CoreModule {
    let syntax = parse_module_as_syntax_output("module app.Api.\n\npub handle(): Int -> 1.\n")
        .expect("parse HTTP lowering fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.imports.extend([
        CoreImport {
            module: "std.http.Request".to_string(),
            kind: CoreImportKind::TypeModule,
        },
        CoreImport {
            module: "std.http.Response".to_string(),
            kind: CoreImportKind::Module,
        },
    ]);
    core
}

/// Creates one checked module importing only the portable HTTP error contract.
fn http_error_core() -> CoreModule {
    let syntax = parse_module_as_syntax_output("module app.ErrorTest.\n\npub run(): Int -> 1.\n")
        .expect("parse HTTP error fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.imports.push(CoreImport {
        module: "std.http.Error".to_string(),
        kind: CoreImportKind::Module,
    });
    core
}

/// Creates one checked module importing the portable HTTP session contract.
fn http_session_core() -> CoreModule {
    let mut core = http_core();
    core.imports.push(CoreImport {
        module: "std.http.Session".to_string(),
        kind: CoreImportKind::Module,
    });
    core
}

/// Creates one checked module importing every managed HTTP value surface.
fn complete_http_core() -> CoreModule {
    let mut core = http_session_core();
    *body(&mut core) = CoreExpr::Tuple(vec![
        session_authority_test::primitive("current", 1),
        response_primitive("text", vec![string("body"), CoreExpr::Int(200)]),
    ]);
    core.imports.push(CoreImport {
        module: "std.http.Error".to_string(),
        kind: CoreImportKind::Module,
    });
    core
}

/// Returns the mutable body of the fixture's only function.
fn body(core: &mut CoreModule) -> &mut CoreExpr {
    core.functions[0].clauses[0]
        .body
        .core_expr
        .as_mut()
        .expect("typed body")
}

#[test]
fn retired_response_primitives_are_not_compiler_substitutes() {
    let mut core = http_core();
    for name in [
        "text",
        "html",
        "json_text",
        "file",
        "stream",
        "redirect",
        "status",
        "header",
    ] {
        let expression = response_primitive(name, vec![string("hello"), CoreExpr::Int(200)]);
        *body(&mut core) = expression.clone();
        lower_http_values(&mut core).unwrap();
        assert_eq!(body(&mut core), &expression);
        assert!(http_managed_layouts(&core).unwrap().is_empty());
    }
}

#[test]
fn router_import_does_not_replace_atoms_or_bindings_or_install_layouts() {
    let mut core = http_core();
    core.imports = vec![CoreImport {
        module: "std.http.Router".to_string(),
        kind: CoreImportKind::Module,
    }];
    for expression in [
        CoreExpr::Atom("continue".into()),
        CoreExpr::Var("Continue".into()),
    ] {
        *body(&mut core) = expression.clone();
        lower_http_values(&mut core).unwrap();
        assert_eq!(body(&mut core), &expression);
        assert!(http_managed_layouts(&core).unwrap().is_empty());
    }
}

/// Verifies every admitted HTTP aggregate and collection has closed metadata.
#[test]
fn complete_http_managed_boundary_inventory_is_closed_and_decodable() {
    let core = complete_http_core();

    let layouts = http_managed_layouts(&core).expect("HTTP layouts");
    assert_eq!(layouts.len(), 3);
    let semantics = layouts
        .iter()
        .map(|layout| {
            decode_aggregate_layout(layout)
                .expect("decode HTTP layout")
                .managed()
                .semantic_id()
        })
        .collect::<Vec<_>>();
    for (canonical, count) in [
        ("Named(Request)", 0),
        ("Named(Jar)", 0),
        ("Apply(Option;String)", 2),
        ("Named(Json)", 0),
        ("Named(Error)", 0),
        ("Apply(Result;Named(Json),Named(Error))", 0),
        ("Named(Session)", 0),
        ("Tuple(String,Bool)", 1),
        ("Named(Response)", 0),
        ("std.http.Response.Header", 0),
    ] {
        let semantic = SemanticTypeId::from_canonical(canonical).expect("inventory semantic");
        assert_eq!(
            semantics
                .iter()
                .filter(|candidate| **candidate == semantic)
                .count(),
            count,
            "unexpected layout count for {canonical}"
        );
    }
}

/// Request reads retain source ownership regardless of call spelling.
#[test]
fn request_accessors_are_not_replaced_by_http_lowering() {
    for (method, arity) in [
        ("method", 1),
        ("path", 1),
        ("query_string", 1),
        ("body_text", 1),
        ("body_file_path", 1),
        ("cookies", 1),
        ("param", 2),
        ("query", 2),
        ("header", 2),
        ("cookie", 2),
        ("body_json", 1),
    ] {
        let mut args = vec![CoreExpr::Var("request".to_string())];
        if arity == 2 {
            args.push(string("key"));
        }
        for owner in ["__receiver__", "std.http.Request", "app.Request"] {
            let original = CoreExpr::RemoteCall {
                type_args: Vec::new(),
                module: owner.to_string(),
                function: method.to_string(),
                args: args.clone(),
            };
            let mut core = http_core();
            *body(&mut core) = original.clone();
            lower_http_values(&mut core).expect("ordinary request method");
            assert_eq!(*body(&mut core), original);
            assert_eq!(managed_http_operation_type(body(&mut core)), None);
        }
        let original = CoreExpr::Call {
            type_args: Vec::new(),
            function: format!("std.http.Request.{method}"),
            args,
        };
        let mut core = http_core();
        *body(&mut core) = original.clone();
        lower_http_values(&mut core).expect("linked source method");
        assert_eq!(*body(&mut core), original);
    }
}

/// HTTP normalization must leave ordinary option patterns to generic lowering.
#[test]
fn request_option_case_retains_ordinary_constructor_patterns() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Case {
        scrutinee: Box::new(map_lookup()),
        clauses: vec![
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "Some".to_string(),
                    constructor_identity: Some("std.core.Option.Some".to_string()),
                    args: vec![CorePattern::Var("value".to_string())],
                },
                guard: None,
                body: CoreExpr::Var("value".to_string()),
            },
            CoreCaseClause {
                pattern: CorePattern::Wildcard,
                guard: None,
                body: string("missing"),
            },
        ],
    };

    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("preserve request option case");
    assert_eq!(body(&mut core), &original);
}

/// Imported option functions execute their source even around HTTP map lookups.
#[test]
fn request_option_default_retains_source_call() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::BinaryOp {
        operator: "+".to_string(),
        left: Box::new(string("prefix:")),
        right: Box::new(CoreExpr::RemoteCall {
            type_args: Vec::new(),
            module: "std.core.Option".to_string(),
            function: "with_default".to_string(),
            args: vec![map_lookup(), string("missing")],
        }),
    };

    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("preserve source option call");
    assert_eq!(body(&mut core), &original);
}

/// Body decoding and Result matching belong to ordinary library code.
#[test]
fn body_json_result_case_is_not_replaced_by_http_lowering() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::RemoteCall {
            type_args: Vec::new(),
            module: "__receiver__".to_string(),
            function: "body_json".to_string(),
            args: vec![CoreExpr::Var("request".to_string())],
        }),
        clauses: vec![
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "Ok".to_string(),
                    constructor_identity: Some("std.core.Result.Ok".to_string()),
                    args: vec![CorePattern::Var("json".to_string())],
                },
                guard: None,
                body: CoreExpr::Var("json".to_string()),
            },
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "Err".to_string(),
                    constructor_identity: Some("std.core.Result.Err".to_string()),
                    args: vec![CorePattern::Wildcard],
                },
                guard: None,
                body: CoreExpr::RemoteCall {
                    type_args: Vec::new(),
                    module: "__receiver__".to_string(),
                    function: "body_json".to_string(),
                    args: vec![CoreExpr::Var("request".to_string())],
                },
            },
        ],
    };

    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("preserve body JSON case");
    assert_eq!(body(&mut core), &original);
}

#[test]
fn retired_json_payload_projection_has_no_managed_http_lowering() {
    let mut core = http_core();
    let expression = CoreExpr::RemoteCall {
        module: "$terlan.managed.http".into(),
        function: "json_payload".into(),
        type_args: vec![],
        args: vec![CoreExpr::Var("json".into())],
    };
    *body(&mut core) = expression.clone();
    lower_http_values(&mut core).unwrap();
    assert_eq!(body(&mut core), &expression);
    assert!(managed_http_operation_type(&expression).is_none());
    assert!(
        lower_managed_http_operation(&expression, |_| panic!("must not read JSON storage"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn retired_response_managed_operations_have_no_lowering() {
    for (name, arity) in [
        ("response_build_0", 2),
        ("response_build_1", 2),
        ("response_build_2", 2),
        ("response_build_3", 2),
        ("response_status", 2),
        ("response_header", 3),
        ("empty_headers", 0),
        ("empty_chunks", 0),
        ("string_equal", 2),
        ("string_append", 2),
        ("string_concat", 3),
        ("string_prepend_literal", 2),
    ] {
        let expression = CoreExpr::RemoteCall {
            module: "$terlan.managed.http".into(),
            function: name.into(),
            type_args: vec![],
            args: vec![CoreExpr::Int(0); arity],
        };
        assert!(managed_http_operation_type(&expression).is_none());
        assert!(
            lower_managed_http_operation(&expression, |_| panic!("retired operation"))
                .unwrap()
                .is_none()
        );
    }
}

/// Qualified calls still resolve through the provider rather than a name rewrite.
#[test]
fn module_owned_response_method_retains_source_call() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Call {
        type_args: Vec::new(),
        function: "std.http.Response.status".to_string(),
        args: vec![CoreExpr::Var("response".to_string()), CoreExpr::Int(204)],
    };

    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("retain module-owned response method");
    assert_eq!(body(&mut core), &original);
}

/// Verifies explicit session primitives install their remaining compatibility metadata.
#[test]
fn session_primitive_installs_complete_managed_boundary_metadata() {
    let mut core = http_session_core();
    *body(&mut core) = session_authority_test::primitive("current", 1);
    let layouts = http_managed_layouts(&core).expect("session layouts");
    let semantics = layouts
        .iter()
        .map(|encoded| {
            decode_aggregate_layout(encoded)
                .expect("decode session layout")
                .managed()
                .semantic_id()
        })
        .collect::<Vec<_>>();
    assert!(semantics.contains(
        &SemanticTypeId::from_canonical("Apply(Option;String)").expect("option semantic")
    ));
    assert!(!semantics.contains(
        &SemanticTypeId::from_canonical("Named(Session)").expect("source session semantic")
    ));
}

#[test]
fn cookie_jar_replay_and_security_policy_remain_source_calls() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: "__receiver__".to_string(),
        function: "with_cookies".to_string(),
        args: vec![
            CoreExpr::Var("response".into()),
            CoreExpr::Var("jar".into()),
        ],
    };
    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("source cookie jar application");
    assert_eq!(body(&mut core), &original);

    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: "std.http.Response".to_string(),
        function: "production_security_headers".to_string(),
        args: Vec::new(),
    };
    lower_http_values(&mut core).expect("retain source security policy");
    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, args, .. }
            if module == "std.http.Response" && function == "production_security_headers" && args.is_empty()
    ));

    let mut core = http_core();
    *body(&mut core) = CoreExpr::ConstructorCall {
        type_args: Vec::new(),
        constructor: "std.http.Response.SecurityHeaders".to_string(),
        constructor_identity: None,
        args: vec![
            CoreExpr::Atom("true".to_string()),
            CoreExpr::Atom("SameOrigin".to_string()),
            CoreExpr::Atom("NoReferrer".to_string()),
            CoreExpr::Int(60),
            CoreExpr::Atom("false".to_string()),
        ],
    };
    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("retain source security policy constructor");
    assert_eq!(body(&mut core), &original);
    assert!(matches!(
        body(&mut core),
        CoreExpr::ConstructorCall { constructor, args, .. }
            if constructor == "std.http.Response.SecurityHeaders"
                && args[1] == CoreExpr::Atom("SameOrigin".into())
                && args[2] == CoreExpr::Atom("NoReferrer".into())
    ));
}

#[test]
fn direct_cookie_jar_chain_is_not_replaced_by_http_lowering() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: "__receiver__".to_string(),
        function: "set".to_string(),
        args: vec![
            CoreExpr::RemoteCall {
                type_args: Vec::new(),
                module: "$terlan.managed.http".to_string(),
                function: "cookies".to_string(),
                args: vec![CoreExpr::Var("request".to_string())],
            },
            string("session"),
            string("abc123"),
        ],
    };
    let original = body(&mut core).clone();
    lower_http_values(&mut core).expect("retain jar chain");
    assert_eq!(body(&mut core), &original);
}

#[test]
fn resolved_cookie_calls_follow_source_for_jars_and_codecs() {
    for (name, args, expected) in [
        (
            "get",
            vec![CoreExpr::Var("jar".into()), string("session")],
            "get",
        ),
        (
            "set",
            vec![
                CoreExpr::Var("jar".into()),
                string("session"),
                string("abc123"),
            ],
            "set",
        ),
        (
            "delete",
            vec![CoreExpr::Var("jar".into()), string("session")],
            "delete",
        ),
        (
            "set_header",
            vec![string("session"), string("abc123")],
            "set_header",
        ),
        (
            "set_header_with_options",
            vec![string("session"), string("abc123")],
            "set_header_with_options",
        ),
        ("delete_header", vec![string("session")], "delete_header"),
    ] {
        let mut core = http_core();
        *body(&mut core) = CoreExpr::Call {
            type_args: Vec::new(),
            function: format!("std.http.Cookies.{name}"),
            args,
        };
        lower_http_values(&mut core).expect("lower resolved cookie call");
        assert!(
            matches!(body(&mut core), CoreExpr::Call { function, .. }
            if function == &format!("std.http.Cookies.{expected}")),
            "{name}"
        );
    }
}

#[test]
fn http_lowering_does_not_supply_cookie_specific_mutation_semantics() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Let {
        bindings: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var("done".into()),
            value: CoreExpr::Call {
                type_args: Vec::new(),
                function: "std.http.Cookies.set".into(),
                args: vec![
                    CoreExpr::Var("jar".into()),
                    string("session"),
                    string("new"),
                ],
            },
        }],
        body: Box::new(CoreExpr::Var("jar".into())),
    };
    lower_http_values(&mut core).unwrap();
    let once = body(&mut core).clone();
    lower_http_values(&mut core).unwrap();
    assert_eq!(body(&mut core), &once);
    let CoreExpr::Let { bindings, .. } = once else {
        panic!("lexical mutation")
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].pattern, CorePattern::Var("done".into()));
    assert!(matches!(&bindings[0].value, CoreExpr::Call { function, .. }
        if function == "std.http.Cookies.set"));
}

#[test]
fn cookie_native_serializer_body_keeps_its_package_contract() {
    let mut core = http_core();
    core.module = "std.http.Cookies".into();
    core.imports.clear();
    *body(&mut core) = CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall {
        id: crate::terlan_typeck::CoreIntrinsicId::NativeOperation {
            operation: "std.http.cookies.set_header_with_options".into(),
            parameter_types: vec![crate::terlan_typeck::CoreType::String; 2],
        },
        args: vec![string("session"), string("value")],
        return_type: crate::terlan_typeck::CoreType::String,
        effects: CoreEffectSet {
            effects: vec!["native-package".into()],
        },
        span: crate::terlan_syntax::span::Span::new(0, 0),
    });
    let original = body(&mut core).clone();
    lower_http_values(&mut core).unwrap();
    assert_eq!(body(&mut core), &original);
}

#[test]
fn http_error_calls_remain_source_owned() {
    for function in ["new", "code", "message", "status"] {
        let mut core = http_error_core();
        *body(&mut core) = CoreExpr::RemoteCall {
            type_args: Vec::new(),
            module: "std.http.Error".into(),
            function: function.into(),
            args: vec![CoreExpr::Var("value".into())],
        };
        let original = body(&mut core).clone();
        lower_http_values(&mut core).expect("ordinary source call");
        assert_eq!(body(&mut core), &original);
        assert!(http_managed_layouts(&core).unwrap().is_empty());
    }
}

/// Builds one CoreIR string literal for HTTP operation tests.
fn string(value: &str) -> CoreExpr {
    CoreExpr::Binary(format!("\"{value}\""))
}
