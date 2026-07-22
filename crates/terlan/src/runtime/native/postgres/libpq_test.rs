use super::*;

#[test]
fn generated_libpq_connection_exposes_nonblocking_poll_and_redacted_debug() {
    let mut connection = DriverConnection::start("postgres://127.0.0.1:1/terlan")
        .expect("PQconnectStart returns a nonblocking connection handle");
    match connection.socket() {
        Ok(socket) => assert!(socket >= 0),
        Err(error) => assert_eq!(error.code(), "postgres.connect.socket"),
    }
    let first = connection.poll_connect();
    assert!(
        first.is_ok()
            || first
                .as_ref()
                .is_err_and(|error| error.code() == "postgres.connect")
    );
    assert!(!format!("{connection:?}").contains("postgres://"));
    connection
        .abort()
        .expect("abort closes the native connection");
}

#[test]
fn libpq_parameter_text_preserves_scalars_and_structured_json() {
    assert_eq!(parameter_text(&serde_json::json!(42)).unwrap(), "42");
    assert_eq!(parameter_text(&serde_json::json!(true)).unwrap(), "true");
    assert_eq!(
        parameter_text(&serde_json::json!({"key": "value"})).unwrap(),
        "{\"key\":\"value\"}"
    );
}

#[test]
fn libpq_byte_copy_rejects_invalid_bytes_and_utf8() {
    assert_eq!(
        decode_bytes(&[84, 101, 114, 108, 97, 110]).unwrap(),
        "Terlan"
    );
    assert_eq!(
        decode_bytes(&[256]).unwrap_err().code(),
        "postgres.driver.bytes"
    );
    assert_eq!(
        decode_bytes(&[255]).unwrap_err().code(),
        "postgres.driver.utf8"
    );
}
