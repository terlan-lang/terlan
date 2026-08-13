//! Source-map projections for one stopped native continuation.

use crate::runtime::native_image::debug::TvmNativeDebugRecord;

#[derive(Clone, Copy)]
pub(super) enum BreakpointAction {
    Enable,
    Disable,
    Remove,
}

pub(super) fn source_for_continuation(
    records: &[TvmNativeDebugRecord],
    fallback: &TvmNativeDebugRecord,
    continuation_id: u64,
) -> TvmNativeDebugRecord {
    let mut record = records
        .iter()
        .find(|record| record.continuation_ids.contains(&continuation_id))
        .cloned()
        .unwrap_or_else(|| fallback.clone());
    if let Some(continuation) = record
        .continuation_spans
        .iter()
        .find(|continuation| continuation.id == continuation_id)
    {
        record.span_start = continuation.span_start;
        record.span_end = continuation.span_end;
    }
    record
}

pub(super) fn continuation_local_names(
    record: &TvmNativeDebugRecord,
    instruction_offset: usize,
) -> &[String] {
    u64::try_from(instruction_offset)
        .ok()
        .and_then(|id| {
            record
                .continuation_spans
                .iter()
                .find(|continuation| continuation.id == id)
        })
        .map_or(&[], |continuation| continuation.local_names.as_slice())
}
