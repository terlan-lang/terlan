use super::*;
use http::HeaderValue;

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.append("X-Mode", HeaderValue::from_static("first"));
    headers.append("X-Mode", HeaderValue::from_static("second"));
    headers.insert("x-bytes", HeaderValue::from_bytes(b"\xff").unwrap());
    headers.append(
        "cookie",
        HeaderValue::from_static("user=Ada; user=Grace; empty="),
    );
    headers.append("cookie", HeaderValue::from_static("ignored=second-header"));
    headers
}

#[test]
fn metadata_projection_preserves_exact_observed_fields_for_every_mask() {
    let params = vec![("id".into(), "7".into()), ("id".into(), "8".into())];
    let query = "name=Ada+Lovelace&name=Grace%20Hopper&empty=";
    let headers = headers();
    let complete = RequestMetadata::from_http(Projection::Complete, &params, query, &headers);
    assert_eq!(complete.params, params);
    assert_eq!(complete.query_string, query);
    assert_eq!(
        complete.cookies,
        ["user=Ada; user=Grace; empty=", "ignored=second-header"]
    );
    for mask in 0..(1 << 11) {
        let projection = Projection::Fields(mask);
        let actual = RequestMetadata::from_http(projection, &params, query, &headers);
        for (field, observed, expected) in [
            (Projection::PARAMS, &actual.params, &complete.params),
            (Projection::HEADERS, &actual.headers, &complete.headers),
        ] {
            assert_eq!(
                observed.as_slice(),
                if projection.requires(field) {
                    expected.as_slice()
                } else {
                    &[]
                },
                "mask {mask}, field {field}"
            );
        }
        assert_eq!(
            actual.query_string,
            if projection.requires(Projection::QUERY_STRING) {
                query
            } else {
                ""
            }
        );
        assert_eq!(
            actual.cookies.as_slice(),
            if projection.requires(Projection::COOKIES) {
                complete.cookies.as_slice()
            } else {
                &[]
            }
        );
    }
}

#[test]
fn header_adapter_preserves_repetition_and_existing_lossy_text_contract() {
    let pairs = request_header_pairs(&headers());
    let repeated: Vec<_> = pairs
        .iter()
        .filter(|(name, _)| name == "x-mode")
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(repeated, ["first", "second"]);
    assert!(pairs.contains(&("x-bytes".into(), "\u{fffd}".into())));
    assert!(pairs
        .iter()
        .all(|(name, _)| name == &name.to_ascii_lowercase()));
    assert!(request_header_pairs(&HeaderMap::new()).is_empty());
}

#[test]
fn missing_or_nontext_cookie_headers_do_not_become_cookie_values() {
    assert!(request_cookie_headers(&HeaderMap::new()).is_empty());
    let mut headers = HeaderMap::new();
    headers.append("cookie", HeaderValue::from_bytes(b"user=\xff").unwrap());
    headers.append("cookie", HeaderValue::from_static("user=later"));
    assert_eq!(request_cookie_headers(&headers), ["", "user=later"]);
    headers.insert("cookie", HeaderValue::from_static("; =bad; flag; good=yes"));
    assert_eq!(request_cookie_headers(&headers), ["; =bad; flag; good=yes"]);
}
