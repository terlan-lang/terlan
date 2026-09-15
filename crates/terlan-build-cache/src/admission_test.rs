use super::*;

#[test]
fn filesystem_space_excludes_reserved_blocks_and_rejects_invalid_arithmetic() {
    assert_eq!(available(5, 4096).unwrap(), 20480);
    assert_eq!(available(0, 4096).unwrap(), 0);
    assert!(available(1, 0).is_err());
    assert!(available(u64::MAX, 4096).is_err());
}

#[test]
fn disk_floor_is_explicit_positive_bounded_and_strictly_numeric() {
    for value in ["", "0", "-1", "+1", "1 ", "18446744073709551616"] {
        assert!(minimum(&["--minimum-free-bytes".into(), value.into()]).is_err());
    }
    assert!(minimum(&[]).is_err());
    assert!(minimum(&["--typo".into(), "1".into()]).is_err());
    assert_eq!(
        minimum(&["--minimum-free-bytes".into(), "8589934592".into()]).unwrap(),
        8589934592
    );
}
