//! Tests for compiler-owned managed HTTP value lowering.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    decode_aggregate_layout, decode_collection_layout,
    encode_string_prepend_projected_literal_operation, managed_abi_result_is_reference,
    ManagedCollectionKind, SemanticTypeId,
};
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, CoreCaseClause, CoreEffectSet, CoreExpr, CoreImport,
    CoreImportKind, CoreModule, CorePattern,
};

use super::http_values::{
    http_managed_collections, http_managed_layouts, install_http_constructors, lower_http_values,
    lower_managed_http_operation, managed_http_operation_type,
};
use super::{NativeExpr, NativeType};

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

/// Verifies ordinary response builders become one managed construction operation.
#[test]
fn response_builders_lower_to_fused_managed_operations() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "std.http.Response".to_string(),
        function: "text".to_string(),
        args: vec![CoreExpr::Binary("\"hello\"".to_string())],
    };
    lower_http_values(&mut core).expect("lower response");

    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, args }
            if module == "$terlan.managed.http"
                && function == "response_build_0"
                && args == &vec![
                    CoreExpr::Binary("\"hello\"".to_string()),
                    CoreExpr::Int(200),
                ]
    ));
}

#[test]
fn middleware_continue_atom_lowers_to_the_compiler_owned_constructor() {
    let mut core = http_core();
    core.imports.push(CoreImport {
        module: "std.http.Router".to_string(),
        kind: CoreImportKind::Module,
    });
    *body(&mut core) = CoreExpr::Atom("continue".to_string());
    lower_http_values(&mut core).expect("lower middleware continuation");

    assert_eq!(
        body(&mut core),
        &CoreExpr::ConstructorCall {
            constructor: "std.http.Router.Continue".to_string(),
            constructor_identity: Some("std.http.Router.Continue".to_string()),
            args: Vec::new(),
        }
    );
}

/// Verifies every admitted HTTP aggregate and collection has closed metadata.
#[test]
fn complete_http_managed_boundary_inventory_is_closed_and_decodable() {
    let core = complete_http_core();
    let mut constructors = HashMap::new();
    install_http_constructors(&core, &mut constructors).expect("install constructors");
    assert_eq!(constructors.len(), 7);

    let layouts = http_managed_layouts(&core).expect("HTTP layouts");
    assert_eq!(layouts.len(), 12);
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
        ("Named(Request)", 1),
        ("Named(Jar)", 1),
        ("Apply(Option;String)", 2),
        ("Named(Json)", 1),
        ("Named(Error)", 1),
        ("Apply(Result;Named(Json),Named(Error))", 2),
        ("Named(Session)", 1),
        ("Named(Response)", 1),
        ("std.http.Response.Header", 1),
        ("Named(HttpError)", 1),
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

    let collections = http_managed_collections(&core).expect("HTTP collections");
    assert_eq!(collections.len(), 3);
    assert_eq!(
        collections
            .iter()
            .map(|collection| decode_collection_layout(collection)
                .expect("HTTP collection")
                .kind())
            .collect::<Vec<_>>(),
        vec![
            ManagedCollectionKind::Map,
            ManagedCollectionKind::List,
            ManagedCollectionKind::List,
        ]
    );
}

/// Verifies every portable request read becomes one typed managed operation.
#[test]
fn request_accessors_lower_to_checked_managed_operations() {
    for (method, arity, expected) in [
        ("method", 1, NativeType::StringRef),
        ("path", 1, NativeType::StringRef),
        ("query_string", 1, NativeType::StringRef),
        ("body_text", 1, NativeType::StringRef),
        (
            "body_json",
            1,
            NativeType::ManagedRef(
                crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                    "Apply(Result;Named(Json),Named(Error))",
                )
                .expect("body JSON result semantic"),
            ),
        ),
        (
            "param",
            2,
            NativeType::ManagedRef(
                crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                    "Apply(Option;String)",
                )
                .expect("option semantic"),
            ),
        ),
        (
            "query",
            2,
            NativeType::ManagedRef(
                crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                    "Apply(Option;String)",
                )
                .expect("option semantic"),
            ),
        ),
        (
            "header",
            2,
            NativeType::ManagedRef(
                crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                    "Apply(Option;String)",
                )
                .expect("option semantic"),
            ),
        ),
        (
            "cookie",
            2,
            NativeType::ManagedRef(
                crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                    "Apply(Option;String)",
                )
                .expect("option semantic"),
            ),
        ),
    ] {
        let mut core = http_core();
        let mut args = vec![CoreExpr::Var("request".to_string())];
        if arity == 2 {
            args.push(CoreExpr::Binary("\"key\"".to_string()));
        }
        *body(&mut core) = CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: method.to_string(),
            args,
        };
        lower_http_values(&mut core).expect("lower request accessor");
        assert_eq!(managed_http_operation_type(body(&mut core)), Some(expected));
        let lowered = lower_managed_http_operation(body(&mut core), |argument| match argument {
            CoreExpr::Var(_) => Ok(NativeExpr::Param(0)),
            CoreExpr::Binary(_) => Ok(NativeExpr::ManagedLiteral {
                encoded: Arc::from(b"test".as_slice()),
            }),
            other => panic!("unexpected operation argument: {other:?}"),
        })
        .expect("lower managed operation")
        .expect("managed operation");
        assert!(matches!(lowered, NativeExpr::ManagedOperation { .. }));
    }
}

#[test]
fn module_owned_request_accessor_lowers_to_checked_managed_operation() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "std.http.Request".to_string(),
        function: "query_string".to_string(),
        args: vec![CoreExpr::Var("request".to_string())],
    };

    lower_http_values(&mut core).expect("lower module-owned request accessor");

    assert_eq!(
        managed_http_operation_type(body(&mut core)),
        Some(NativeType::StringRef)
    );
}

#[test]
fn linked_request_accessor_lowers_to_checked_managed_operation() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Call {
        function: "std.http.Request.query_string".to_string(),
        args: vec![CoreExpr::Var("request".to_string())],
    };

    lower_http_values(&mut core).expect("lower linked request accessor");

    assert_eq!(
        managed_http_operation_type(body(&mut core)),
        Some(NativeType::StringRef)
    );
}

/// Literal-prefix concatenation is one managed operation with no literal heap allocation.
#[test]
fn literal_prefix_string_append_fuses_into_one_managed_operation() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::BinaryOp {
        operator: "+".to_string(),
        left: Box::new(CoreExpr::Binary("\"prefix:\\u03bb\"".to_string())),
        right: Box::new(CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: "body_text".to_string(),
            args: vec![CoreExpr::Var("request".to_string())],
        }),
    };

    lower_http_values(&mut core).expect("lower fused prefix");
    assert_eq!(
        managed_http_operation_type(body(&mut core)),
        Some(NativeType::StringRef)
    );
    let lowered = lower_managed_http_operation(body(&mut core), |argument| match argument {
        CoreExpr::Var(name) if name == "request" => Ok(NativeExpr::Param(0)),
        other => panic!("unexpected fused operation argument: {other:?}"),
    })
    .expect("lower fused operation")
    .expect("fused operation");

    let NativeExpr::ManagedOperation { encoded, args } = lowered else {
        panic!("literal prepend must lower to one managed operation");
    };
    assert_eq!(args, vec![NativeExpr::Param(0)]);
    let request_semantic = SemanticTypeId::from_canonical(
        &crate::terlan_typeck::CoreType::Named("Request".to_string()).contract_text(),
    )
    .expect("request semantic");
    assert_eq!(
        encoded.as_ref(),
        encode_string_prepend_projected_literal_operation(request_semantic, 4, "prefix:λ")
            .expect("expected fused operation")
    );
    assert!(managed_abi_result_is_reference(&encoded));
}

/// Verifies immediate request lookup matches use shared managed option operations.
#[test]
fn request_option_case_lowers_without_scalar_constructor_patterns() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: "header".to_string(),
            args: vec![CoreExpr::Var("request".to_string()), string("x-deny")],
        }),
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

    lower_http_values(&mut core).expect("lower request option case");
    let rendered = format!("{:?}", body(&mut core));
    assert!(rendered.contains("function: \"header\""), "{rendered}");
    assert!(
        rendered.contains("function: \"option_is_none\""),
        "{rendered}"
    );
    assert!(rendered.contains("function: \"option_some\""), "{rendered}");
    assert!(!rendered.contains("Case("), "{rendered}");
}

/// Managed option defaults retain their string type when nested in concatenation.
#[test]
fn request_option_default_can_be_concatenated_after_managed_case_lowering() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::BinaryOp {
        operator: "+".to_string(),
        left: Box::new(CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: "method".to_string(),
            args: vec![CoreExpr::Var("request".to_string())],
        }),
        right: Box::new(CoreExpr::RemoteCall {
            module: "std.core.Option".to_string(),
            function: "with_default".to_string(),
            args: vec![
                CoreExpr::RemoteCall {
                    module: "__receiver__".to_string(),
                    function: "header".to_string(),
                    args: vec![CoreExpr::Var("request".to_string()), string("accept")],
                },
                string("missing"),
            ],
        }),
    };

    lower_http_values(&mut core).expect("lower managed option concatenation");
    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, args }
            if module == "$terlan.managed.http"
                && function == "string_append"
                && args.len() == 2
    ));
}

/// Associative managed string expressions lower to one variadic operation.
#[test]
fn managed_string_append_chain_flattens_into_one_operation() {
    let request_field = |function: &str| CoreExpr::RemoteCall {
        module: "__receiver__".to_string(),
        function: function.to_string(),
        args: vec![CoreExpr::Var("request".to_string())],
    };
    let mut core = http_core();
    *body(&mut core) = CoreExpr::BinaryOp {
        operator: "+".to_string(),
        left: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(request_field("method")),
            right: Box::new(request_field("path")),
        }),
        right: Box::new(request_field("query_string")),
    };

    lower_http_values(&mut core).expect("lower managed string concat");
    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, args }
            if module == "$terlan.managed.http"
                && function == "string_concat"
                && args.len() == 3
    ));
}

/// Verifies immediate body decode matches become managed result operations.
#[test]
fn body_json_result_case_lowers_to_typed_managed_branches() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::RemoteCall {
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
                    module: "__receiver__".to_string(),
                    function: "body_json".to_string(),
                    args: vec![CoreExpr::Var("request".to_string())],
                },
            },
        ],
    };

    lower_http_values(&mut core).expect("lower body JSON case");
    let rendered = format!("{:?}", body(&mut core));
    assert!(rendered.contains("\"$terlan.managed.http\""), "{rendered}");
    assert!(rendered.contains("function: \"body_json\""), "{rendered}");
    assert!(
        rendered.contains("function: \"body_json_is_ok\""),
        "{rendered}"
    );
    assert!(
        rendered.contains("function: \"body_json_ok\""),
        "{rendered}"
    );
    assert!(!rendered.contains("Case("), "{rendered}");
}

/// Verifies unsupported response builders fail with an explicit typed diagnostic.
#[test]
fn unsupported_response_builder_is_rejected() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "std.http.Response".to_string(),
        function: "stream".to_string(),
        args: Vec::new(),
    };
    assert_eq!(
        lower_http_values(&mut core).unwrap_err(),
        "error[native_ir.http_response_builder]: Response.stream is not in the native managed HTTP profile"
    );
}

#[test]
fn response_json_projects_the_canonical_managed_payload() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "std.http.Response".to_string(),
        function: "json".to_string(),
        args: vec![CoreExpr::Var("json".to_string())],
    };
    lower_http_values(&mut core).expect("lower JSON response");
    let rendered = format!("{:?}", body(&mut core));
    assert!(
        rendered.contains("function: \"response_build_2\""),
        "{rendered}"
    );
    assert!(
        rendered.contains("function: \"json_payload\""),
        "{rendered}"
    );
}

#[test]
fn response_status_headers_and_raw_cookies_lower_to_persistent_operations() {
    for (method, args, expected_function) in [
        ("with_status", vec![CoreExpr::Int(204)], "response_status"),
        (
            "with_header",
            vec![
                CoreExpr::Binary("\"X-Trace\"".to_string()),
                CoreExpr::Binary("\"one\"".to_string()),
            ],
            "response_header",
        ),
        (
            "set_cookie_header",
            vec![CoreExpr::Binary("\"session=one\"".to_string())],
            "response_header",
        ),
    ] {
        let mut core = http_core();
        *body(&mut core) = CoreExpr::MutableReceiverCall {
            receiver: Box::new(CoreExpr::Var("response".to_string())),
            method: method.to_string(),
            args,
            effects: CoreEffectSet {
                effects: vec!["state".to_string()],
            },
        };
        lower_http_values(&mut core).expect("lower response mutation");
        assert!(matches!(
            body(&mut core),
            CoreExpr::RemoteCall { module, function, .. }
                if module == "$terlan.managed.http" && function == expected_function
        ));
        assert!(matches!(
            lower_managed_http_operation(body(&mut core), |argument| match argument {
                CoreExpr::Var(_) => Ok(NativeExpr::Param(0)),
                CoreExpr::Int(value) => Ok(NativeExpr::Int(*value)),
                CoreExpr::Binary(_) => Ok(NativeExpr::ManagedLiteral {
                    encoded: Arc::from(b"value".as_slice()),
                }),
                other => panic!("unexpected response operation argument: {other:?}"),
            })
            .expect("lower response operation"),
            Some(NativeExpr::ManagedOperation { .. })
        ));
    }
}

/// Canonical selected-import resolution may retain the response-qualified
/// function identity while making the receiver the first argument. That typed
/// spelling must use the same managed operation as a receiver expression.
#[test]
fn module_owned_response_method_lowers_to_persistent_operation() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::Call {
        function: "std.http.Response.with_status".to_string(),
        args: vec![CoreExpr::Var("response".to_string()), CoreExpr::Int(204)],
    };

    lower_http_values(&mut core).expect("lower module-owned response method");
    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, .. }
            if module == "$terlan.managed.http" && function == "response_status"
    ));
}

/// Verifies the complete session surface becomes managed operations.
#[test]
fn session_calls_lower_to_vm_owned_managed_operations() {
    let cases = [
        (
            CoreExpr::RemoteCall {
                module: "std.http.Session".to_string(),
                function: "current".to_string(),
                args: vec![CoreExpr::Var("request".to_string())],
            },
            "session_current",
        ),
        (
            CoreExpr::RemoteCall {
                module: "__receiver__".to_string(),
                function: "get".to_string(),
                args: vec![CoreExpr::Var("session".to_string()), string("key")],
            },
            "session_get",
        ),
        (
            CoreExpr::MutableReceiverCall {
                receiver: Box::new(CoreExpr::Var("session".to_string())),
                method: "set".to_string(),
                args: vec![string("key"), string("value")],
                effects: CoreEffectSet {
                    effects: vec!["receiver_mutation".to_string()],
                },
            },
            "session_set",
        ),
        (
            CoreExpr::RemoteCall {
                module: "std.http.Session".to_string(),
                function: "delete".to_string(),
                args: vec![CoreExpr::Var("session".to_string()), string("key")],
            },
            "session_delete",
        ),
        (
            CoreExpr::RemoteCall {
                module: "std.http.Session".to_string(),
                function: "rotate".to_string(),
                args: vec![CoreExpr::Var("session".to_string())],
            },
            "session_rotate",
        ),
        (
            CoreExpr::RemoteCall {
                module: "std.http.Session".to_string(),
                function: "expire".to_string(),
                args: vec![CoreExpr::Var("session".to_string())],
            },
            "session_expire",
        ),
        (
            CoreExpr::RemoteCall {
                module: "std.http.Session".to_string(),
                function: "with_response".to_string(),
                args: vec![
                    CoreExpr::Var("response".to_string()),
                    CoreExpr::Var("session".to_string()),
                ],
            },
            "session_with_response",
        ),
    ];
    for (expr, expected) in cases {
        let mut core = http_session_core();
        *body(&mut core) = expr;
        lower_http_values(&mut core).expect("lower session call");
        assert!(matches!(
            body(&mut core),
            CoreExpr::RemoteCall { module, function, .. }
                if module == "$terlan.managed.http" && function == expected
        ));
        assert!(matches!(
            lower_managed_http_operation(body(&mut core), |argument| match argument {
                CoreExpr::Var(_) => Ok(NativeExpr::Param(0)),
                CoreExpr::Binary(_) => Ok(NativeExpr::ManagedLiteral {
                    encoded: Arc::from(b"value".as_slice()),
                }),
                other => panic!("unexpected session argument: {other:?}"),
            })
            .expect("lower managed session operation"),
            Some(NativeExpr::ManagedOperation { .. })
        ));
    }
}

/// Verifies session imports install request, response, option, and handle layouts.
#[test]
fn session_import_installs_complete_managed_boundary_metadata() {
    let core = http_session_core();
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
    for canonical in [
        "Named(Session)",
        "Named(Request)",
        "Named(Response)",
        "Apply(Option;String)",
    ] {
        assert!(semantics
            .contains(&SemanticTypeId::from_canonical(canonical).expect("expected semantic")));
    }
    assert_eq!(
        http_managed_collections(&core)
            .expect("session collections")
            .len(),
        3
    );
}

#[test]
fn typed_cookie_jar_and_security_calls_rewrite_to_managed_operations() {
    let cases = [
        (
            "with_cookie",
            vec![string("session"), string("abc123")],
            "response_header",
        ),
        (
            "with_deleted_cookie",
            vec![string("session")],
            "response_header",
        ),
        (
            "with_cookies",
            vec![CoreExpr::Var("jar".to_string())],
            "response_cookie_jar",
        ),
        (
            "with_security_headers",
            vec![CoreExpr::Var("policy".to_string())],
            "response_security_headers",
        ),
    ];
    for (method, args, expected) in cases {
        let mut core = http_core();
        *body(&mut core) = CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: method.to_string(),
            args: [vec![CoreExpr::Var("response".to_string())], args].concat(),
        };
        lower_http_values(&mut core).expect("lower typed response update");
        assert!(matches!(
            body(&mut core),
            CoreExpr::RemoteCall { module, function, .. }
                if module == "$terlan.managed.http" && function == expected
        ));
    }

    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "std.http.Response".to_string(),
        function: "production_security_headers".to_string(),
        args: Vec::new(),
    };
    lower_http_values(&mut core).expect("lower security constructor");
    assert!(matches!(
        body(&mut core),
        CoreExpr::ConstructorCall { constructor_identity: Some(identity), args, .. }
            if identity == "$terlan.http.security_headers" && args.len() == 5
    ));

    let mut core = http_core();
    *body(&mut core) = CoreExpr::ConstructorCall {
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
    lower_http_values(&mut core).expect("lower arbitrary typed security policy");
    assert!(matches!(
        body(&mut core),
        CoreExpr::ConstructorCall { constructor_identity: Some(identity), args, .. }
            if identity == "$terlan.http.security_headers"
                && args[1] == CoreExpr::Int(1)
                && args[2] == CoreExpr::Int(0)
    ));
}

/// Verifies the private policy ABI refuses marker values outside public unions.
#[test]
fn typed_security_policy_rejects_unknown_marker() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::ConstructorCall {
        constructor: "SecurityHeaders".to_string(),
        constructor_identity: None,
        args: vec![
            CoreExpr::Atom("true".to_string()),
            CoreExpr::Atom("AllowAll".to_string()),
            CoreExpr::Atom("NoReferrer".to_string()),
            CoreExpr::Int(0),
            CoreExpr::Atom("false".to_string()),
        ],
    };
    assert_eq!(
        lower_http_values(&mut core).unwrap_err(),
        "error[native_ir.http_security_policy]: unsupported policy marker AllowAll"
    );
}

#[test]
fn direct_cookie_jar_chain_rewrites_without_a_host_handle() {
    let mut core = http_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "__receiver__".to_string(),
        function: "set".to_string(),
        args: vec![
            CoreExpr::RemoteCall {
                module: "__receiver__".to_string(),
                function: "cookies".to_string(),
                args: vec![CoreExpr::Var("request".to_string())],
            },
            string("session"),
            string("abc123"),
        ],
    };
    lower_http_values(&mut core).expect("lower jar chain");
    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, args }
            if module == "$terlan.managed.http"
                && function == "jar_append"
                && args.len() == 2
    ));
}

#[test]
fn typed_http_error_constructor_and_accessors_lower_to_managed_values() {
    let mut constructor_core = http_error_core();
    *body(&mut constructor_core) = CoreExpr::RemoteCall {
        module: "std.http.Error".to_string(),
        function: "new".to_string(),
        args: vec![
            CoreExpr::Var("code".to_string()),
            CoreExpr::Var("message".to_string()),
            CoreExpr::Var("status".to_string()),
        ],
    };
    lower_http_values(&mut constructor_core).expect("lower HTTP error constructor");
    assert!(matches!(
        body(&mut constructor_core),
        CoreExpr::ConstructorCall { constructor_identity: Some(identity), args, .. }
            if identity == "$terlan.http.error" && args.len() == 3
    ));

    let mut constructors = HashMap::new();
    install_http_constructors(&constructor_core, &mut constructors)
        .expect("install HTTP error constructor");
    assert_eq!(constructors.len(), 1);
    assert_eq!(
        http_managed_layouts(&constructor_core)
            .expect("error layouts")
            .len(),
        1
    );
    assert!(http_managed_collections(&constructor_core)
        .expect("error collections")
        .is_empty());

    for (method, expected_type, reference_result) in [
        ("code", NativeType::Atom, false),
        ("message", NativeType::StringRef, true),
        ("status", NativeType::Int, false),
    ] {
        let mut core = http_error_core();
        *body(&mut core) = CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: method.to_string(),
            args: vec![CoreExpr::Var("error".to_string())],
        };
        lower_http_values(&mut core).expect("lower HTTP error accessor");
        assert_eq!(
            managed_http_operation_type(body(&mut core)),
            Some(expected_type)
        );
        let operation = lower_managed_http_operation(body(&mut core), |_| Ok(NativeExpr::Param(0)))
            .expect("lower managed HTTP error accessor")
            .expect("managed HTTP error operation");
        let NativeExpr::ManagedOperation { encoded, args } = operation else {
            panic!("HTTP error accessor must use a managed operation");
        };
        assert_eq!(args, vec![NativeExpr::Param(0)]);
        assert_eq!(managed_abi_result_is_reference(&encoded), reference_result);
    }
}

#[test]
fn typed_http_error_operations_reject_invalid_arities() {
    let mut core = http_error_core();
    *body(&mut core) = CoreExpr::RemoteCall {
        module: "std.http.Error".to_string(),
        function: "new".to_string(),
        args: vec![CoreExpr::Var("code".to_string())],
    };
    assert_eq!(
        lower_http_values(&mut core).expect_err("invalid error arity"),
        "error[native_ir.http_error_arity]: HttpError.new received 1 arguments"
    );
}

/// Builds one CoreIR string literal for HTTP operation tests.
fn string(value: &str) -> CoreExpr {
    CoreExpr::Binary(format!("\"{value}\""))
}
