use super::*;

fn invoke(binding: &NativeBinding, value: NativeValue) -> NativeValue {
    (binding.invoke)(&[value]).unwrap()
}

#[test]
fn all_bindings_reject_wrong_arity_and_value_types() {
    for binding in [
        &ENCODE,
        &ENCODE_URL,
        &ENCODE_BYTES,
        &ENCODE_URL_BYTES,
        &DECODE_TEXT,
        &DECODE_URL_TEXT,
        &DECODE_BYTES,
        &DECODE_URL_BYTES,
        &MD5,
    ] {
        for args in [
            vec![],
            vec![NativeValue::Unit],
            vec![NativeValue::Int(0)],
            vec![
                NativeValue::String("x".into()),
                NativeValue::String("y".into()),
            ],
        ] {
            assert!((binding.invoke)(&args).is_err(), "{}", binding.operation);
        }
        let wrong = if binding.operation.ends_with("encode_bytes")
            || binding.operation.ends_with("encode_url_bytes")
        {
            NativeValue::String("".into())
        } else {
            NativeValue::Bytes(vec![])
        };
        assert!((binding.invoke)(&[wrong]).is_err(), "{}", binding.operation);
    }
}

#[test]
fn text_codecs_preserve_owned_result_shapes_and_exact_unicode() {
    for (encode, decode) in [(&ENCODE, &DECODE_TEXT), (&ENCODE_URL, &DECODE_URL_TEXT)] {
        for text in ["", "hello", "\u{3bb}\u{1f600}\0\n"] {
            let encoded = invoke(encode, text.into());
            assert_eq!(
                invoke(decode, encoded),
                NativeValue::from(Ok::<_, String>(text))
            );
        }
    }
    assert_eq!(
        invoke(&MD5, "abc".into()),
        NativeValue::from("900150983cd24fb0d6963f7d28e17f72")
    );
}

#[test]
fn byte_codecs_preserve_all_octets_and_padding_boundaries() {
    for (encode, decode) in [
        (&ENCODE_BYTES, &DECODE_BYTES),
        (&ENCODE_URL_BYTES, &DECODE_URL_BYTES),
    ] {
        for len in [0, 1, 2, 3, 255, 256, 257, 4096] {
            let bytes = (0..len).map(|index| index as u8).collect::<Vec<_>>();
            let encoded = invoke(encode, NativeValue::Bytes(bytes.clone()));
            assert_eq!(
                invoke(decode, encoded),
                NativeValue::from(Ok::<_, String>(NativeValue::Bytes(bytes)))
            );
        }
    }
}

fn assert_failure(binding: &NativeBinding, text: &str, utf8: bool) {
    let NativeValue::Record { name, fields } = invoke(binding, text.into()) else {
        panic!("typed result required");
    };
    assert_eq!(name, "Err");
    assert!(
        matches!(fields.as_slice(), [(field, NativeValue::Tuple(values))]
        if field == "reason" && matches!(values.as_slice(), [NativeValue::Bool(kind), NativeValue::String(message)]
            if *kind == utf8 && !message.is_empty()))
    );
}

#[test]
fn failures_distinguish_codec_and_utf8_without_public_error_policy() {
    for binding in [
        &DECODE_TEXT,
        &DECODE_URL_TEXT,
        &DECODE_BYTES,
        &DECODE_URL_BYTES,
    ] {
        for input in [
            "!", "Zg", "Zg=", "Zh==", "Zm9=", "Zg===", "Zg==x", " Zg==", "Zg==\n", "\0", "\u{e9}",
        ] {
            assert_failure(binding, input, false);
        }
    }
    assert_failure(&DECODE_TEXT, "//4=", true);
    assert_failure(&DECODE_URL_TEXT, "__4=", true);
    assert_failure(&DECODE_TEXT, "__4=", false);
    assert_failure(&DECODE_URL_TEXT, "//4=", false);
    assert_eq!(
        invoke(&DECODE_BYTES, "//4=".into()),
        NativeValue::from(Ok::<_, String>(NativeValue::Bytes(vec![255, 254])))
    );
}
