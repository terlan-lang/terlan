use super::*;
#[test]
fn union_frame_is_bounded_and_excludes_continuation_captures() {
    let mut frame = vec![TAG, 0, 0, 0, 99, 1, 15, 2];
    frame.extend(TvmBoundaryType::Managed([1; 16]).transition_words());
    frame.extend(TvmBoundaryType::Managed([2; 16]).transition_words());
    frame.push(42);
    frame.push(999);
    assert_eq!(frame_words(&frame).unwrap(), 15);
    assert_eq!(arguments(&frame[..15]).unwrap()[0].word, 42);
    for bad in [-1, 0, 17, i64::MAX] {
        let mut changed = frame.clone();
        changed[7] = bad;
        assert!(frame_words(&changed).is_err());
    }
    frame[6] = 16;
    assert!(frame_words(&frame).is_err());
}
#[test]
fn union_alternatives_cannot_reinterpret_scalar_words() {
    let mut frame = vec![TAG, 0, 0, 0, 99, 1, 15, 2];
    frame.extend(TvmBoundaryType::Int.transition_words());
    frame.extend(TvmBoundaryType::Managed([2; 16]).transition_words());
    frame.push(42);
    assert!(arguments(&frame).is_err());
}
