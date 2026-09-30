//! Real source-produced closures and package descriptors survive actor teardown.

use std::process::ExitCode;

use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::ReplValue;
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};

#[test]
fn source_closures_and_http_descriptors_cross_independent_execution_shards() {
    let directory = TestDirectory::new("closure-boundary", "source");
    let source = directory.join("callbacks.terl");
    std::fs::write(
        &source,
        r#"
module callbacks.
import std.http.Router.
import std.http.Response.
import type std.http.Request.

type Callback = (String) -> String.

pub make(prefix: String): Callback ->
    (value: String) -> prefix + value.
pub invoke(callback: Callback, value: String): String -> callback(value).
pub nested(prefix: String): Callback ->
    let inner = make(prefix);
    (value: String) -> inner(value) + "!".
pub repeated(prefix: String): List[Callback] ->
    let callback = make(prefix);
    [callback, callback].
pub make_router(prefix: String): Router ->
    Router.new().get("/source", (_request: Request) -> Response.text(prefix)).
pub extend_router(router: Router, suffix: String): Router ->
    router.post("/next", (_request: Request) -> Response.text(suffix)).
pub same_router(router: Router): Router -> router.
"#,
    )
    .unwrap();
    let output = directory.join("build");
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![source.display().to_string()]
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            },
        ),
        ExitCode::SUCCESS
    );
    let image = PureNativeExecutionImage::load(&output.join("vm/callbacks.tvm")).unwrap();
    let mut first = image.spawn_shard().unwrap();
    let callback = first
        .call("make", &[ReplValue::String("source:".into())])
        .unwrap();
    let nested = first
        .call("nested", &[ReplValue::String("nested:".into())])
        .unwrap();
    let repeated = first
        .call("repeated", &[ReplValue::String("shared:".into())])
        .unwrap();
    let router = first
        .call("make_router", &[ReplValue::String("first".into())])
        .unwrap();
    drop(first);

    let mut second = image.spawn_shard().unwrap();
    let ReplValue::List(repeated) = repeated else {
        panic!("callback list")
    };
    assert_eq!(repeated.len(), 2);
    assert_eq!(repeated[0], repeated[1]);
    for callback in repeated {
        assert_eq!(
            second
                .call("invoke", &[callback, ReplValue::String("value".into())])
                .unwrap(),
            ReplValue::String("shared:value".into())
        );
    }
    for (callback, expected) in [(callback, "source:next"), (nested, "nested:next!")] {
        assert_eq!(
            second
                .call("invoke", &[callback, ReplValue::String("next".into())])
                .unwrap(),
            ReplValue::String(expected.into())
        );
    }
    assert_eq!(
        second
            .call("same_router", std::slice::from_ref(&router))
            .unwrap(),
        router
    );
    let extended = second
        .call(
            "extend_router",
            &[router.clone(), ReplValue::String("second".into())],
        )
        .unwrap();
    assert_eq!(captures(&router), vec![ReplValue::String("first".into())]);
    assert_eq!(
        captures(&extended),
        vec![
            ReplValue::String("first".into()),
            ReplValue::String("second".into())
        ]
    );
    assert_eq!(
        second.call("same_router", &[extended.clone()]).unwrap(),
        extended
    );
}

fn captures(value: &ReplValue) -> Vec<ReplValue> {
    match value {
        ReplValue::Closure(value) => value.captures.to_vec(),
        ReplValue::Record { fields, .. } => fields
            .iter()
            .flat_map(|(_, value)| captures(value))
            .collect(),
        ReplValue::List(values) | ReplValue::Tuple(values) => {
            values.iter().flat_map(captures).collect()
        }
        _ => vec![],
    }
}
