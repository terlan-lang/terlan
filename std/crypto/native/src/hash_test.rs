use super::hash;

#[test]
fn sha256_bytes_matches_the_standard_vector() {
    assert_eq!(
        hash::sha256_bytes(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn sha256_framing_contracts_are_ordered_and_domain_separated() {
    let values = vec!["alpha".to_string(), "beta".to_string()];
    let reversed = vec!["beta".to_string(), "alpha".to_string()];
    assert_ne!(hash::sha256_framed(&values), hash::sha256_framed(&reversed));
    assert_ne!(
        hash::sha256_domain_framed("one", &values),
        hash::sha256_domain_framed("two", &values)
    );
    assert_ne!(
        hash::sha256_nul_separated(&values),
        hash::sha256_nul_separated(&reversed)
    );
}
