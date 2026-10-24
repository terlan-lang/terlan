use super::*;

#[test]
fn entropy_preserves_every_octet_at_each_supported_boundary() {
    for count in [1, 16, 32, 4096] {
        let mut calls = 0;
        let bytes = bytes_with(count, |output| {
            calls += 1;
            assert_eq!(output.len(), count as usize);
            for (index, byte) in output.iter_mut().enumerate() {
                *byte = index as u8;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(
            bytes,
            (0..count).map(|index| index as u8).collect::<Vec<_>>()
        );
        let NativeValue::Bytes(actual) = (ENTROPY_BYTES.invoke)(&[count.into()]).unwrap() else {
            panic!("raw bytes required");
        };
        assert_eq!(actual.len(), count as usize);
    }
}

#[test]
fn invalid_sizes_never_allocate_or_call_the_provider() {
    for count in [i64::MIN, -1, 0, 4097, i64::MAX] {
        let error = bytes_with(count, |_| panic!("invalid size reaches provider")).unwrap_err();
        assert!(error.to_string().contains("crypto.entropy.size"));
        assert_eq!((ENTROPY_BYTES.invoke)(&[count.into()]), Err(error));
    }
}

#[test]
fn partial_entropy_failure_is_not_returned_or_retried() {
    let mut calls = 0;
    let result = bytes_with(32, |output| {
        calls += 1;
        output[..16].fill(255);
        Err(getrandom::Error::UNSUPPORTED)
    });
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("crypto.entropy.unavailable"));
    assert_eq!(calls, 1);
}

#[test]
fn entropy_binding_rejects_wrong_arity_and_coercions() {
    assert_eq!(ENTROPY_BYTES.arity, 1);
    for args in [vec![], vec![32_i64.into(), 32_i64.into()]] {
        assert_eq!(
            (ENTROPY_BYTES.invoke)(&args).unwrap_err().code(),
            "native_package.arguments"
        );
    }
    for value in [
        NativeValue::Unit,
        NativeValue::Bool(true),
        NativeValue::Float(32.0),
        NativeValue::from("32"),
        NativeValue::Atom("32".into()),
        NativeValue::Bytes(vec![32]),
        NativeValue::List(vec![]),
        NativeValue::Tuple(vec![]),
        NativeValue::Map(vec![]),
        None::<String>.into(),
    ] {
        assert_eq!(
            (ENTROPY_BYTES.invoke)(&[value]).unwrap_err().code(),
            "dispatch.type"
        );
    }
}
