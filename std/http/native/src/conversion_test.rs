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
