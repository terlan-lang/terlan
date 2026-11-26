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

#[test]
fn semantic_inventory_is_sorted_deduplicated_and_explicitly_empty() {
    assert!(native_semantic_inventory_event(&[], &[]).contains(
        "semantic-types=0 aggregate-layouts=0 collection-layouts=0 semantic-type-ids=none"
    ));

    let aggregates = [[2; 16], [1; 16]];
    let collections = [[2; 16]];
    assert_eq!(
        native_semantic_inventory_event(&aggregates, &collections),
        concat!(
            "terlc native-aot: image.types: semantic-types=2 aggregate-layouts=2 ",
            "collection-layouts=1 semantic-type-ids=01010101010101010101010101010101,",
            "02020202020202020202020202020202",
        )
    );
}
