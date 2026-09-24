use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits, ManagedLayoutRegistry};

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(93).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

fn execute(heap: &mut ActorHeap, encoded: &[u8], word: i64) -> Result<u64, ManagedMemoryError> {
    super::super::execute_managed_operation_with_context(
        heap,
        &ManagedLayoutRegistry::default(),
        None,
        encoded,
        &[word],
    )
}

#[test]
fn string_pattern_matches_and_extracts_typed_unicode_captures_through_abi() {
    let segments = [
        Segment::Literal("user/"),
        Segment::Capture(Kind::Int),
        Segment::Literal("/"),
        Segment::Capture(Kind::Float),
        Segment::Literal("/"),
        Segment::Capture(Kind::Bool),
        Segment::Literal("/"),
        Segment::Capture(Kind::String),
        Segment::Literal(".txt"),
    ];
    let mut heap = heap();
    let reference = heap
        .allocate_string("user/-42/1.5/true/ēna.txt.txt")
        .expect("input");
    let word = reference.encoded_abi_word() as i64;
    let matches = encode_string_pattern_matches_operation(&segments).expect("match descriptor");
    assert!(super::super::is_managed_operation(&matches));
    assert!(!super::super::managed_abi_result_is_reference(&matches));
    assert_eq!(execute(&mut heap, &matches, word), Ok(1));
    for (index, expected) in [(0, (-42_i64) as u64), (1, 1.5_f64.to_bits()), (2, 1)] {
        let extract = encode_string_pattern_extract_operation(&segments, index).expect("extract");
        assert!(!super::super::managed_abi_result_is_reference(&extract));
        assert_eq!(execute(&mut heap, &extract, word), Ok(expected));
    }
    let extract = encode_string_pattern_extract_operation(&segments, 3).expect("string capture");
    assert!(super::super::managed_abi_result_is_reference(&extract));
    let output = execute(&mut heap, &extract, word).expect("extract string");
    let output = super::super::reference_word(output as i64)
        .expect("reference")
        .cast::<ManagedString>();
    assert_eq!(heap.read_string(output).expect("text"), "ēna.txt");
}

#[test]
fn string_pattern_mismatch_does_not_extract_or_accept_partial_input() {
    let segments = [
        Segment::Literal("id/"),
        Segment::Capture(Kind::Int),
        Segment::Literal("/done"),
    ];
    let matches = encode_string_pattern_matches_operation(&segments).expect("match");
    let extract = encode_string_pattern_extract_operation(&segments, 0).expect("extract");
    let mut heap = heap();
    for text in [
        "prefix/id/1/done",
        "id/1/done/trailing",
        "id/not-int/done",
        "id/9223372036854775808/done",
        "id//done",
    ] {
        let word = heap
            .allocate_string(text)
            .expect("input")
            .encoded_abi_word() as i64;
        assert_eq!(execute(&mut heap, &matches, word), Ok(0), "{text}");
        assert_eq!(
            execute(&mut heap, &extract, word),
            Err(ManagedMemoryError::InvalidManagedOperation)
        );
    }
    for (kind, invalid) in [
        (Kind::Float, "NaN"),
        (Kind::Float, "inf"),
        (Kind::Bool, "TRUE"),
    ] {
        let encoded =
            encode_string_pattern_matches_operation(&[Segment::Capture(kind)]).expect("match");
        let word = heap
            .allocate_string(invalid)
            .expect("input")
            .encoded_abi_word() as i64;
        assert_eq!(execute(&mut heap, &encoded, word), Ok(0));
    }
}

#[test]
fn string_pattern_delimiters_are_deterministic_and_empty_strings_are_valid_captures() {
    use super::matching::{capture, Captured};
    let pattern = [
        Segment::Capture(Kind::String),
        Segment::Literal("/"),
        Segment::Capture(Kind::String),
    ];
    assert_eq!(
        capture("a/b/c", &pattern),
        Some(vec![Captured::String("a"), Captured::String("b/c")])
    );
    assert_eq!(
        capture("/", &pattern),
        Some(vec![Captured::String(""), Captured::String("")])
    );
    assert_eq!(
        capture("", &[Segment::Capture(Kind::String)]),
        Some(vec![Captured::String("")])
    );
    assert!(capture("12/extra", &[Segment::Capture(Kind::Int)]).is_none());
}

#[test]
fn string_pattern_descriptors_reject_malformed_and_unbounded_payloads() {
    let segments = [Segment::Literal("x/"), Segment::Capture(Kind::String)];
    let encoded = encode_string_pattern_extract_operation(&segments, 0).expect("descriptor");
    for end in 0..encoded.len() {
        assert!(decode(&encoded[..end]).is_err(), "truncation {end}");
    }
    for (index, value) in [
        (0, 0),
        (4, 2),
        (6, 3),
        (7, 0),
        (8, 1),
        (10, 0),
        (11, 2),
        (12, 9),
        (13, 255),
    ] {
        let mut changed = encoded.clone();
        changed[index] = value;
        assert!(decode(&changed).is_err(), "corruption {index}");
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
    assert!(encode_string_pattern_matches_operation(&[]).is_err());
    assert!(encode_string_pattern_matches_operation(&[Segment::Literal("")]).is_err());
    assert!(encode_string_pattern_matches_operation(&[
        Segment::Capture(Kind::String),
        Segment::Capture(Kind::Int)
    ])
    .is_err());
    assert!(encode_string_pattern_matches_operation(&vec![
        Segment::Literal("x");
        MAX_SEGMENTS + 1
    ])
    .is_err());
    assert!(encode_string_pattern_matches_operation(&[Segment::Literal(
        &"x".repeat(MAX_ENCODED_BYTES)
    )])
    .is_err());
    assert!(encode_string_pattern_extract_operation(&segments, 1).is_err());
    let mut heap = heap();
    assert!(execute(&mut heap, &encoded, 0).is_err());
    assert_eq!(
        execute_string_pattern_operation(&mut heap, &encoded, &[]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
}
