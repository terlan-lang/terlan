use super::*;

fn projection(fields: &[usize]) -> NativeAggregateProjection {
    NativeAggregateProjection {
        module: "app".into(),
        function: "handle".into(),
        arity: 1,
        semantic: SemanticTypeId::from_canonical(RequestFieldProjection::SEMANTIC_TYPE).unwrap(),
        fields: AggregateFieldProjection::Fields(fields.iter().copied().collect()),
        scalar_entry: Some("scalar".into()),
        scalar_field: fields.first().copied(),
        suspending: true,
    }
}

#[test]
fn unrelated_aggregate_is_not_an_http_request() {
    for name in [
        "Named(app.Document)",
        "Named(Request)",
        "Named(app.Request)",
    ] {
        let mut input = projection(&[3]);
        input.semantic = SemanticTypeId::from_canonical(name).unwrap();
        assert!(admit_projection(&input).is_none());
    }
}

#[test]
fn http_adapter_preserves_verified_fields_and_suspension() {
    let output = admit_projection(&projection(&[3])).unwrap();
    assert_eq!(output.fields, RequestFieldProjection::Fields(1 << 4));
    assert_eq!(output.scalar_field, Some(4));
    assert_eq!(output.scalar_entry.as_deref(), Some("scalar"));
    assert!(output.suspending);
}

#[test]
fn out_of_contract_fields_cannot_enable_a_scalar_ingress() {
    for fields in [vec![9], vec![10], vec![11], vec![usize::MAX]] {
        let output = admit_projection(&projection(&fields)).unwrap();
        assert_eq!(output.fields, RequestFieldProjection::Complete);
        assert!(output.scalar_field.is_none());
        assert!(output.scalar_entry.is_none());
    }
    for fields in [vec![], vec![2], vec![3, 4]] {
        let output = admit_projection(&projection(&fields)).unwrap();
        assert!(output.scalar_field.is_none());
        assert!(output.scalar_entry.is_none());
    }
    let mut input = projection(&[3]);
    input.fields = AggregateFieldProjection::Complete;
    let output = admit_projection(&input).unwrap();
    assert_eq!(output.fields, RequestFieldProjection::Complete);
    assert!(output.scalar_entry.is_none());
}

#[test]
fn partial_scalar_metadata_cannot_select_a_replacement_entry() {
    for field in [None, Some(1), Some(usize::MAX)] {
        let mut input = projection(&[3]);
        input.scalar_field = field;
        let output = admit_projection(&input).unwrap();
        assert!(output.scalar_entry.is_none());
        assert!(output.scalar_field.is_none());
    }
    let mut input = projection(&[3]);
    input.scalar_entry = None;
    input.suspending = false;
    let output = admit_projection(&input).unwrap();
    assert!(output.scalar_entry.is_none());
    assert!(output.scalar_field.is_none());
    assert!(!output.suspending);
}

#[test]
fn decoded_metadata_executes_source_lookup_policy_in_a_compiled_handler() {
    use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;
    use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
    use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
    use crate::commands::serve::handler_cache::AotHandlerRuntime;
    use crate::runtime::vm::ReplValue;
    use terlan_http_native::{Request, RequestMetadata};

    let fixture = compile_native_handler_fixture(
        "source_request_metadata_policy",
        "src/app/Metadata.terl",
        "app_Metadata",
        r#"module app.Metadata.
import std.http.Request.
import std.core.Option.{with_default}.
pub handle(request: Request, key: String): String ->
    with_default(request.param(key), "<missing>") + "|" +
    with_default(request.query(key), "<missing>") + "|" +
    with_default(request.header(key), "<missing>") + "|" +
    with_default(request.cookie(key), "<missing>").
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Metadata".into(), &fixture.image, None, 1)
            .unwrap();
    let mut headers = http::HeaderMap::new();
    for value in ["header-first", "", "header-last"] {
        headers.append("key", http::HeaderValue::from_str(value).unwrap());
    }
    headers.insert("empty", http::HeaderValue::from_static(""));
    headers.insert(
        "cookie",
        http::HeaderValue::from_static("key=first; key=last; empty="),
    );
    let params = [
        ("key".into(), "param-first".into()),
        ("key".into(), "param-last".into()),
        ("empty".into(), "".into()),
    ];
    let metadata = RequestMetadata::from_http(
        RequestFieldProjection::Complete,
        &params,
        "%6Bey=first&key=&key=last&KEY=upper&empty=&%E2%84%AAey=unicode",
        &headers,
    );
    let value = vm_request_descriptor_owned(
        Request::from_parts_with_raw_query_metadata("GET", "/", "", metadata).into_parts(),
        RequestFieldProjection::Complete,
    );
    for (key, expected) in [
        ("key", "param-last|last|header-last|first"),
        ("KEY", "<missing>|upper|header-last|<missing>"),
        ("empty", "|||"),
        ("absent", "<missing>|<missing>|<missing>|<missing>"),
        ("\u{212a}ey", "<missing>|unicode|<missing>|<missing>"),
    ] {
        let AotHandlerInvocationStep::Complete(actual) = runtime
            .begin_request_invocation(
                "app.Metadata",
                "handle",
                vec![value.clone(), ReplValue::String(key.into())],
            )
            .unwrap()
        else {
            panic!("lookups must execute Terlan without native dispatch")
        };
        assert_eq!(actual, ReplValue::String(expected.into()), "{key}");
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
