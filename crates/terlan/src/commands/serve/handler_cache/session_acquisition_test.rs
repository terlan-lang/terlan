//! Source acquisition chooses allocation; context failures never become absence.

use super::compile_native_handler_fixture;
use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;
use crate::runtime::vm::package_native_helper::{execute_call, VmPackageNativeHelpers};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::ReplValue;
use std::sync::{Arc, Mutex};
use terlan_http_native::{Request, RequestFieldProjection, RequestMetadata};
use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeContextBinding, NativeServices};

struct Storage {
    found: Option<String>,
    fail: &'static str,
    calls: Vec<&'static str>,
}

#[test]
fn compiled_current_allocates_only_after_absence_and_never_retries_failures() {
    let fixture = compile_native_handler_fixture(
        "source_session_acquisition",
        "src/app/Acquire.terl",
        "app_Acquire",
        r#"module app.Acquire.
import std.http.Request.
import std.http.Session.
pub acquire(request: Request): Session -> Session.current(request).
pub healthy(): Int -> 42.
"#,
    );
    for (found, fail, expected_calls) in [
        (Some("id"), "", vec!["lookup"]),
        (None, "", vec!["lookup", "create"]),
        (None, "lookup", vec!["lookup"]),
        (None, "create", vec!["lookup", "create"]),
    ] {
        let storage = Arc::new(Mutex::new(Storage {
            found: found.map(str::to_string),
            fail,
            calls: vec![],
        }));
        let mut services = NativeServices::default();
        let bindings = [
            NativeContextBinding::new("std.http.session.lookup", 1, |state: &mut Storage, args| {
                state.calls.push("lookup");
                assert_eq!(args, &["id".into()]);
                if state.fail == "lookup" {
                    return Err(failure());
                }
                Ok(state.found.clone().into())
            }),
            NativeContextBinding::new("std.http.session.create", 2, |state: &mut Storage, args| {
                state.calls.push("create");
                assert_eq!(
                    args,
                    &["id".into(), 86400_i64.into()],
                    "exclude the untrusted requested identity"
                );
                if state.fail == "create" {
                    return Err(failure());
                }
                Ok("issued".into())
            }),
        ];
        services
            .register_context(Arc::clone(&storage), bindings)
            .unwrap();
        let mut shard =
            PureNativeExecutionImage::load_with_native_services(&fixture.image, services)
                .unwrap()
                .spawn_shard()
                .unwrap();
        let request = Request::from_parts_with_raw_query_metadata(
            "GET",
            "/",
            "",
            RequestMetadata {
                cookies: vec![("terlan_session".into(), " \u{2003}id\u{2003} ".into())],
                ..Default::default()
            },
        );
        let mut helpers = VmPackageNativeHelpers::default();
        let result = execute_call(
            &mut shard,
            &mut helpers,
            "acquire",
            &[vm_request_descriptor_owned(
                request.into_parts(),
                RequestFieldProjection::Complete,
            )],
        );
        if fail.is_empty() {
            let identity = found.unwrap_or("issued");
            assert_eq!(
                result.unwrap(),
                ReplValue::Record {
                    name: "Session".into(),
                    fields: vec![
                        ("identity".into(), ReplValue::String(identity.into())),
                        (
                            "pending_identity".into(),
                            ReplValue::String(if found.is_some() { "" } else { identity }.into())
                        ),
                        ("ttl_seconds".into(), ReplValue::Int(86400)),
                    ],
                }
            );
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("acquisition fixture failed"));
        }
        assert_eq!(storage.lock().unwrap().calls, expected_calls);
        assert_eq!(
            execute_call(&mut shard, &mut helpers, "healthy", &[]).unwrap(),
            ReplValue::Int(42)
        );
        assert_eq!(storage.lock().unwrap().calls, expected_calls);
    }
    std::fs::remove_dir_all(fixture.root).unwrap();
}

fn failure() -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "session test",
        "acquisition fixture failed",
    )
}
