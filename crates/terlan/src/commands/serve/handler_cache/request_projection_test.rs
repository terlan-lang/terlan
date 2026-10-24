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
    for fields in [vec![8], vec![9], vec![10], vec![11], vec![usize::MAX]] {
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
    let evaluate = |value: ReplValue, key: &str| {
        let ReplValue::Record { ref fields, .. } = value else {
            panic!("request record")
        };
        let query = fields[4].1.clone();
        let mut step = runtime
            .begin_request_invocation(
                "app.Metadata",
                "handle",
                vec![value, ReplValue::String(key.into())],
            )
            .unwrap();
        if query != ReplValue::String(String::new()) {
            let AotHandlerInvocationStep::CapabilityWaiting(ref invocation) = step else {
                panic!("query decoding must be selected by source");
            };
            assert_eq!(
                invocation.request().unwrap().package_arguments.as_deref(),
                Some(std::slice::from_ref(&query))
            );
            step = super::super::handler_cache_test_support::resume_value_codec(
                step,
                &mut crate::runtime::vm::package_native_helper::VmPackageNativeHelpers::default(),
                "std.net.uri.query_pairs",
            );
        }
        if matches!(step, AotHandlerInvocationStep::CapabilityWaiting(_)) {
            step = crate::commands::serve::handler_cache::handler_cache_test_support::resume_cookie_decode(
                step, &mut crate::runtime::vm::package_native_helper::VmPackageNativeHelpers::default(),
            );
        }
        let AotHandlerInvocationStep::Complete(actual) = step else {
            panic!("lookups must complete after the selected cookie codec")
        };
        actual
    };
    let mut headers = http::HeaderMap::new();
    for value in ["header-first", "", "header-last"] {
        headers.append("key", http::HeaderValue::from_str(value).unwrap());
    }
    headers.insert("empty", http::HeaderValue::from_static(""));
    headers.insert(
        "cookie",
        http::HeaderValue::from_static("key=first; key=last; empty="),
    );
    headers.append("cookie", http::HeaderValue::from_static("key=ignored"));
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
        assert_eq!(
            evaluate(value.clone(), key),
            ReplValue::String(expected.into()),
            "{key}"
        );
    }
    for (first, expected) in [
        (None, "<missing>"),
        (Some(b"".as_slice()), "<missing>"),
        (Some(b"invalid".as_slice()), "<missing>"),
        (Some(b"key=\xff".as_slice()), "<missing>"),
        (Some(b"key=".as_slice()), ""),
        (Some(b"key=first; key=duplicate".as_slice()), "first"),
    ] {
        let mut headers = http::HeaderMap::new();
        if let Some(first) = first {
            headers.append("cookie", http::HeaderValue::from_bytes(first).unwrap());
            headers.append("cookie", http::HeaderValue::from_static("key=later"));
        }
        let projection = RequestFieldProjection::Fields(1 << RequestFieldProjection::COOKIES);
        let metadata = RequestMetadata::from_http(projection, &[], "", &headers);
        let value = vm_request_descriptor_owned(
            Request::from_parts_with_raw_query_metadata("GET", "/", "", metadata).into_parts(),
            projection,
        );
        assert_eq!(
            evaluate(value, "key"),
            ReplValue::String(format!("<missing>|<missing>|<missing>|{expected}"))
        );
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn cookie_header_selection_executes_in_a_renamed_source_request_module() {
    use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;
    use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
    use crate::commands::serve::handler_cache::AotHandlerRuntime;
    use crate::runtime::vm::ReplValue;
    use crate::support::test_fs::TestDirectory;
    use crate::{CliCommand, CliState};
    use terlan_http_native::{Request, RequestMetadata};

    let directory = TestDirectory::new("serve", "source_cookie_header_selection");
    std::fs::create_dir_all(directory.join("app")).unwrap();
    let policy = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../std/http/Request.terl"
    ))
    .unwrap();
    let first = "[header | _] -> Cookies.parse(header)";
    assert!(policy.contains(first));
    std::fs::write(
        directory.join("app/RequestPolicy.terl"),
        policy.replace("module std.http.Request.", "module app.RequestPolicy.").replace(
            first,
            "[header | rest] -> case rest { [] -> Cookies.parse(header); [second | _] -> Cookies.parse(second) }",
        ),
    )
    .unwrap();
    std::fs::write(
        directory.join("app/Main.terl"),
        r#"module app.Main.
import app.RequestPolicy.
import type app.RequestPolicy.Request.
import std.core.Option.{with_default}.
pub handle(request: Request): String -> with_default(request.cookie("key"), "missing").
pub unread(request: Request): String -> request.path().
"#,
    )
    .unwrap();
    let output = directory.join("build");
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![directory.as_ref().display().to_string()],
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            },
        ),
        std::process::ExitCode::SUCCESS
    );
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.Main".into(),
        &output.join("vm/app_Main.tvm"),
        None,
        1,
    )
    .unwrap();
    for (values, expected) in [
        (vec![], "missing"),
        (vec!["key=one; key=ignored"], "one"),
        (vec!["key=one", "key=two; key=ignored", "key=three"], "two"),
        (vec!["", "key=two"], "two"),
        (vec!["invalid", "key=two"], "two"),
        (vec!["key=one", ""], "missing"),
    ] {
        let selected = values.get(1).or(values.first()).copied();
        let mut headers = http::HeaderMap::new();
        for value in &values {
            headers.append("cookie", http::HeaderValue::from_str(value).unwrap());
        }
        let projection = RequestFieldProjection::Fields(1 << RequestFieldProjection::COOKIES);
        let metadata = RequestMetadata::from_http(projection, &[], "", &headers);
        let value = vm_request_descriptor_owned(
            Request::from_parts_with_raw_query_metadata("GET", "/", "", metadata).into_parts(),
            projection,
        );
        let unread = runtime
            .begin_request_invocation("app.Main", "unread", vec![value.clone()])
            .unwrap();
        assert!(
            matches!(unread, AotHandlerInvocationStep::Complete(_)),
            "unobserved cookies must not call the decoder"
        );
        let mut step = runtime
            .begin_request_invocation("app.Main", "handle", vec![value])
            .unwrap();
        if let Some(selected) = selected {
            let AotHandlerInvocationStep::CapabilityWaiting(ref invocation) = step else {
                panic!("selected raw header must reach the decoder");
            };
            assert_eq!(
                invocation.request().unwrap().package_arguments.as_deref(),
                Some([ReplValue::String(selected.into())].as_slice())
            );
            step = super::super::handler_cache_test_support::resume_cookie_decode(
                step,
                &mut crate::runtime::vm::package_native_helper::VmPackageNativeHelpers::default(),
            );
        }
        let AotHandlerInvocationStep::Complete(actual) = step else {
            panic!("only the selected header may be decoded");
        };
        assert_eq!(actual, ReplValue::String(expected.into()));
    }
}

#[test]
fn query_decoding_is_lazy_and_preserves_maintained_codec_semantics() {
    use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;
    use crate::commands::serve::handler_cache::handler_cache_test_support::{
        compile_native_handler_fixture, resume_value_codec,
    };
    use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
    use crate::commands::serve::handler_cache::AotHandlerRuntime;
    use crate::runtime::vm::{package_native_helper::VmPackageNativeHelpers, ReplValue};
    use terlan_http_native::{Request, RequestMetadata};

    let fixture = compile_native_handler_fixture(
        "source_query_decode",
        "src/app/Query.terl",
        "app_Query",
        r#"module app.Query.
import std.http.Request.
import std.core.Option.{with_default}.
pub raw(request: Request): String -> request.query_string().
pub decoded(request: Request, key: String): String -> with_default(request.query(key), "<missing>").
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Query".into(), &fixture.image, None, 1)
            .unwrap();
    let projection = runtime.direct_request_projection("app.Query", "decoded", 2);
    assert!(projection.requires(RequestFieldProjection::QUERY_STRING));
    for (query, key, expected) in [
        ("", "key", "<missing>"),
        ("&&", "key", "<missing>"),
        ("key=first&key=&key=last&KEY=other", "key", "last"),
        ("key=first&key=", "key", ""),
        ("flag&=empty&eq=a=b", "flag", ""),
        ("flag&=empty&eq=a=b", "", "empty"),
        ("flag&=empty&eq=a=b", "eq", "a=b"),
        ("a%26b=c%3Dd", "a&b", "c=d"),
        ("plus=%2B+%20", "plus", "+  "),
        ("bad=%GG%", "bad", "%GG%"),
        ("utf8=%FF", "utf8", "\u{fffd}"),
        ("zero=%00", "zero", "\0"),
    ] {
        let metadata = RequestMetadata::from_http(projection, &[], query, &http::HeaderMap::new());
        let value = vm_request_descriptor_owned(
            Request::from_parts_with_raw_query_metadata("GET", "/", "", metadata).into_parts(),
            projection,
        );
        let raw = runtime
            .begin_request_invocation("app.Query", "raw", vec![value.clone()])
            .unwrap();
        let AotHandlerInvocationStep::Complete(actual) = raw else {
            panic!("raw query reads must not decode")
        };
        assert_eq!(actual, ReplValue::String(query.into()));
        let mut step = runtime
            .begin_request_invocation(
                "app.Query",
                "decoded",
                vec![value, ReplValue::String(key.into())],
            )
            .unwrap();
        if !query.is_empty() {
            let AotHandlerInvocationStep::CapabilityWaiting(ref invocation) = step else {
                panic!("source must request query decoding")
            };
            assert_eq!(
                invocation.request().unwrap().package_arguments.as_deref(),
                Some([ReplValue::String(query.into())].as_slice())
            );
            step = resume_value_codec(
                step,
                &mut VmPackageNativeHelpers::default(),
                "std.net.uri.query_pairs",
            );
        }
        let AotHandlerInvocationStep::Complete(actual) = step else {
            panic!("one decode must complete the lookup")
        };
        assert_eq!(actual, ReplValue::String(expected.into()), "{query}: {key}");
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
