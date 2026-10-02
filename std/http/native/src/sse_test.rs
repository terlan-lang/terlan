use super::*;

#[test]
fn codec_body_must_be_ready_valid_utf8_and_error_free() {
    use axum::body::{Body, Bytes};
    for body in [
        Body::from_stream(stream::pending::<Result<Bytes, std::io::Error>>()),
        Body::from_stream(stream::iter([Err::<Bytes, _>(std::io::Error::other(
            "broken codec",
        ))])),
        Body::from(vec![0xff]),
    ] {
        assert!(ready_body_text(body)
            .unwrap_err()
            .to_string()
            .contains("http.sse.codec"));
    }
    assert_eq!(ready_body_text(Body::from("ready")).unwrap(), "ready");
}

#[test]
fn maintained_encoder_frames_empty_multiline_and_unicode_data() {
    for (data, expected) in [
        ("", "\n"),
        ("hello", "data: hello\n\n"),
        ("a\nb", "data: a\ndata: b\n\n"),
        ("a\n", "data: a\ndata: \n\n"),
        ("\n\nid: injected", "data: \ndata: \ndata: id: injected\n\n"),
        ("\u{e9}\u{1f642}\0", "data: \u{e9}\u{1f642}\0\n\n"),
    ] {
        assert_eq!(
            encode_event(None, None, None, data).unwrap(),
            expected,
            "{data:?}"
        );
    }
    assert_eq!(
        encode_event(Some("42"), Some("update"), Some(1500), "ok").unwrap(),
        "id: 42\nevent: update\nretry: 1500\ndata: ok\n\n"
    );
    assert_eq!(
        encode_event(Some(""), Some(""), Some(i64::MAX), "").unwrap(),
        format!("id: \nevent: \nretry: {}\n\n", i64::MAX)
    );
}

#[test]
fn codec_rejects_unnormalized_data_without_rewriting_or_exposing_it() {
    let none = NativeValue::from(None::<String>);
    for data in [
        "\r",
        "\r\n",
        "secret\r\npayload",
        "secret\rpayload",
        "\n\r\n",
    ] {
        let args = [none.clone(), none.clone(), none.clone(), data.into()];
        let error = (ENCODE_EVENT.invoke)(&args).unwrap_err().to_string();
        assert!(error.contains("http.sse.unnormalized_data"));
        assert!(!error.contains("secret"));
    }
}

#[test]
fn metadata_injection_and_invalid_retry_return_errors_not_panics() {
    for metadata in ["\r", "\n", "\0", "secret\r\ndata: injected"] {
        for (id, event) in [(Some(metadata), None), (None, Some(metadata))] {
            let error = encode_event(id, event, None, "payload")
                .unwrap_err()
                .to_string();
            assert!(error.contains("http.sse.invalid_metadata"));
            assert!(!error.contains("secret"));
        }
    }
    for retry in [i64::MIN, -1, 0] {
        assert!(encode_event(None, None, Some(retry), "ok")
            .unwrap_err()
            .to_string()
            .contains("http.sse.invalid_retry"));
    }
}

#[test]
fn binding_validates_arity_and_every_argument_before_encoding() {
    let args = [
        NativeValue::from(Some("1")),
        NativeValue::from(Some("event")),
        NativeValue::from(Some(NativeValue::Int(1))),
        NativeValue::from("data"),
    ];
    assert_eq!(
        (ENCODE_EVENT.invoke)(&args).unwrap(),
        NativeValue::from("id: 1\nevent: event\nretry: 1\ndata: data\n\n")
    );
    for count in [0, 1, 2, 3, 5] {
        assert!((ENCODE_EVENT.invoke)(&vec![NativeValue::Unit; count])
            .unwrap_err()
            .to_string()
            .contains("native_package.arguments"));
    }
    for index in 0..4 {
        let mut invalid = args.clone();
        invalid[index] = NativeValue::Unit;
        assert!((ENCODE_EVENT.invoke)(&invalid)
            .unwrap_err()
            .to_string()
            .contains("dispatch.type"));
    }
    let none = NativeValue::from(None::<String>);
    assert_eq!(
        (ENCODE_EVENT.invoke)(&[none.clone(), none.clone(), none, NativeValue::from("")]).unwrap(),
        NativeValue::from("\n")
    );
}
