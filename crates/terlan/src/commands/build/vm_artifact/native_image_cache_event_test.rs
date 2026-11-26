use super::*;

#[test]
fn rooted_cache_events_attribute_every_outcome() {
    assert!(rooted_cache_event(RootedCacheDisposition::Disabled)
        .contains("cache-miss-reason=incremental_reuse_disabled"));
    assert!(rooted_cache_event(RootedCacheDisposition::Invalidated)
        .contains("cache-miss-reason=checked_core_or_artifact_changed"));
    assert!(rooted_cache_event(RootedCacheDisposition::Hit)
        .contains("cache-hit=verified_checked_core_image"));
    trace_rooted_cache(false, RootedCacheDisposition::Hit);
    trace_rooted_cache(true, RootedCacheDisposition::Hit);
}
