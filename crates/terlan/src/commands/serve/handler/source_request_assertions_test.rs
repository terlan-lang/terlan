use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;
use terlan_http_native::{Request, RequestFieldProjection, RequestMetadata};

#[test]
fn source_request_assertions_observe_every_field_and_reject_counterexamples() {
    let fixture = compile_native_handler_fixture(
        "source_request_assertions",
        "std/http/RequestTest.terl",
        "std_http_RequestTest",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/http/RequestTest.terl"
        )),
    );
    let export = "std.http.RequestTest.request_accessors_report_expected_values";
    let mut shard =
        PureNativeExecutionShard::load_image(&fixture.image).expect("load request fixture");
    let owner = shard
        .spawn_fixed_owner_actor(export, 1)
        .expect("admit request fixture");
    for changed in [
        "none", "method", "path", "params", "query", "headers", "cookies", "body",
    ] {
        let mut request = Request::from_parts_with_raw_query_metadata(
            "POST",
            "/items/42",
            "{\"item\":42}",
            RequestMetadata {
                params: vec![("id".into(), "42".into())],
                query_string: "page=2".into(),
                headers: vec![("accept".into(), "application/json".into())],
                cookies: vec!["session=test-session".into()],
            },
        )
        .into_parts();
        match changed {
            "method" => request.method = "GET".into(),
            "path" => request.path = "/other".into(),
            "params" => request.params.clear(),
            "query" => request.query_string.clear(),
            "headers" => request.headers.clear(),
            "cookies" => request.cookies.clear(),
            "body" => request.body = "different".into(),
            _ => {}
        }
        let argument = super::request_materialization::vm_request_descriptor_owned(
            request,
            RequestFieldProjection::Complete,
        );
        let observed = shard
            .call_on_admitted_fixed_owner(owner, export, &[argument])
            .expect("execute compiled request assertions");
        assert_eq!(
            observed,
            ReplValue::Bool(changed == "none"),
            "changed {changed}"
        );
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).expect("remove request fixture");
}
