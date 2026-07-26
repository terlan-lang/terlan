//! Actor-local managed memory used by direct-AOT native images.

mod aggregate_abi;
mod aggregates;
mod atoms;
mod closure_abi;
mod closure_dispatch;
mod closures;
mod collection_abi;
mod core;
mod execution;
mod heap;
mod layout;
mod layout_registry;
mod lists;
mod literal_abi;
mod mailbox;
mod maps;
mod operation_abi;
mod roots;
mod sequences;
mod sets;
mod slots;

pub use atoms::{AtomIndex, AtomTable};
pub(crate) use closure_abi::encode_closure_allocation;
pub(crate) use closure_abi::{execute_closure_allocation, is_closure_allocation};
pub use closure_dispatch::{
    ManagedClosureDispatchTable, ManagedClosureInvocation, ManagedClosureTarget,
};
pub use closures::{
    ManagedClosure, ManagedClosureDescriptor, ManagedClosureImageGeneration, ManagedClosureView,
};
pub use collection_abi::{
    decode_collection_layout, encode_collection_layout, ManagedCollectionDescriptor,
    ManagedCollectionKind, MAX_MANAGED_COLLECTION_ABI_BYTES,
};
pub use core::{ActorId, ManagedMemoryError, TvmRef};
pub(crate) use execution::{ManagedActorTransfer, ManagedExecutionRuntime, PendingManagedCaptures};
pub use heap::{ActorHeap, CollectionStats, HeapLimits};
pub use layout::{
    managed_binary_semantic_id, managed_bytes_semantic_id, managed_string_semantic_id,
    AllocationClass, LayoutFingerprint, ManagedTypeDescriptor, SemanticTypeId,
};
pub(crate) use layout_registry::ManagedLayoutRegistry;
pub use lists::{ManagedList, ManagedListBuilder, ManagedListDescriptor, ManagedListProfile};
pub use literal_abi::{encode_string_literal, MAX_MANAGED_LITERAL_ABI_BYTES};
pub use mailbox::ManagedMailboxFragment;
pub use maps::{
    ManagedKeySemantics, ManagedMap, ManagedMapDescriptor, ManagedMapProfile,
    ManagedScalarKeySemantics, ManagedStringKeySemantics,
};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use operation_abi::execute_managed_operation;
pub(crate) use operation_abi::{
    decode_aggregate_field_projection, execute_managed_operation_with_context,
    managed_abi_result_is_reference, scalar_string_projection_rewrite,
};
pub use operation_abi::{
    encode_aggregate_append_pair_operation, encode_aggregate_append_value_operation,
    encode_aggregate_field_operation, encode_aggregate_replace_field_operation,
    encode_aggregate_scalar_field_operation, encode_binary_pattern_extract_operation,
    encode_binary_pattern_matches_operation, encode_bitstring_operation,
    encode_bytes_concat_operation, encode_bytes_from_list_operation, encode_bytes_length_operation,
    encode_bytes_read_int_be_operation, encode_bytes_read_int_le_operation,
    encode_bytes_read_uint_be_operation, encode_bytes_read_uint_le_operation,
    encode_bytes_slice_operation, encode_bytes_to_list_operation, encode_cookie_header_operation,
    encode_float_from_string_operation, encode_float_log_operation,
    encode_float_to_string_operation, encode_int_from_string_base_operation,
    encode_int_from_string_operation, encode_int_to_string_base_operation,
    encode_int_to_string_operation, encode_iterator_next_operation,
    encode_json_parse_result_operation, encode_list_append_operation, encode_list_empty_operation,
    encode_list_first_operation, encode_list_first_option_operation,
    encode_list_from_elements_operation, encode_list_get_operation, encode_list_is_empty_operation,
    encode_list_length_operation, encode_list_prepend_operation, encode_list_rest_operation,
    encode_list_rest_option_operation, encode_managed_type_is_operation,
    encode_managed_value_equal_operation, encode_managed_variant_is_operation,
    encode_map_clear_operation, encode_map_contains_operation, encode_map_empty_operation,
    encode_map_from_entries_operation, encode_map_from_entry_list_operation,
    encode_map_get_operation, encode_map_get_option_operation, encode_map_is_empty_operation,
    encode_map_iterator_operation, encode_map_length_operation, encode_map_put_operation,
    encode_map_remove_operation, encode_map_take_operation, encode_response_build_operation,
    encode_response_cookie_jar_operation, encode_response_security_headers_operation,
    encode_result_is_ok_operation, encode_session_current_operation,
    encode_session_expire_operation, encode_session_get_operation,
    encode_session_mutation_operation, encode_session_option_is_none_operation,
    encode_session_rotate_operation, encode_session_with_response_operation,
    encode_string_append_operation, encode_string_concat_operation, encode_string_equal_operation,
    encode_string_escape_html_attribute_operation, encode_string_escape_html_text_operation,
    encode_string_list_join_operation, encode_string_map_get_option_operation,
    encode_string_prepend_literal_operation, encode_string_prepend_projected_literal_operation,
    encode_template_render_operation, is_managed_operation, ManagedBinaryPatternEndian,
    ManagedBinaryPatternField, ManagedBitStringOperation, ManagedCookieHeaderOperation,
    ManagedSessionMutation, ManagedTemplateValueKind,
};
pub use roots::{
    ManagedContinuation, ManagedRoot, RootLocation, StackMapEntry, StackMapRecord, StackMapTable,
    StackRootKind,
};
pub use sequences::{
    ManagedBinary, ManagedBinaryView, ManagedBytes, ManagedString, MANAGED_SEQUENCE_HEADER_BYTES,
};
pub use sets::{ManagedSet, ManagedSetDescriptor};

#[cfg(test)]
#[path = "gc_suite_parity_test.rs"]
mod gc_suite_parity_test;
#[cfg(test)]
#[path = "mailbox_test.rs"]
mod mailbox_test;
#[cfg(test)]
#[path = "managed_closure_dispatch_test.rs"]
mod managed_closure_dispatch_test;
#[cfg(test)]
#[path = "managed_closure_test.rs"]
mod managed_closure_test;
#[cfg(test)]
#[path = "managed_execution_test.rs"]
mod managed_execution_test;
#[cfg(test)]
#[path = "managed_test.rs"]
mod managed_test;
pub use aggregate_abi::{
    decode_aggregate_layout, encode_aggregate_layout, MANAGED_ALLOCATION_FAILED_STATUS,
    MAX_MANAGED_AGGREGATE_ABI_BYTES,
};
pub use aggregates::{
    ManagedAggregate, ManagedAggregateDescriptor, ManagedAggregateKind, ManagedAggregateView,
    ManagedFieldDescriptor, ManagedFieldType, ManagedFieldValue, ManagedProductKind,
};
