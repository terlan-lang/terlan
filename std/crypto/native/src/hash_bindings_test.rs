use super::*;
use terlan_runtime_abi::NativeValue;

fn fields(values: &[&str]) -> NativeValue {
    NativeValue::List(values.iter().map(|value| (*value).into()).collect())
}

#[test]
fn sha256_package_bindings_preserve_standard_vectors_and_binary_bytes() {
    for (text, expected) in [
        (
            "",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        for (binding, value) in [
            (SHA256, text.into()),
            (SHA256_BYTES, NativeValue::Bytes(text.as_bytes().to_vec())),
        ] {
            assert_eq!((binding.invoke)(&[value]).unwrap(), expected.into());
        }
    }
    for bytes in [vec![0, 255, 128], vec![b'a'; 1_000_000]] {
        assert_eq!(
            (SHA256_BYTES.invoke)(&[NativeValue::Bytes(bytes.clone())]).unwrap(),
            hash::sha256_bytes(&bytes).into()
        );
    }
    assert_eq!(
        hash::sha256_bytes(&vec![b'a'; 1_000_000]),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn sha256_package_framing_matches_independent_byte_assembly() {
    for values in [
        vec![],
        vec![""],
        vec!["a", "bc"],
        vec!["ab", "c"],
        vec!["\0", "\u{e9}", "e\u{301}"],
    ] {
        let mut framed = Vec::new();
        for value in &values {
            framed.extend_from_slice(&(value.len() as u64).to_be_bytes());
            framed.extend_from_slice(value.as_bytes());
        }
        assert_eq!(
            (SHA256_FRAMED.invoke)(&[fields(&values)]).unwrap(),
            hash::sha256_bytes(&framed).into()
        );
        assert_eq!(
            (SHA256_NUL_SEPARATED.invoke)(&[fields(&values)]).unwrap(),
            hash::sha256_bytes(values.join("\0").as_bytes()).into()
        );
        for domain in ["", "domain", "\0\u{e9}"] {
            let mut payload = domain.as_bytes().to_vec();
            payload.push(0);
            payload.extend_from_slice(&framed);
            assert_eq!(
                (SHA256_DOMAIN_FRAMED.invoke)(&[domain.into(), fields(&values)]).unwrap(),
                hash::sha256_bytes(&payload).into()
            );
        }
    }
    assert_ne!(
        (SHA256_FRAMED.invoke)(&[fields(&["a", "bc"])]).unwrap(),
        (SHA256_FRAMED.invoke)(&[fields(&["ab", "c"])]).unwrap()
    );
}

#[test]
fn sha256_package_bindings_reject_arity_and_each_invalid_argument() {
    for (binding, args) in [
        (SHA256, vec!["abc".into()]),
        (SHA256_BYTES, vec![NativeValue::Bytes(vec![])]),
        (SHA256_FRAMED, vec![fields(&[])]),
        (SHA256_DOMAIN_FRAMED, vec!["domain".into(), fields(&[])]),
        (SHA256_NUL_SEPARATED, vec![fields(&[])]),
    ] {
        for count in 0..4 {
            if count != binding.arity {
                assert!(
                    (binding.invoke)(&vec![NativeValue::Unit; count]).is_err(),
                    "{} arity {count}",
                    binding.operation
                );
            }
        }
        for index in 0..args.len() {
            let mut invalid = args.clone();
            invalid[index] = NativeValue::Bool(false);
            assert!((binding.invoke)(&invalid)
                .unwrap_err()
                .to_string()
                .contains("dispatch.type"));
            if matches!(args[index], NativeValue::List(_)) {
                invalid[index] = NativeValue::List(vec!["valid".into(), NativeValue::Int(7)]);
                assert!((binding.invoke)(&invalid).is_err());
            }
        }
    }
    assert!(framed_digest(None, SHA256_FRAMED.operation)
        .unwrap_err()
        .to_string()
        .contains("dispatch.hash_field_too_large"));
}
