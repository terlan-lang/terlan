//! Identity policy is executable source, not a native registry retry loop.

use super::compile_native_handler_fixture;
use crate::runtime::vm::package_native_helper::{execute_call, VmPackageNativeHelpers};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::ReplValue;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use std::sync::{Arc, Mutex};
use terlan_runtime_abi::{
    BoundaryError, ErrorDomain, NativeContextBinding, NativeServices, NativeValue,
};

struct Attempts {
    candidates: usize,
    reservations: usize,
    limit: usize,
    bytes: i64,
    issued: String,
    excluded: String,
    rotating: bool,
    scenario: &'static str,
}

#[test]
fn source_controls_identity_retries_exclusion_rotation_and_exhaustion() {
    for modified in [false, true] {
        let bytes = if modified { 16 } else { 32 };
        let encoder = if modified { STANDARD } else { URL_SAFE_NO_PAD };
        let excluded = encoder.encode(vec![0; bytes]);
        let issued = encoder.encode(vec![255; bytes]);
        let source = if modified {
            include_str!("../../../../../../std/http/Session.terl")
                .replace("module std.http.Session.", "module app.IdentityPolicy.")
                // Relocated source still calls the same package service contracts.
                .replace("@compiler.native\nlookup(", "@compiler.native {std.http.session.lookup}\nlookup(")
                .replace("@compiler.native\ntry_create(", "@compiler.native {std.http.session.try_create}\ntry_create(")
                .replace("@compiler.native\ntry_rotate(", "@compiler.native {std.http.session.try_rotate}\ntry_rotate(")
                .replace("@compiler.native\nis_live(", "@compiler.native {std.http.session.is_live}\nis_live(")
                .replace("false, 16)", "false, 3)")
                .replace("true, 16)", "true, 3)")
                .replace("candidate(32)", "candidate(16)")
                .replace("encode_url_unpadded_bytes", "encode_bytes")
                + "\npub issue(rotating: Bool): String -> if { rotating -> renew(\"excluded\", 86400); true -> create(\"excluded\", 86400) }.\npub healthy(): Int -> 42."
        } else {
            r#"module app.IdentityPolicy.
import std.http.Session.
import std.http.Request.
pub issue(rotating: Bool, request: Request): Session ->
    let session = Session.current(request);
    if { rotating -> session.rotate(); true -> session }.
pub healthy(): Int -> 42.
"#
            .to_string()
        };
        let source = source.replace("\"excluded\"", &format!("\"{excluded}\""));
        let fixture = compile_native_handler_fixture(
            if modified {
                "changed_identity_policy"
            } else {
                "canonical_identity_policy"
            },
            "src/app/IdentityPolicy.terl",
            "app_IdentityPolicy",
            &source,
        );
        for rotating in [false, true] {
            for scenario in [
                "success",
                "excluded",
                "collision",
                "exhausted",
                "entropy_error",
                "encoding_error",
                "storage_error",
            ] {
                let state = Arc::new(Mutex::new(Attempts {
                    candidates: 0,
                    reservations: 0,
                    limit: if modified { 3 } else { 16 },
                    bytes: bytes as i64,
                    issued: issued.clone(),
                    excluded: excluded.clone(),
                    rotating,
                    scenario,
                }));
                let mut services = NativeServices::default();
                services
                    .register_context(
                        Arc::clone(&state),
                        [
                            NativeContextBinding::new(
                                "std.http.session.lookup",
                                1,
                                |state: &mut Attempts, args| {
                                    assert_eq!(args, &[state.excluded.clone().into()]);
                                    Ok(state.rotating.then_some(state.excluded.clone()).into())
                                },
                            ),
                            NativeContextBinding::new(
                                "std.crypto.entropy.bytes",
                                1,
                                |state: &mut Attempts, args| {
                                    assert_eq!(args, &[state.bytes.into()]);
                                    state.candidates += 1;
                                    if state.scenario == "entropy_error" {
                                        return Err(failure());
                                    }
                                    let byte =
                                        if state.scenario == "excluded" && state.candidates == 1 {
                                            0
                                        } else {
                                            255
                                        };
                                    Ok(NativeValue::Bytes(vec![byte; state.bytes as usize]))
                                },
                            ),
                            NativeContextBinding::new(
                                "std.http.session.try_create",
                                2,
                                |state: &mut Attempts, args| {
                                    assert!(!state.rotating);
                                    assert_eq!(
                                        args,
                                        &[state.issued.clone().into(), 86400_i64.into()]
                                    );
                                    reserve(state)
                                },
                            ),
                            NativeContextBinding::new(
                                "std.http.session.try_rotate",
                                3,
                                |state: &mut Attempts, args| {
                                    assert!(state.rotating);
                                    assert_eq!(
                                        args,
                                        &[
                                            state.excluded.clone().into(),
                                            state.issued.clone().into(),
                                            86400_i64.into()
                                        ]
                                    );
                                    reserve(state)
                                },
                            ),
                        ],
                    )
                    .unwrap();
                if scenario == "encoding_error" {
                    services
                        .register_context(
                            Arc::clone(&state),
                            [NativeContextBinding::new(
                                if modified {
                                    "std.encoding.base64.encode_bytes"
                                } else {
                                    "std.encoding.base64.encode_url_unpadded_bytes"
                                },
                                1,
                                |state: &mut Attempts, args| {
                                    assert_eq!(
                                        args,
                                        &[NativeValue::Bytes(vec![255; state.bytes as usize])]
                                    );
                                    Err(failure())
                                },
                            )],
                        )
                        .unwrap();
                }
                let mut shard =
                    PureNativeExecutionImage::load_with_native_services(&fixture.image, services)
                        .unwrap()
                        .spawn_shard()
                        .unwrap();
                let mut args = vec![ReplValue::Bool(rotating)];
                if !modified {
                    use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;
                    use terlan_http_native::{Request, RequestFieldProjection, RequestMetadata};
                    let request = Request::from_parts_with_raw_query_metadata(
                        "GET",
                        "/",
                        "",
                        RequestMetadata {
                            cookies: vec![format!("terlan_session={excluded}")],
                            ..Default::default()
                        },
                    );
                    args.push(vm_request_descriptor_owned(
                        request.into_parts(),
                        RequestFieldProjection::Complete,
                    ));
                }
                let mut helpers = VmPackageNativeHelpers::default();
                let result = execute_call(&mut shard, &mut helpers, "issue", &args);
                let state = state.lock().unwrap();
                match scenario {
                    "exhausted" => {
                        let error = result.unwrap_err();
                        assert!(error.contains("pure_native_effect_failure"), "{error}");
                        assert!(
                            !error.contains("could not allocate"),
                            "typed payloads are not public diagnostics"
                        );
                        assert_eq!(
                            (state.candidates, state.reservations),
                            (state.limit, state.limit)
                        );
                    }
                    "entropy_error" | "encoding_error" | "storage_error" => {
                        assert!(result.unwrap_err().contains("identity fixture failed"));
                        assert_eq!(state.candidates, 1);
                        assert_eq!(state.reservations, usize::from(scenario == "storage_error"));
                    }
                    _ => {
                        let value = result.unwrap();
                        if modified {
                            assert_eq!(value, ReplValue::String(issued.clone()));
                        } else {
                            let ReplValue::Record { fields, .. } = value else {
                                panic!("Session record");
                            };
                            assert_eq!(fields[0].1, ReplValue::String(issued.clone()));
                            assert_eq!(fields[1].1, ReplValue::String(issued.clone()));
                        }
                        assert_eq!(state.candidates, if scenario == "success" { 1 } else { 2 });
                        assert_eq!(
                            state.reservations,
                            if scenario == "collision" { 2 } else { 1 }
                        );
                    }
                }
                drop(state);
                assert_eq!(
                    execute_call(&mut shard, &mut helpers, "healthy", &[]).unwrap(),
                    ReplValue::Int(42)
                );
            }
        }
        std::fs::remove_dir_all(fixture.root).unwrap();
    }
}

fn reserve(state: &mut Attempts) -> Result<terlan_runtime_abi::NativeValue, BoundaryError> {
    state.reservations += 1;
    if state.scenario == "storage_error" {
        return Err(failure());
    }
    Ok(
        (state.scenario != "exhausted"
            && (state.scenario != "collision" || state.reservations > 1))
            .into(),
    )
}

fn failure() -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "identity test",
        "identity fixture failed",
    )
}
