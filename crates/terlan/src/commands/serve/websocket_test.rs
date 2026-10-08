use super::*;

#[test]
fn websocket_upgrade_response_reuses_package_handshake_plan() {
    let request = Request::builder()
        .header("upgrade", "websocket")
        .header("connection", "Upgrade")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("sec-websocket-version", "13")
        .body(())
        .unwrap();
    let response = websocket_upgrade_response(&request);
    assert_eq!(response.status(), 101);
    assert_eq!(response.headers()["upgrade"], "websocket");
    assert_eq!(response.headers()["connection"], "Upgrade");
    assert_eq!(
        response.headers()["sec-websocket-accept"],
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );
}
