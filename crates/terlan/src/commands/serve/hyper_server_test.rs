use super::*;

#[test]
fn protocol_errors_are_hyper_responses() {
    let response = error_response(400, "bad request".to_string());
    assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get(http::header::CONTENT_TYPE),
        Some(&http::HeaderValue::from_static("text/plain; charset=utf-8"))
    );
}
