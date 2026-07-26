//! Tests for bounded managed operations used by generated native code.

use std::sync::Arc;

use crate::runtime::native_image::{TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor};

use super::super::{
    encode_aggregate_layout, encode_collection_layout, ActorHeap, ActorId, HeapLimits,
    ManagedAggregate, ManagedAggregateDescriptor, ManagedCollectionDescriptor, ManagedFieldType,
    ManagedFieldValue, ManagedKeySemantics, ManagedLayoutRegistry, ManagedList, ManagedMap,
    ManagedMemoryError, ManagedRoot, ManagedString, RootLocation, SemanticTypeId, TvmRef,
};
use super::{
    encode_aggregate_append_pair_operation, encode_aggregate_field_operation,
    encode_aggregate_replace_field_operation, encode_aggregate_scalar_field_operation,
    encode_binary_pattern_extract_operation, encode_binary_pattern_matches_operation,
    encode_bitstring_operation, encode_bytes_from_list_operation, encode_bytes_length_operation,
    encode_bytes_to_list_operation, encode_list_empty_operation, encode_list_first_operation,
    encode_list_from_elements_operation, encode_list_get_operation, encode_list_is_empty_operation,
    encode_list_prepend_operation, encode_list_rest_operation,
    encode_managed_value_equal_operation, encode_map_contains_operation,
    encode_map_from_entries_operation, encode_map_get_operation, encode_string_append_operation,
    encode_string_concat_operation, encode_string_equal_operation,
    encode_string_escape_html_attribute_operation, encode_string_escape_html_text_operation,
    encode_string_list_join_operation, encode_string_map_get_option_operation,
    encode_string_prepend_literal_operation, encode_string_prepend_projected_literal_operation,
    encode_template_render_operation, execute_managed_operation, managed_abi_result_is_reference,
    ManagedBinaryPatternEndian, ManagedBinaryPatternField, ManagedBitStringOperation,
    ManagedTemplateValueKind,
};
use crate::runtime::vm::ReplValue;

const REQUEST: &str = "Named(Request)";
const STRING_MAP: &str = "std.http.Request.StringMap";
const STRING_OPTION: &str = "Option[String]";
const RESPONSE: &str = "Named(Response)";
const RESPONSE_HEADER: &str = "std.http.Response.Header";
const RESPONSE_HEADERS: &str = "std.http.Response.Headers";
const HTML_FRAGMENTS: &str = "List(Named(Template.Html))";
const INT_LIST: &str = "List(Int)";

/// Independent public-boundary string semantics used to construct fixture maps.
struct PublicStringSemantics;

impl ManagedKeySemantics for PublicStringSemantics {
    fn equivalent(
        &mut self,
        heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(string_field(heap, left)? == string_field(heap, right)?)
    }

    fn hash(
        &mut self,
        heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError> {
        ReplValue::String(string_field(heap, value)?.to_string())
            .stable_hash()
            .map_err(|_| ManagedMemoryError::InvalidAggregateField)
    }
}

/// Reads one managed string field for the independent fixture key semantics.
fn string_field(heap: &ActorHeap, value: ManagedFieldValue) -> Result<&str, ManagedMemoryError> {
    let ManagedFieldValue::Reference(reference) = value else {
        return Err(ManagedMemoryError::InvalidAggregateField);
    };
    heap.read_string(reference.cast())
}

/// Builds one actor heap with room for the complete operation fixture.
fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(91).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

#[test]
fn generated_collection_values_use_admitted_persistent_schemas() {
    let layouts = registry();
    let mut heap = heap();
    let list_semantic = semantic(INT_LIST);
    let list = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_list_from_elements_operation(list_semantic),
        &[2, 3],
    )
    .expect("list literal");
    let list =
        TvmRef::<ManagedList>::from_encoded(std::num::NonZeroUsize::new(list as usize).unwrap());
    let list_descriptor = layouts
        .collection(list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .expect("list schema");
    assert_eq!(
        heap.list_elements(list_descriptor, list),
        Ok(vec![ManagedFieldValue::Int(2), ManagedFieldValue::Int(3)])
    );
    let prepended = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_list_prepend_operation(list_semantic),
        &[1, word(list)],
    )
    .expect("list prepend");
    let prepended = TvmRef::<ManagedList>::from_encoded(
        std::num::NonZeroUsize::new(prepended as usize).unwrap(),
    );
    assert_eq!(
        heap.list_elements(list_descriptor, prepended),
        Ok(vec![
            ManagedFieldValue::Int(1),
            ManagedFieldValue::Int(2),
            ManagedFieldValue::Int(3),
        ])
    );
    let is_empty = encode_list_is_empty_operation(list_semantic);
    let first = encode_list_first_operation(list_semantic, false);
    let rest = encode_list_rest_operation(list_semantic);
    assert!(!managed_abi_result_is_reference(&is_empty));
    assert!(!managed_abi_result_is_reference(&first));
    assert!(managed_abi_result_is_reference(&rest));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &is_empty, &[word(prepended)]),
        Ok(0)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &first, &[word(prepended)]),
        Ok(1)
    );
    let get = encode_list_get_operation(list_semantic, false);
    assert!(!managed_abi_result_is_reference(&get));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &get, &[word(prepended), 1]),
        Ok(2)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &get, &[word(prepended), -1]),
        Err(ManagedMemoryError::CollectionIndexOutOfBounds)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &get, &[word(prepended), 3]),
        Err(ManagedMemoryError::CollectionIndexOutOfBounds)
    );
    let from_list = encode_bytes_from_list_operation(list_semantic);
    let bytes = execute_managed_operation(&mut heap, &layouts, &from_list, &[word(prepended)])
        .expect("bytes from list");
    assert_eq!(
        heap.read_bytes(TvmRef::from_encoded(
            std::num::NonZeroUsize::new(bytes as usize).unwrap()
        )),
        Ok(&[1, 2, 3][..])
    );
    let length = encode_bytes_length_operation(list_semantic);
    assert!(!managed_abi_result_is_reference(&length));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &length, &[bytes as i64]),
        Ok(3)
    );
    let to_list = encode_bytes_to_list_operation(list_semantic);
    assert!(managed_abi_result_is_reference(&to_list));
    let round_trip = execute_managed_operation(&mut heap, &layouts, &to_list, &[bytes as i64])
        .expect("bytes to list");
    let round_trip = TvmRef::<ManagedList>::from_encoded(
        std::num::NonZeroUsize::new(round_trip as usize).unwrap(),
    );
    assert_eq!(
        heap.list_elements(list_descriptor, round_trip),
        Ok(vec![
            ManagedFieldValue::Int(1),
            ManagedFieldValue::Int(2),
            ManagedFieldValue::Int(3),
        ])
    );
    let rest_value = execute_managed_operation(&mut heap, &layouts, &rest, &[word(prepended)])
        .expect("list rest");
    let rest_value = TvmRef::<ManagedList>::from_encoded(
        std::num::NonZeroUsize::new(rest_value as usize).unwrap(),
    );
    assert_eq!(
        heap.list_elements(list_descriptor, rest_value),
        Ok(vec![ManagedFieldValue::Int(2), ManagedFieldValue::Int(3)])
    );

    let key = heap.allocate_string("route").expect("map key");
    let value = heap.allocate_string("native").expect("map value");
    let map_semantic = semantic(STRING_MAP);
    let map = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_map_from_entries_operation(map_semantic),
        &[word(key), word(value)],
    )
    .expect("map literal");
    let map =
        TvmRef::<ManagedMap>::from_encoded(std::num::NonZeroUsize::new(map as usize).unwrap());
    let map_descriptor = layouts
        .collection(map_semantic)
        .and_then(|collection| collection.map_descriptor())
        .expect("map schema");
    assert_eq!(
        heap.map_get(
            map_descriptor,
            map,
            ManagedFieldValue::Reference(key.erase()),
            &mut PublicStringSemantics,
        ),
        Ok(Some(ManagedFieldValue::Reference(value.erase())))
    );
    let contains = encode_map_contains_operation(map_semantic);
    let get = encode_map_get_operation(map_semantic, true);
    assert!(!managed_abi_result_is_reference(&contains));
    assert!(managed_abi_result_is_reference(&get));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &contains, &[word(map), word(key)]),
        Ok(1)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &get, &[word(map), word(key)]),
        Ok(word(value) as u64)
    );
}

/// Managed string equality compares values and rejects invalid operand shapes.
#[test]
fn string_equal_operation_is_value_based_and_checked() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let left = heap.allocate_string("route").expect("left");
    let same = heap.allocate_string("route").expect("same");
    let different = heap.allocate_string("other").expect("different");
    let operation = encode_string_equal_operation();

    assert!(!managed_abi_result_is_reference(&operation));

    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(left), word(same)]),
        Ok(1)
    );
    assert_eq!(
        execute_managed_operation(
            &mut heap,
            &layouts,
            &operation,
            &[word(left), word(different)]
        ),
        Ok(0)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(left)]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
}

/// Schema-directed equality compares collection values instead of references.
#[test]
fn managed_value_equality_is_structural_and_checked() {
    let layouts = registry();
    let mut heap = heap();
    let semantic = semantic(INT_LIST);
    let descriptor = layouts
        .collection(semantic)
        .and_then(|collection| collection.list_descriptor())
        .expect("integer list schema");
    let left = heap
        .list_from_elements(
            descriptor,
            &[ManagedFieldValue::Int(1), ManagedFieldValue::Int(2)],
        )
        .expect("left list");
    let same = heap
        .list_from_elements(
            descriptor,
            &[ManagedFieldValue::Int(1), ManagedFieldValue::Int(2)],
        )
        .expect("same list");
    let different = heap
        .list_from_elements(
            descriptor,
            &[ManagedFieldValue::Int(2), ManagedFieldValue::Int(1)],
        )
        .expect("different list");
    let operation = encode_managed_value_equal_operation(semantic);

    assert!(!managed_abi_result_is_reference(&operation));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(left), word(same)]),
        Ok(1)
    );
    assert_eq!(
        execute_managed_operation(
            &mut heap,
            &layouts,
            &operation,
            &[word(left), word(different)]
        ),
        Ok(0)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(left)]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, b"TVME", &[word(left), word(same)]),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
}

#[test]
fn managed_value_equality_supports_immediate_zero_field_union_variants() {
    let layouts = registry();
    let mut heap = heap();
    let semantic = semantic(STRING_OPTION);
    let some_layout = layouts
        .layouts(semantic)
        .iter()
        .find(|layout| layout.variant_name() == Some("Some"))
        .cloned()
        .expect("Some layout");
    let value = heap.allocate_string("value").expect("string value");
    let some = heap
        .allocate_aggregate(
            some_layout.clone(),
            &[ManagedFieldValue::Reference(value.erase())],
        )
        .expect("Some value");
    let operation = encode_managed_value_equal_operation(semantic);

    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[20, 20]),
        Ok(1)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[20, 24]),
        Ok(0)
    );
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[20, word(some)]),
        Ok(0)
    );

    let mut foreign = ActorHeap::new(
        ActorId::new(92).expect("foreign actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("foreign limits"),
    )
    .expect("foreign heap");
    let foreign_value = foreign.allocate_string("value").expect("foreign string");
    let foreign_some = foreign
        .allocate_aggregate(
            some_layout,
            &[ManagedFieldValue::Reference(foreign_value.erase())],
        )
        .expect("foreign Some");
    assert_eq!(
        execute_managed_operation(
            &mut heap,
            &layouts,
            &operation,
            &[word(some), word(foreign_some)]
        ),
        Err(ManagedMemoryError::CrossActorReference)
    );
}

#[test]
fn binary_pattern_operations_match_and_extract_checked_fields() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let storage = heap
        .allocate_bytes(&[0x12, 0xfe, b'A', 0b1010_0000])
        .expect("binary storage");
    let binary = heap.allocate_binary(storage, 0, 32).expect("binary value");
    let fields = [
        ManagedBinaryPatternField::UInt(8),
        ManagedBinaryPatternField::Int(8),
        ManagedBinaryPatternField::Utf8,
        ManagedBinaryPatternField::Rest,
    ];
    let matches = encode_binary_pattern_matches_operation(ManagedBinaryPatternEndian::Big, &fields)
        .expect("match descriptor");
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &matches, &[word(binary)]),
        Ok(1)
    );
    for (index, expected) in [(0, 0x12_u64), (1, (-2_i64) as u64), (2, 65_u64)] {
        let extract = encode_binary_pattern_extract_operation(
            ManagedBinaryPatternEndian::Big,
            &fields,
            index,
        )
        .expect("scalar extractor");
        assert!(!managed_abi_result_is_reference(&extract));
        assert_eq!(
            execute_managed_operation(&mut heap, &layouts, &extract, &[word(binary)]),
            Ok(expected)
        );
    }
    let rest = encode_binary_pattern_extract_operation(ManagedBinaryPatternEndian::Big, &fields, 3)
        .expect("rest extractor");
    assert!(managed_abi_result_is_reference(&rest));
    let rest = execute_managed_operation(&mut heap, &layouts, &rest, &[word(binary)])
        .map(reference)
        .expect("rest value");
    assert_eq!(heap.read_bytes(rest.cast()), Ok(&[0b1010_0000][..]));

    let exact = encode_binary_pattern_matches_operation(
        ManagedBinaryPatternEndian::Big,
        &[ManagedBinaryPatternField::UInt(8)],
    )
    .expect("exact descriptor");
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &exact, &[word(binary)]),
        Ok(0)
    );
}

#[test]
fn binary_pattern_rest_can_start_after_an_unaligned_bit_field() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let storage = heap
        .allocate_bytes(&[0xaa, 0xbf, 0xe0])
        .expect("binary storage");
    let binary = heap.allocate_binary(storage, 0, 19).expect("binary value");
    let fields = [
        ManagedBinaryPatternField::Bytes(1),
        ManagedBinaryPatternField::Bits(3),
        ManagedBinaryPatternField::Rest,
    ];
    let matches = encode_binary_pattern_matches_operation(ManagedBinaryPatternEndian::Big, &fields)
        .expect("match descriptor");
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &matches, &[word(binary)]),
        Ok(1)
    );
    let rest = encode_binary_pattern_extract_operation(ManagedBinaryPatternEndian::Big, &fields, 2)
        .expect("rest extractor");
    let rest = execute_managed_operation(&mut heap, &layouts, &rest, &[word(binary)])
        .map(reference)
        .expect("rest value");
    assert_eq!(heap.read_bytes(rest.cast()), Ok(&[0xff][..]));
}

#[test]
fn bitstring_construction_operations_preserve_layout_bits() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let bytes = heap.allocate_bytes(&[1, 2, 3]).expect("rest bytes");
    let integer = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_bitstring_operation(ManagedBitStringOperation::FromUintBe),
        &[8080, 16],
    )
    .map(reference)
    .expect("integer segment");
    let rest = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_bitstring_operation(ManagedBitStringOperation::FromAllBytes),
        &[word(bytes)],
    )
    .map(reference)
    .expect("rest segment");
    let packet = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_bitstring_operation(ManagedBitStringOperation::Concat),
        &[word(integer), word(rest)],
    )
    .map(reference)
    .expect("concatenated packet");
    let packet = heap.read_binary(packet.cast()).expect("packet view");

    assert_eq!(packet.bit_length(), 40);
    assert_eq!(packet.aligned_bytes(), Some(&[0x1f, 0x90, 1, 2, 3][..]));
    assert_eq!(
        execute_managed_operation(
            &mut heap,
            &layouts,
            &encode_bitstring_operation(ManagedBitStringOperation::FromUintBe),
            &[8080, 0],
        ),
        Err(ManagedMemoryError::InvalidManagedScalar)
    );
}

/// Managed string append allocates one independent UTF-8 result value.
#[test]
fn string_append_operation_concatenates_validated_values() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let left = heap.allocate_string("fallλ").expect("left");
    let right = heap.allocate_string("back").expect("right");
    let operation = encode_string_append_operation();
    assert!(managed_abi_result_is_reference(&operation));
    let result =
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(left), word(right)])
            .map(reference)
            .expect("append strings");
    let aliased =
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(left), word(left)])
            .map(reference)
            .expect("append aliased string");

    assert_eq!(heap.read_string(result.cast()), Ok("fallλback"));
    assert_eq!(heap.read_string(aliased.cast()), Ok("fallλfallλ"));
    assert_eq!(heap.read_string(left), Ok("fallλ"));
    assert_eq!(heap.read_string(right), Ok("back"));
}

/// Variadic string concatenation allocates one result without intermediates.
#[test]
fn string_concat_operation_concatenates_all_validated_values() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let first = heap.allocate_string("one").expect("first");
    let second = heap.allocate_string("λ").expect("second");
    let third = heap.allocate_string("three").expect("third");
    let operation = encode_string_concat_operation();
    let result = execute_managed_operation(
        &mut heap,
        &layouts,
        &operation,
        &[word(first), word(second), word(third)],
    )
    .map(reference)
    .expect("concatenate strings");

    assert_eq!(heap.read_string(result.cast()), Ok("oneλthree"));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(first)]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
}

/// Literal prepend keeps immutable image bytes outside the actor heap.
#[test]
fn string_prepend_literal_operation_allocates_only_the_result() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let right = heap.allocate_string("λbody").expect("right");
    let operation = encode_string_prepend_literal_operation("prefix:").expect("prepend operation");
    assert!(managed_abi_result_is_reference(&operation));

    let result = execute_managed_operation(&mut heap, &layouts, &operation, &[word(right)])
        .map(reference)
        .expect("prepend literal");

    assert_eq!(heap.read_string(result.cast()), Ok("prefix:λbody"));
    assert_eq!(heap.read_string(right), Ok("λbody"));
}

#[test]
fn response_sized_string_prepend_uses_collectible_external_storage() {
    let mut heap = heap();
    let layouts = ManagedLayoutRegistry::default();
    let body = "x".repeat(4 * 1024);
    let right = heap.allocate_string(&body).expect("right");
    let operation = encode_string_prepend_literal_operation("prefix:").expect("operation");
    let result = execute_managed_operation(&mut heap, &layouts, &operation, &[word(right)])
        .map(reference)
        .expect("prepend response body")
        .cast::<ManagedString>();
    assert!(heap
        .external_string_bytes(result)
        .expect("external lookup")
        .is_some());
    assert_eq!(
        heap.read_string(result).expect("external string"),
        format!("prefix:{body}")
    );

    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        result.erase(),
    )];
    heap.collect(&mut roots, 32 * 1024)
        .expect("collect external string");
    let relocated = roots[0].reference().cast::<ManagedString>();
    assert_eq!(
        heap.read_string(relocated).expect("relocated string"),
        format!("prefix:{body}")
    );
}

/// Projection and prefix concatenation execute through one generated ABI call.
#[test]
fn string_prepend_projected_literal_operation_reads_the_checked_field() {
    let layouts = registry();
    let mut heap = heap();
    let (request, _) = request_fixture(&mut heap, &layouts, 1);
    let operation =
        encode_string_prepend_projected_literal_operation(semantic(REQUEST), 4, "prefix:")
            .expect("projected prepend operation");

    let result = execute_managed_operation(&mut heap, &layouts, &operation, &[word(request)])
        .map(reference)
        .expect("project and prepend");

    assert_eq!(heap.read_string(result.cast()), Ok("prefix:payload"));
}

/// Builds the fixed aggregate descriptors admitted by the operation fixture.
fn aggregate_descriptors() -> Vec<Arc<ManagedAggregateDescriptor>> {
    let string = ManagedFieldType::Reference(semantic("std.core.String"));
    let map = ManagedFieldType::Reference(semantic(STRING_MAP));
    vec![
        Arc::new(
            ManagedAggregateDescriptor::tuple(
                REQUEST,
                vec![
                    ManagedFieldType::Int,
                    string,
                    string,
                    map,
                    string,
                    string,
                    map,
                    map,
                    map,
                ],
            )
            .expect("request layout"),
        ),
        Arc::new(
            ManagedAggregateDescriptor::constructor(STRING_OPTION, "None", 0, 2, Vec::new())
                .expect("None layout"),
        ),
        Arc::new(
            ManagedAggregateDescriptor::constructor(
                STRING_OPTION,
                "Some",
                1,
                2,
                vec![(None, string)],
            )
            .expect("Some layout"),
        ),
        Arc::new(
            ManagedAggregateDescriptor::tuple(
                RESPONSE,
                vec![
                    ManagedFieldType::Int,
                    ManagedFieldType::Int,
                    string,
                    ManagedFieldType::Int,
                    string,
                    ManagedFieldType::Reference(semantic(RESPONSE_HEADERS)),
                ],
            )
            .expect("response layout"),
        ),
        Arc::new(
            ManagedAggregateDescriptor::tuple(RESPONSE_HEADER, vec![string, string])
                .expect("header layout"),
        ),
    ]
}

/// Builds the immutable image registry used by operation execution.
fn registry() -> ManagedLayoutRegistry {
    let layouts = aggregate_descriptors()
        .into_iter()
        .map(|descriptor| TvmManagedLayoutDescriptor {
            semantic_id: descriptor.managed().semantic_id().bytes(),
            encoded_layout: encode_aggregate_layout(&descriptor).expect("encode layout"),
        })
        .collect::<Vec<_>>();
    let string_map = ManagedCollectionDescriptor::map(
        STRING_MAP,
        ManagedFieldType::Reference(semantic("std.core.String")),
        ManagedFieldType::Reference(semantic("std.core.String")),
    )
    .expect("map schema");
    let headers = ManagedCollectionDescriptor::list(
        RESPONSE_HEADERS,
        ManagedFieldType::Reference(semantic(RESPONSE_HEADER)),
    )
    .expect("header list schema");
    let html_fragments = ManagedCollectionDescriptor::list(
        HTML_FRAGMENTS,
        ManagedFieldType::Reference(semantic("std.core.String")),
    )
    .expect("HTML fragment list schema");
    let int_list = ManagedCollectionDescriptor::list(INT_LIST, ManagedFieldType::Int)
        .expect("integer list schema");
    let collections = [string_map, headers, html_fragments, int_list].map(|collection| {
        TvmManagedCollectionDescriptor {
            semantic_id: collection.semantic_id().bytes(),
            encoded_layout: encode_collection_layout(&collection).expect("encode collection"),
        }
    });
    ManagedLayoutRegistry::from_image(&layouts, &collections, &[]).expect("layout registry")
}

/// Allocates one request and returns its request/map/string references.
fn request_fixture(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    entries: usize,
) -> (TvmRef<ManagedAggregate>, TvmRef<ManagedMap>) {
    let string_map = layouts
        .collection(semantic(STRING_MAP))
        .and_then(|collection| collection.map_descriptor())
        .expect("string map");
    let mut pairs = Vec::new();
    for index in 0..entries {
        let key = heap
            .allocate_string(&format!("key-{index}"))
            .expect("map key");
        let value = heap
            .allocate_string(&format!("value-{index}"))
            .expect("map value");
        pairs.push((
            ManagedFieldValue::Reference(key.erase()),
            ManagedFieldValue::Reference(value.erase()),
        ));
    }
    let map = heap
        .map_from_entries(string_map, &pairs, &mut PublicStringSemantics)
        .expect("request map");
    let method = heap.allocate_string("POST").expect("method");
    let path = heap.allocate_string("/users/42").expect("path");
    let body = heap.allocate_string("payload").expect("body");
    let query = heap.allocate_string("page=2").expect("query");
    let request = layouts.layouts(semantic(REQUEST))[0].clone();
    let request = heap
        .allocate_aggregate(
            request,
            &[
                ManagedFieldValue::Int(0),
                ManagedFieldValue::Reference(method.erase()),
                ManagedFieldValue::Reference(path.erase()),
                ManagedFieldValue::Reference(map.erase()),
                ManagedFieldValue::Reference(body.erase()),
                ManagedFieldValue::Reference(query.erase()),
                ManagedFieldValue::Reference(map.erase()),
                ManagedFieldValue::Reference(map.erase()),
                ManagedFieldValue::Reference(map.erase()),
            ],
        )
        .expect("request");
    (request, map)
}

/// Derives one checked semantic identity for fixture values.
fn semantic(canonical: &str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical).expect("semantic identity")
}

/// Converts one managed reference into the callback's signed native word.
fn word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

/// Converts one callback result into its opaque managed reference.
fn reference(result: u64) -> TvmRef<()> {
    TvmRef::from_encoded(std::num::NonZeroUsize::new(result as usize).expect("reference word"))
}

#[test]
fn string_list_join_handles_empty_inline_and_tree_profiles_in_order() {
    let layouts = registry();
    let descriptor = layouts
        .collection(semantic(HTML_FRAGMENTS))
        .and_then(|collection| collection.list_descriptor())
        .expect("HTML fragment list");
    let operation = encode_string_list_join_operation();
    assert!(managed_abi_result_is_reference(&operation));

    for count in [0usize, 3, 40] {
        let mut heap = heap();
        let references = (0..count)
            .map(|index| heap.allocate_string(&index.to_string()).expect("fragment"))
            .collect::<Vec<_>>();
        let fields = references
            .iter()
            .map(|reference| ManagedFieldValue::Reference(reference.erase()))
            .collect::<Vec<_>>();
        let list = heap
            .list_from_elements(descriptor, &fields)
            .expect("fragment list");
        let joined = execute_managed_operation(&mut heap, &layouts, &operation, &[word(list)])
            .map(reference)
            .expect("join fragments");
        let expected = (0..count)
            .map(|index| index.to_string())
            .collect::<String>();
        assert_eq!(heap.read_string(joined.cast()), Ok(expected.as_str()));
    }
}

#[test]
fn string_list_join_rejects_wrong_arity_and_non_string_lists() {
    let layouts = registry();
    let operation = encode_string_list_join_operation();
    let mut heap = heap();
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
    let descriptor = layouts
        .collection(semantic(INT_LIST))
        .and_then(|collection| collection.list_descriptor())
        .expect("integer list");
    let list = heap
        .list_from_elements(descriptor, &[ManagedFieldValue::Int(1)])
        .expect("integer list value");
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(list)]),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    );
}

#[test]
fn html_escape_operations_preserve_context_and_validate_arity() {
    let layouts = registry();
    let mut heap = heap();
    let input = heap
        .allocate_string("<a title=\"x&y\">'")
        .expect("input string");
    let text = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_string_escape_html_text_operation(),
        &[word(input)],
    )
    .map(reference)
    .expect("escape text");
    assert_eq!(
        heap.read_string(text.cast()),
        Ok("&lt;a&#32;title&#61;&quot;x&amp;y&quot;&gt;&apos;")
    );
    let attribute = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_string_escape_html_attribute_operation(),
        &[word(input)],
    )
    .map(reference)
    .expect("escape attribute");
    assert_eq!(
        heap.read_string(attribute.cast()),
        Ok("&lt;a title=&quot;x&amp;y&quot;&gt;'")
    );
    assert_eq!(
        execute_managed_operation(
            &mut heap,
            &layouts,
            &encode_string_escape_html_text_operation(),
            &[]
        ),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
}

#[test]
fn typed_template_rendering_enforces_attributes_options_and_urls() {
    let layouts = registry();
    let mut heap = heap();
    let href = heap.allocate_string("/users/7?x=1&y=2").expect("href");
    let href_operation =
        encode_template_render_operation(ManagedTemplateValueKind::String, Some("href"))
            .expect("href operation");
    let href = execute_managed_operation(&mut heap, &layouts, &href_operation, &[word(href)])
        .map(reference)
        .expect("render href");
    assert_eq!(
        heap.read_string(href.cast()),
        Ok(" href=\"/users/7?x=1&amp;y=2\"")
    );

    let disabled =
        encode_template_render_operation(ManagedTemplateValueKind::Bool, Some("disabled"))
            .expect("boolean operation");
    let present = execute_managed_operation(&mut heap, &layouts, &disabled, &[1])
        .map(reference)
        .expect("render present boolean");
    let absent = execute_managed_operation(&mut heap, &layouts, &disabled, &[0])
        .map(reference)
        .expect("render absent boolean");
    assert_eq!(heap.read_string(present.cast()), Ok(" disabled"));
    assert_eq!(heap.read_string(absent.cast()), Ok(""));
    let scalar_bool =
        encode_template_render_operation(ManagedTemplateValueKind::Bool, Some("data-enabled"))
            .expect("scalar boolean operation");
    let scalar_bool = execute_managed_operation(&mut heap, &layouts, &scalar_bool, &[1])
        .map(reference)
        .expect("render scalar boolean");
    assert_eq!(
        heap.read_string(scalar_bool.cast()),
        Ok(" data-enabled=\"true\"")
    );

    let value = heap.allocate_string("profile").expect("optional string");
    let some_layout = layouts
        .layouts(semantic(STRING_OPTION))
        .iter()
        .find(|layout| layout.variant_name() == Some("Some"))
        .cloned()
        .expect("Some layout");
    let none_layout = layouts
        .layouts(semantic(STRING_OPTION))
        .iter()
        .find(|layout| layout.variant_name() == Some("None"))
        .cloned()
        .expect("None layout");
    let some = heap
        .allocate_aggregate(some_layout, &[ManagedFieldValue::Reference(value.erase())])
        .expect("Some value");
    let none = heap
        .allocate_aggregate(none_layout, &[])
        .expect("None value");
    let optional =
        encode_template_render_operation(ManagedTemplateValueKind::OptionalString, Some("title"))
            .expect("optional operation");
    let present = execute_managed_operation(&mut heap, &layouts, &optional, &[word(some)])
        .map(reference)
        .expect("render Some");
    let absent = execute_managed_operation(&mut heap, &layouts, &optional, &[word(none)])
        .map(reference)
        .expect("render None");
    assert_eq!(heap.read_string(present.cast()), Ok(" title=\"profile\""));
    assert_eq!(heap.read_string(absent.cast()), Ok(""));

    let unsafe_url = heap
        .allocate_string("javascript:alert(1)")
        .expect("unsafe URL");
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &href_operation, &[word(unsafe_url)]),
        Err(ManagedMemoryError::InvalidManagedOperation)
    );
}

#[test]
fn aggregate_projection_reads_the_checked_request_field() {
    let layouts = registry();
    let mut heap = heap();
    let (request, _) = request_fixture(&mut heap, &layouts, 1);
    let operation = encode_aggregate_field_operation(semantic(REQUEST), 1).expect("operation");
    let result = execute_managed_operation(&mut heap, &layouts, &operation, &[word(request)])
        .expect("project method");
    let method: TvmRef<ManagedString> =
        TvmRef::from_encoded(std::num::NonZeroUsize::new(result as usize).expect("word"));
    assert_eq!(heap.read_string(method.cast()), Ok("POST"));
}

#[test]
fn aggregate_scalar_projection_returns_an_unboxed_native_word() {
    let layouts = registry();
    let mut heap = heap();
    let (request, _) = request_fixture(&mut heap, &layouts, 1);
    let operation =
        encode_aggregate_scalar_field_operation(semantic(REQUEST), 0).expect("operation");

    assert!(!managed_abi_result_is_reference(&operation));
    assert_eq!(
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(request)]),
        Ok(0)
    );
}

#[test]
fn string_map_lookup_allocates_some_and_none_for_indexed_maps() {
    let layouts = registry();
    let mut heap = heap();
    let (_, map) = request_fixture(&mut heap, &layouts, 129);
    let operation =
        encode_string_map_get_option_operation(semantic(STRING_MAP), semantic(STRING_OPTION));

    let present = heap.allocate_string("key-128").expect("present key");
    let present =
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(map), word(present)])
            .expect("Some lookup");
    let present = TvmRef::from_encoded(
        std::num::NonZeroUsize::new(present as usize).expect("Some reference"),
    );
    let present_layout = layouts
        .layout_for_reference(&heap, semantic(STRING_OPTION), present)
        .expect("Some layout");
    assert_eq!(present_layout.variant_name(), Some("Some"));
    let present_value = heap
        .read_aggregate(present.cast(), &present_layout)
        .expect("Some value")
        .field(0)
        .expect("Some field");
    let ManagedFieldValue::Reference(present_value) = present_value else {
        panic!("Some field must be a string reference");
    };
    assert_eq!(heap.read_string(present_value.cast()), Ok("value-128"));

    let missing = heap.allocate_string("missing").expect("missing key");
    let missing =
        execute_managed_operation(&mut heap, &layouts, &operation, &[word(map), word(missing)])
            .expect("None lookup");
    let missing = TvmRef::from_encoded(
        std::num::NonZeroUsize::new(missing as usize).expect("None reference"),
    );
    let missing_layout = layouts
        .layout_for_reference(&heap, semantic(STRING_OPTION), missing)
        .expect("None layout");
    assert_eq!(missing_layout.variant_name(), Some("None"));
}

#[test]
fn malformed_operations_and_wrong_shapes_are_rejected() {
    let layouts = registry();
    let mut heap = heap();
    let (request, _) = request_fixture(&mut heap, &layouts, 0);
    let mut operation = encode_aggregate_field_operation(semantic(REQUEST), 99).expect("operation");
    assert!(execute_managed_operation(&mut heap, &layouts, &operation, &[word(request)]).is_err());
    operation[7] = 1;
    assert!(execute_managed_operation(&mut heap, &layouts, &operation, &[word(request)]).is_err());
    assert!(execute_managed_operation(&mut heap, &layouts, b"TVMO", &[]).is_err());
}

#[test]
fn response_updates_are_persistent_and_preserve_repeated_headers() {
    let layouts = registry();
    let mut heap = heap();
    let empty = execute_managed_operation(
        &mut heap,
        &layouts,
        &encode_list_empty_operation(semantic(RESPONSE_HEADERS)),
        &[],
    )
    .map(reference)
    .expect("empty headers");
    let body = heap.allocate_string("body").expect("body");
    let content_type = heap.allocate_string("").expect("content type");
    let response_layout = layouts.layouts(semantic(RESPONSE))[0].clone();
    let original = heap
        .allocate_aggregate(
            response_layout.clone(),
            &[
                ManagedFieldValue::Int(0),
                ManagedFieldValue::Int(0),
                ManagedFieldValue::Reference(body.erase()),
                ManagedFieldValue::Int(200),
                ManagedFieldValue::Reference(content_type.erase()),
                ManagedFieldValue::Reference(empty),
            ],
        )
        .expect("response");
    let replace =
        encode_aggregate_replace_field_operation(semantic(RESPONSE), 3).expect("replace status");
    let updated = execute_managed_operation(&mut heap, &layouts, &replace, &[word(original), 201])
        .map(reference)
        .expect("updated status");
    let append = encode_aggregate_append_pair_operation(
        semantic(RESPONSE),
        semantic(RESPONSE_HEADERS),
        semantic(RESPONSE_HEADER),
        5,
    )
    .expect("append header");
    let name = heap.allocate_string("Set-Cookie").expect("header name");
    let first = heap.allocate_string("a=1").expect("first value");
    let second = heap.allocate_string("b=2").expect("second value");
    let updated = execute_managed_operation(
        &mut heap,
        &layouts,
        &append,
        &[word(updated), word(name), word(first)],
    )
    .map(reference)
    .expect("first header");
    let updated = execute_managed_operation(
        &mut heap,
        &layouts,
        &append,
        &[word(updated), word(name), word(second)],
    )
    .map(reference)
    .expect("second header");

    let original_view = heap
        .read_aggregate(original, &response_layout)
        .expect("original response");
    assert_eq!(original_view.field(3), Ok(ManagedFieldValue::Int(200)));
    let ManagedFieldValue::Reference(original_headers) = original_view.field(5).expect("headers")
    else {
        panic!("headers must be a reference");
    };
    let list = layouts
        .collection(semantic(RESPONSE_HEADERS))
        .and_then(|collection| collection.list_descriptor())
        .expect("header list");
    assert!(heap
        .list_is_empty(list, original_headers.cast::<ManagedList>())
        .expect("original list"));

    let updated_view = heap
        .read_aggregate(updated.cast(), &response_layout)
        .expect("updated response");
    assert_eq!(updated_view.field(3), Ok(ManagedFieldValue::Int(201)));
    let ManagedFieldValue::Reference(updated_headers) = updated_view.field(5).expect("headers")
    else {
        panic!("headers must be a reference");
    };
    let headers = heap
        .list_elements(list, updated_headers.cast())
        .expect("header values");
    assert_eq!(headers.len(), 2);
    for (header, expected) in headers.into_iter().zip(["a=1", "b=2"]) {
        let ManagedFieldValue::Reference(header) = header else {
            panic!("header must be an aggregate reference");
        };
        let header_layout = layouts.layouts(semantic(RESPONSE_HEADER))[0].clone();
        let header = heap
            .read_aggregate(header.cast(), &header_layout)
            .expect("header pair");
        let ManagedFieldValue::Reference(value) = header.field(1).expect("header value") else {
            panic!("header value must be a string");
        };
        assert_eq!(heap.read_string(value.cast()), Ok(expected));
    }
}
