//! Adjacent behavioral tests; module identity and assertions are preserved.

use super::*;

#[test]
fn oversized_write_does_not_allocate_or_change_output() {
    let payload = vec![0; 4096];
    let mut bytes = EncodingBuffer::new(8);
    assert!(bytes.extend_from_slice(&payload).is_err());
    assert_eq!(bytes.bytes.capacity(), 0);
    bytes.extend_from_slice(&[1, 2, 3]).unwrap();
    let capacity = bytes.bytes.capacity();
    assert!(bytes.extend_from_slice(&payload).is_err());
    assert_eq!(bytes.bytes, [1, 2, 3]);
    assert_eq!(bytes.bytes.capacity(), capacity);
    assert!(bytes.require(usize::MAX).is_err());
}

#[test]
fn growth_never_reserves_more_than_the_configured_byte_limit() {
    for maximum in 0..65 {
        let mut bytes = EncodingBuffer::new(maximum);
        for _ in 0..maximum {
            bytes.push(42).unwrap();
            assert!(bytes.bytes.capacity() <= maximum);
        }
        assert_eq!(bytes.remaining(), 0);
        assert!(bytes.push(43).is_err());
        assert_eq!(bytes.bytes, vec![42; maximum]);
    }
}
