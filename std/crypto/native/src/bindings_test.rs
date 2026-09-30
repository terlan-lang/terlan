use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};

const KEY: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
const SIGNATURE: &str =
    "5VZDAMNgrHKQhuLMgG6CioSHfx645dl02HPgZSJJAVVfuIIVkKM7rMYeOXAc+bRr0lv18FlbviRlUUFDjnoQCw==";

fn verify(key: &str, payload: &str, signature: &str) -> bool {
    let value = (VERIFY_ED25519.invoke)(&[key.into(), payload.into(), signature.into()])
        .expect("typed arguments");
    let NativeValue::Bool(result) = value else {
        panic!("verification must return a boolean")
    };
    result
}

#[test]
fn rfc8032_empty_message_vector_and_every_signature_byte_mutation() {
    assert!(verify(KEY, "", SIGNATURE));
    assert!(!verify(KEY, "\0", SIGNATURE));
    let bytes = STANDARD.decode(SIGNATURE).unwrap();
    for index in 0..bytes.len() {
        let mut mutated = bytes.clone();
        mutated[index] ^= 1;
        assert!(!verify(KEY, "", &STANDARD.encode(mutated)), "byte {index}");
    }
}

#[test]
fn malformed_encodings_and_key_signature_lengths_fail_closed() {
    for value in ["", "!", "====", "AA", "AA==\n", "AA== "] {
        assert!(!verify(value, "", SIGNATURE));
        assert!(!verify(KEY, "", value));
    }
    for size in [0, 1, 31, 32, 33, 63, 64, 65, 128] {
        let value = STANDARD.encode(vec![0; size]);
        assert!(!verify(&value, "", SIGNATURE));
        assert!(!verify(KEY, "", &value));
        if size != 32 {
            assert!(ed25519::sign(&value, "").is_none());
        }
    }
}

#[test]
fn binding_rejects_wrong_arity_and_each_non_string_argument() {
    assert_eq!(VERIFY_ED25519.arity, 3);
    for count in [0, 1, 2, 4] {
        assert!((VERIFY_ED25519.invoke)(&vec![NativeValue::String(String::new()); count]).is_err());
    }
    for index in 0..3 {
        for invalid in [
            NativeValue::Bool(false),
            NativeValue::Bytes(vec![]),
            NativeValue::Unit,
        ] {
            let mut args = vec![KEY.into(), "".into(), SIGNATURE.into()];
            args[index] = invalid;
            assert!((VERIFY_ED25519.invoke)(&args).is_err());
        }
    }
}

#[test]
fn signed_unicode_and_nul_payloads_preserve_exact_bytes() {
    let seed = STANDARD.encode([7_u8; 32]);
    for payload in ["", "a\0b", "\u{e9}", "e\u{301}", "line\r\nnext"] {
        let signed = ed25519::sign(&seed, payload).unwrap();
        assert!(verify(
            &signed.public_key_base64,
            payload,
            &signed.signature_base64
        ));
        assert!(!verify(
            &signed.public_key_base64,
            &format!("{payload} "),
            &signed.signature_base64
        ));
        assert_eq!(signed, ed25519::sign(&seed, payload).unwrap());
    }
}
