use super::digest_values;
use std::collections::BTreeMap;

#[test]
fn linker_environment_identity_preserves_absence_and_exact_bytes() {
    let absent = BTreeMap::from([("SDKROOT", None)]);
    let empty = BTreeMap::from([("SDKROOT", Some(Vec::new()))]);
    let configured = BTreeMap::from([("SDKROOT", Some(b"sdk-v2".to_vec()))]);
    assert_ne!(digest_values(&absent), digest_values(&empty));
    assert_ne!(digest_values(&empty), digest_values(&configured));
    assert_eq!(digest_values(&configured), digest_values(&configured));
}

#[test]
fn linker_environment_identity_is_framed_and_name_sensitive() {
    let first = BTreeMap::from([("A", Some(b"bc".to_vec()))]);
    let second = BTreeMap::from([("Ab", Some(b"c".to_vec()))]);
    assert_ne!(digest_values(&first), digest_values(&second));
    let changed = BTreeMap::from([("A", Some(vec![0xff]))]);
    assert_ne!(digest_values(&first), digest_values(&changed));
}
