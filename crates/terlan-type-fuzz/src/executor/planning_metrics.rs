use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct CompilerMetrics {
    timing_events: usize,
    last_phase: Option<String>,
    phase_elapsed_ms: BTreeMap<String, u64>,
    reachable_declarations: Option<u64>,
    native_providers: BTreeSet<String>,
    cache_miss_reasons: BTreeMap<String, u64>,
    generic_specializations: u64,
    call_profiles: u64,
    lowered_calls: u64,
    lowering_cache_hits: u64,
    native_images: u64,
    native_image_cache_hits: u64,
    native_image_cache_misses: u64,
    semantic_types: u64,
    aggregate_layouts: u64,
    collection_layouts: u64,
    semantic_inventory_events: u64,
    semantic_type_ids: BTreeSet<String>,
    native_units_built: u64,
    native_functions_built: u64,
    native_continuations_built: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct CompilerMetricDelta {
    generic_specializations: i128,
    call_profiles: i128,
    lowered_calls: i128,
    lowering_cache_hits: i128,
    native_images: i128,
    native_image_cache_hits: i128,
    native_image_cache_misses: i128,
    semantic_types: i128,
    aggregate_layouts: i128,
    collection_layouts: i128,
    native_units_built: i128,
    native_functions_built: i128,
    native_continuations_built: i128,
}

pub(super) fn measure(stderr: &str) -> CompilerMetrics {
    let mut metrics = CompilerMetrics::default();
    for line in stderr.lines() {
        let prefix = if line.starts_with("terlc timing:") {
            Some("terlc timing:")
        } else if line.starts_with("terlc native-aot:") {
            Some("terlc native-aot:")
        } else {
            None
        };
        let Some(prefix) = prefix else { continue };
        metrics.timing_events += 1;
        if let Some(phase) = phase_name(line, prefix) {
            metrics.last_phase = Some(phase);
        }
        if let Some((phase, elapsed)) = phase_elapsed(line, prefix) {
            let total = metrics.phase_elapsed_ms.entry(phase).or_default();
            *total = total.saturating_add(elapsed);
        }
        if line.starts_with("terlc native-aot: generic-specialization.") {
            metrics.generic_specializations = metrics
                .generic_specializations
                .saturating_add(field(line, "specializations=").unwrap_or(0));
        }
        if line.contains("profiles=") && line.contains("lowered=") {
            metrics.call_profiles = metrics
                .call_profiles
                .max(field(line, "profiles=").unwrap_or(0));
            metrics.lowered_calls = metrics
                .lowered_calls
                .max(field(line, "lowered=").unwrap_or(0));
            metrics.lowering_cache_hits = metrics
                .lowering_cache_hits
                .max(field(line, "cache-hits=").unwrap_or(0));
        }
        if line.starts_with("terlc native-aot: module-unit.begin.") {
            metrics.native_units_built += 1;
            metrics.native_functions_built = metrics
                .native_functions_built
                .saturating_add(field(line, "functions=").unwrap_or(0));
            metrics.native_continuations_built = metrics
                .native_continuations_built
                .saturating_add(field(line, "continuations=").unwrap_or(0));
        }
        if line.starts_with("terlc native-aot: image.cache-or-build:") {
            metrics.native_images += 1;
        }
        if line.starts_with("terlc native-aot: publish.publish:") {
            metrics.native_image_cache_misses += 1;
        }
        if let Some(count) = field(line, "reachable-declarations=") {
            metrics.reachable_declarations = Some(count);
        }
        if let Some(providers) = text_field(line, "native-providers=") {
            metrics.native_providers.extend(
                providers
                    .split(',')
                    .filter(|provider| !provider.is_empty() && *provider != "none")
                    .map(str::to_string),
            );
        }
        if let Some(reason) = text_field(line, "cache-miss-reason=") {
            let count = metrics
                .cache_miss_reasons
                .entry(reason.to_string())
                .or_default();
            *count = count.saturating_add(1);
        }
        if text_field(line, "cache-hit=").is_some() {
            metrics.native_image_cache_hits = metrics.native_image_cache_hits.saturating_add(1);
        }
        if line.starts_with("terlc native-aot: image.types:") {
            metrics.semantic_inventory_events = metrics.semantic_inventory_events.saturating_add(1);
            metrics.semantic_types = metrics
                .semantic_types
                .saturating_add(field(line, "semantic-types=").unwrap_or(0));
            metrics.aggregate_layouts = metrics
                .aggregate_layouts
                .saturating_add(field(line, "aggregate-layouts=").unwrap_or(0));
            metrics.collection_layouts = metrics
                .collection_layouts
                .saturating_add(field(line, "collection-layouts=").unwrap_or(0));
            if let Some(identities) = text_field(line, "semantic-type-ids=") {
                metrics.semantic_type_ids.extend(
                    identities
                        .split(',')
                        .filter(|identity| !identity.is_empty() && *identity != "none")
                        .map(str::to_string),
                );
            }
        }
    }
    metrics.native_image_cache_hits = metrics.native_image_cache_hits.saturating_add(
        metrics
            .native_images
            .saturating_sub(metrics.native_image_cache_misses),
    );
    metrics
}

pub(super) fn attribution_gaps(metrics: &CompilerMetrics) -> Vec<&'static str> {
    let mut gaps = Vec::new();
    if metrics.phase_elapsed_ms.is_empty() {
        gaps.push("phase timings");
    }
    if metrics.reachable_declarations.is_none() {
        gaps.push("reachable declarations");
    }
    if metrics.native_image_cache_hits == 0 && metrics.cache_miss_reasons.is_empty() {
        gaps.push("cache disposition");
    }
    if metrics.semantic_inventory_events == 0 {
        gaps.push("semantic type inventory");
    }
    gaps
}

pub(super) fn has_native_image_cache_hit(metrics: &CompilerMetrics) -> bool {
    metrics.native_image_cache_hits > 0
}

pub(super) fn combined_work_is_linear(
    combined: &CompilerMetrics,
    focused: &CompilerMetrics,
) -> bool {
    let bounded =
        |combined: u64, focused: u64| combined <= focused.saturating_mul(2).saturating_add(2);
    bounded(
        combined.generic_specializations,
        focused.generic_specializations,
    ) && bounded(combined.call_profiles, focused.call_profiles)
        && bounded(combined.lowered_calls, focused.lowered_calls)
        && bounded(combined.native_units_built, focused.native_units_built)
        && bounded(
            combined.native_functions_built,
            focused.native_functions_built,
        )
        && bounded(
            combined.native_continuations_built,
            focused.native_continuations_built,
        )
        && bounded(combined.semantic_types, focused.semantic_types)
        && bounded(combined.aggregate_layouts, focused.aggregate_layouts)
        && bounded(combined.collection_layouts, focused.collection_layouts)
        && bounded(
            combined.semantic_type_ids.len() as u64,
            focused.semantic_type_ids.len() as u64,
        )
        && combined.native_providers == focused.native_providers
        && match (
            combined.reachable_declarations,
            focused.reachable_declarations,
        ) {
            (Some(combined), Some(focused)) => bounded(combined, focused),
            (None, None) => true,
            _ => false,
        }
}

pub(super) fn delta(right: &CompilerMetrics, left: &CompilerMetrics) -> CompilerMetricDelta {
    CompilerMetricDelta {
        generic_specializations: difference(
            right.generic_specializations,
            left.generic_specializations,
        ),
        call_profiles: difference(right.call_profiles, left.call_profiles),
        lowered_calls: difference(right.lowered_calls, left.lowered_calls),
        lowering_cache_hits: difference(right.lowering_cache_hits, left.lowering_cache_hits),
        native_images: difference(right.native_images, left.native_images),
        native_image_cache_hits: difference(
            right.native_image_cache_hits,
            left.native_image_cache_hits,
        ),
        native_image_cache_misses: difference(
            right.native_image_cache_misses,
            left.native_image_cache_misses,
        ),
        semantic_types: difference(right.semantic_types, left.semantic_types),
        aggregate_layouts: difference(right.aggregate_layouts, left.aggregate_layouts),
        collection_layouts: difference(right.collection_layouts, left.collection_layouts),
        native_units_built: difference(right.native_units_built, left.native_units_built),
        native_functions_built: difference(
            right.native_functions_built,
            left.native_functions_built,
        ),
        native_continuations_built: difference(
            right.native_continuations_built,
            left.native_continuations_built,
        ),
    }
}

pub(super) fn same_structural_work(right: &CompilerMetrics, left: &CompilerMetrics) -> bool {
    (
        right.generic_specializations,
        right.call_profiles,
        right.lowered_calls,
        right.native_units_built,
        right.native_functions_built,
        right.native_continuations_built,
        right.semantic_types,
        right.aggregate_layouts,
        right.collection_layouts,
        &right.semantic_type_ids,
        right.reachable_declarations,
        &right.native_providers,
    ) == (
        left.generic_specializations,
        left.call_profiles,
        left.lowered_calls,
        left.native_units_built,
        left.native_functions_built,
        left.native_continuations_built,
        left.semantic_types,
        left.aggregate_layouts,
        left.collection_layouts,
        &left.semantic_type_ids,
        left.reachable_declarations,
        &left.native_providers,
    )
}

fn phase_name(line: &str, prefix: &str) -> Option<String> {
    line.strip_prefix(prefix)?
        .trim_start()
        .split_once(':')
        .map(|(phase, _)| phase.to_string())
}

fn phase_elapsed(line: &str, prefix: &str) -> Option<(String, u64)> {
    let body = line
        .strip_prefix(prefix)
        .expect("timing prefix was selected from this line");
    let (phase, _) = body.trim_start().split_once(':')?;
    let elapsed = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix('+')?.strip_suffix("ms")?.parse().ok())?;
    Some((phase.to_string(), elapsed))
}

fn field(line: &str, prefix: &str) -> Option<u64> {
    line.split_whitespace().find_map(|value| {
        value
            .strip_prefix(prefix)?
            .trim_end_matches(|character: char| !character.is_ascii_digit())
            .parse()
            .ok()
    })
}

fn text_field<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|value| value.strip_prefix(prefix))
        .map(|value| value.trim_end_matches([',', ';']))
}

fn difference(right: u64, left: u64) -> i128 {
    i128::from(right) - i128::from(left)
}

#[cfg(test)]
#[path = "planning_metrics_test.rs"]
mod tests;
