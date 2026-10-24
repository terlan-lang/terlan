use super::*;
use bytes::Bytes;
use futures_util::FutureExt;
use http_body_util::Full;

use crate::request_ingress::{prepare_request, BodyStorage};

fn incoming() -> http::Request<String> {
    http::Request::builder()
        .method("PATCH")
        .uri("/users/%CE%B1?q=first&q=last&space=a+b&bad=%FF")
        .header("x-values", "first")
        .header("x-values", "last")
        .header("cookie", "sid=first; sid=last; empty=; SID=upper")
        .body("payload".into())
        .unwrap()
}

#[test]
fn complete_snapshot_preserves_wire_values_for_source_owned_lookup() {
    let mut input = incoming();
    input
        .headers_mut()
        .insert("x-bytes", http::HeaderValue::from_bytes(b"\xff").unwrap());
    let params = vec![("id".into(), "first".into()), ("id".into(), "last".into())];
    let borrowed = Request::from_http(&input, &params, Projection::Complete);
    let (parts, body) = input.into_parts();
    let owned = Request::from_http_parts(&parts, body, &params, Projection::Complete);
    assert_eq!(borrowed, owned);
    let value = owned.into_parts();
    assert_eq!(value.method, "PATCH");
    assert_eq!(value.path, "/users/%CE%B1");
    assert_eq!(value.body, "payload");
    assert_eq!(value.body_file_path, "");
    assert_eq!(value.params, params);
    assert_eq!(value.query_string, "q=first&q=last&space=a+b&bad=%FF");
    assert_eq!(value.cookies, ["sid=first; sid=last; empty=; SID=upper"]);
    assert_eq!(
        value
            .headers
            .iter()
            .filter(|(name, _)| name == "x-values")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>(),
        ["first", "last"]
    );
    assert!(value
        .headers
        .contains(&("x-bytes".into(), "\u{fffd}".into())));
}

#[test]
fn every_projection_agrees_across_borrowed_and_owned_ingress() {
    let root = tempfile::tempdir().unwrap();
    let input = incoming().map(|_| Full::new(Bytes::from_static(b"upload")));
    let mut input = prepare_request(input, 6, BodyStorage::File(root.path()))
        .now_or_never()
        .unwrap()
        .unwrap();
    // Exercise mixed projections too: the snapshot layer does not choose storage.
    *input.body_mut() = "text".into();
    let path = input
        .extensions()
        .get::<RequestBodyFile>()
        .unwrap()
        .path()
        .to_owned();
    let params = vec![("id".into(), "7".into())];
    let complete = Request::from_http(&input, &params, Projection::Complete).into_parts();
    for bits in 0..512 {
        let projection = Projection::Fields(bits << 1);
        let actual = Request::from_http(&input, &params, projection);
        let (parts, body) = input.clone().into_parts();
        assert_eq!(
            actual,
            Request::from_http_parts(&parts, body, &params, projection)
        );
        let actual = actual.into_parts();
        for (field, actual, expected) in [
            (Projection::METHOD, &actual.method, &complete.method),
            (Projection::PATH, &actual.path, &complete.path),
            (Projection::BODY, &actual.body, &complete.body),
            (
                Projection::QUERY_STRING,
                &actual.query_string,
                &complete.query_string,
            ),
            (
                Projection::BODY_FILE_PATH,
                &actual.body_file_path,
                &complete.body_file_path,
            ),
        ] {
            assert_eq!(
                actual,
                if projection.requires(field) {
                    expected.as_str()
                } else {
                    ""
                }
            );
        }
        for (field, actual, expected) in [
            (Projection::PARAMS, &actual.params, &complete.params),
            (Projection::HEADERS, &actual.headers, &complete.headers),
        ] {
            assert_eq!(
                actual.as_slice(),
                if projection.requires(field) {
                    expected.as_slice()
                } else {
                    &[]
                }
            );
        }
        assert_eq!(
            actual.cookies.as_slice(),
            if projection.requires(Projection::COOKIES) {
                complete.cookies.as_slice()
            } else {
                &[]
            }
        );
        assert!(std::path::Path::new(&path).exists());
    }
    assert_eq!(complete.body_file_path, path);
    drop(input);
    assert!(
        !std::path::Path::new(&path).exists(),
        "snapshots must not leak upload leases"
    );
}

#[test]
fn owned_body_is_transferred_and_empty_metadata_stays_empty() {
    let body = "body".repeat(1024);
    let address = body.as_ptr();
    let (parts, body) = http::Request::new(body).into_parts();
    let snapshot = Request::from_http_parts(&parts, body, &[], Projection::Complete);
    assert_eq!(
        snapshot.body().as_ptr(),
        address,
        "owned ingress must not copy body bytes"
    );
    assert!(snapshot.query_string().is_empty());
    assert!(snapshot.header_pairs().is_empty());
    assert!(snapshot.cookie_headers().is_empty());
    assert!(snapshot.body_file_path().is_empty());
}
