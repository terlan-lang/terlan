use super::*;

/// Builds a text bridge value fixture.
///
/// Inputs:
/// - `value`: string slice for the fixture.
///
/// Output:
/// - Bridge text value.
///
/// Transformation:
/// - Copies the text into the bridge-neutral value shape used by vectors.
fn text(value: &str) -> SafeNativeBridgeValue {
    SafeNativeBridgeValue::Text(value.to_string())
}

/// Validates native vectors preserve insertion order.
///
/// Inputs:
/// - Two bridge text values.
///
/// Output:
/// - Test passes when indexed reads return the original values.
///
/// Transformation:
/// - Moves values into Rust-owned vector storage and reads them by zero-based
///   Terlan indexes.
#[test]
fn vector_reads_values_by_index() {
    let vector = from_list(vec![text("Ada"), text("Grace")]);

    assert_eq!(vector.length(), Ok(2));
    assert_eq!(len(&vector), Ok(2));
    assert_eq!(vector.get_at(0), Ok(text("Ada")));
    assert_eq!(vector.get_at(1), Ok(text("Grace")));
}

/// Validates native vector mutation updates indexed storage.
///
/// Inputs:
/// - One vector with two values.
///
/// Output:
/// - Test passes when set, swap, and push are reflected by later reads.
///
/// Transformation:
/// - Applies mutable receiver operations to one Rust-owned vector resource.
#[test]
fn vector_mutations_update_storage() {
    let mut vector = from_list(vec![text("Ada"), text("Grace")]);

    assert_eq!(vector.set_at(1, text("Carol")), Ok(()));
    assert_eq!(vector.get_at(1), Ok(text("Carol")));

    assert_eq!(vector.swap(0, 1), Ok(()));
    assert_eq!(to_list(&vector), vec![text("Carol"), text("Ada")]);

    vector.push(text("Lin"));
    assert_eq!(
        to_list(&vector),
        vec![text("Carol"), text("Ada"), text("Lin")]
    );
}

/// Validates invalid indexes produce stable vector errors.
///
/// Inputs:
/// - One vector with a single value.
///
/// Output:
/// - Test passes when negative and out-of-bounds indexes return stable codes.
///
/// Transformation:
/// - Exercises bounds checking before the dispatch layer maps errors into
///   SafeNative diagnostics.
#[test]
fn vector_rejects_invalid_indexes() {
    let vector = NativeVector::from_values(vec![text("Ada")]);

    let negative = vector
        .get_at(-1)
        .err()
        .unwrap_or_else(|| VectorError::new("missing", ""));
    assert_eq!(negative.code(), "vector.negative_index");

    let out_of_bounds = vector
        .get_at(1)
        .err()
        .unwrap_or_else(|| VectorError::new("missing", ""));
    assert_eq!(out_of_bounds.code(), "vector.index_out_of_bounds");
}
