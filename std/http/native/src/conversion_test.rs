use super::*;

#[test]
fn cookie_projection_preserves_first_lookup_without_changing_parser_output() {
    let cookies =
        parse_request_cookie_header("sid=first; empty=; sid=second; SID=upper; empty=later");
    let request = Request::from_parts_with_raw_query_metadata(
        "GET",
        "/",
        "",
        RequestMetadata {
            cookies: cookies.clone(),
            ..Default::default()
        },
    );
    assert_eq!(request.cookie_pairs(), cookies);
    assert_eq!(request.cookie("sid").as_deref(), Some("first"));
    assert_eq!(request.cookie("SID").as_deref(), Some("upper"));
    assert_eq!(request.cookie("empty").as_deref(), Some(""));
    assert_eq!(request.cookie("absent"), None);
    let first_value = request.cookie_pairs()[0].1.as_ptr();
    let parts = request.into_parts();
    assert_eq!(
        parts.cookies,
        [
            ("sid".into(), "first".into()),
            ("empty".into(), "".into()),
            ("SID".into(), "upper".into()),
        ]
    );
    assert_eq!(parts.cookies[0].1.as_ptr(), first_value);
    assert!(Request::from_parts("GET", "/", "")
        .into_parts()
        .cookies
        .is_empty());
}

#[test]
fn request_parts_transfer_all_metadata_without_cloning() {
    let metadata = RequestMetadata {
        params: vec![("id".into(), "42".into())],
        query_string: "tag=a&tag=&tag=b".into(),
        query: vec![("tag".into(), "a".into()), ("tag".into(), "".into())],
        headers: vec![("x-tag".into(), "a".into()), ("x-tag".into(), "b".into())],
        cookies: vec![("session".into(), "abc".into())],
    };
    let request = Request::from_parts_with_raw_query_metadata(
        "POST",
        "/items/42",
        "payload",
        metadata.clone(),
    )
    .with_body_file_path("/tmp/upload");
    assert_eq!(request.query_pairs(), metadata.query);
    assert_eq!(request.header_pairs(), metadata.headers);
    let body_ptr = request.body().as_ptr();
    let query_ptr = request.query_pairs().as_ptr();
    let headers_ptr = request.header_pairs().as_ptr();
    let parts = request.into_parts();
    assert_eq!(parts.method, "POST");
    assert_eq!(parts.path, "/items/42");
    assert_eq!(parts.body, "payload");
    assert_eq!(parts.body_file_path, "/tmp/upload");
    assert_eq!(parts.params, metadata.params);
    assert_eq!(parts.query_string, metadata.query_string);
    assert_eq!(parts.query, metadata.query);
    assert_eq!(parts.headers, metadata.headers);
    assert_eq!(parts.cookies, metadata.cookies);
    assert_eq!(parts.body.as_ptr(), body_ptr);
    assert_eq!(parts.query.as_ptr(), query_ptr);
    assert_eq!(parts.headers.as_ptr(), headers_ptr);
}

#[test]
fn response_conversion_rejects_invalid_metadata() {
    for status in [i64::MIN, -1, 0, 99, 1000, 65536, i64::MAX] {
        let error = text("body", status).to_http_response().unwrap_err();
        assert_eq!(error.code(), "http.response.invalid_status");
    }
    for invalid in ["text/plain\r\nx-injected: yes", "text/plain\0"] {
        let error = Response::from_parts(200, invalid, "body")
            .to_http_response()
            .unwrap_err();
        assert_eq!(error.code(), "http.response.invalid_content_type");
        let mut response = text("body", 200);
        header(&mut response, "x-value", invalid);
        assert_eq!(
            response.to_http_response().unwrap_err().code(),
            "http.response.invalid_header"
        );
    }
    for status in [100, 200, 999] {
        assert_eq!(
            text("body", status)
                .to_http_response()
                .unwrap()
                .status()
                .as_u16(),
            status as u16
        );
    }
}

#[test]
fn response_conversion_preserves_repeated_headers() {
    let mut response = text("body", 200);
    header(&mut response, "Set-Cookie", "a=1");
    header(&mut response, "Set-Cookie", "b=2");
    header(&mut response, "content-type", "application/custom");
    let converted = response.to_http_response().unwrap();
    let cookies: Vec<_> = converted.headers().get_all("set-cookie").iter().collect();
    assert_eq!(cookies, ["a=1", "b=2"]);
    let types: Vec<_> = converted.headers().get_all("content-type").iter().collect();
    assert_eq!(types, ["text/plain; charset=utf-8", "application/custom"]);
    let restored = Response::from_http_response(converted);
    assert_eq!(restored.body(), "body");
    assert_eq!(
        restored.headers(),
        &[
            ("set-cookie".into(), "a=1".into()),
            ("set-cookie".into(), "b=2".into())
        ]
    );
}

#[test]
fn response_conversion_handles_absent_and_non_text_headers() {
    let response = Response::from_http_response(http::Response::new("body".to_string()));
    assert_eq!(response.content_type(), "application/octet-stream");
    assert!(response.headers().is_empty());
    let mut response = http::Response::new("body".to_string());
    let opaque = http::HeaderValue::from_bytes(&[0xff]).unwrap();
    response
        .headers_mut()
        .insert("content-type", opaque.clone());
    response.headers_mut().insert("x-opaque", opaque);
    let response = Response::from_http_response(response);
    assert_eq!(response.content_type(), "application/octet-stream");
    assert!(response.headers().is_empty());
}
