use bytes::Bytes;

use super::{HttpResponseChunks, InvalidHttpStreamLimits};

#[test]
fn limits_fail_before_retaining_source_and_empty_chunks_never_emit() {
    let source = Bytes::from(vec![42; 100]);
    for (size, pending) in [(0, 1), (-1, 1), (1, 0), (1, -1), (i64::MIN, 1)] {
        assert_eq!(
            HttpResponseChunks::new(vec![source.clone()], size, pending),
            Err(InvalidHttpStreamLimits)
        );
        assert!(source.is_unique());
    }
    let mut empty = HttpResponseChunks::new(vec![Bytes::new(); 3], 1, 1).unwrap();
    assert!(empty.is_complete());
    assert_eq!(empty.next_chunk(), None);
    assert_eq!(empty.next_chunk(), None);
}

#[test]
fn every_chunk_bound_preserves_exact_source_bytes_and_shares_storage() {
    let source = Bytes::from((0..=255).collect::<Vec<u8>>());
    for size in [1, 2, 3, 16, 255, 256, 257] {
        let mut chunks = HttpResponseChunks::new(vec![source.clone()], size, 8).unwrap();
        let mut output = Vec::new();
        while let Some(chunk) = chunks.next_chunk() {
            assert!(!chunk.is_empty());
            assert!(chunk.len() <= size as usize);
            assert_eq!(chunk.as_ptr(), source[output.len()..].as_ptr());
            output.extend_from_slice(&chunk);
        }
        assert!(chunks.is_complete());
        assert_eq!(output, source);
        assert!(source.is_unique());
    }
}

#[test]
fn clones_have_independent_cursors_and_cancellation_releases_retained_bytes() {
    let source = Bytes::from(vec![0, 1, 2, 3]);
    let mut first = HttpResponseChunks::new(vec![source.clone(), Bytes::new()], 2, 1).unwrap();
    let mut second = first.clone();
    assert_eq!(first.next_chunk().unwrap().as_ref(), &[0, 1]);
    assert_eq!(first.next_chunk().unwrap().as_ref(), &[2, 3]);
    assert!(first.is_complete());
    assert!(!second.is_complete());
    assert_eq!(second.next_chunk().unwrap().as_ref(), &[0, 1]);
    assert!(!source.is_unique());
    drop(second);
    assert!(source.is_unique());
}
