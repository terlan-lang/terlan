use std::path::Path;

use super::{asset_pattern_matches, wildcard_match, AssetFilters};

/// Verifies empty include filters allow assets by default.
///
/// Inputs:
/// - Default filter set and an arbitrary static asset path.
///
/// Output:
/// - `true` when no include or exclude rule constrains the path.
///
/// Transformation:
/// - Exercises the default-copy behavior used by static emit.
#[test]
fn asset_filters_allow_by_default() {
    let filters = AssetFilters::default();

    assert!(filters.allows(Path::new("assets/app.css")));
}

/// Verifies include rules must match when configured.
///
/// Inputs:
/// - Filter set with one JavaScript include pattern.
///
/// Output:
/// - JavaScript asset allowed; CSS asset rejected.
///
/// Transformation:
/// - Applies include matching before exclude checks.
#[test]
fn asset_filters_require_include_match_when_includes_exist() {
    let filters = AssetFilters {
        includes: vec!["*.js".to_owned()],
        excludes: Vec::new(),
    };

    assert!(filters.allows(Path::new("assets/app.js")));
    assert!(!filters.allows(Path::new("assets/app.css")));
}

/// Verifies exclude rules override matching include rules.
///
/// Inputs:
/// - Filter set that includes all CSS and excludes generated CSS.
///
/// Output:
/// - Hand-authored CSS allowed; generated CSS rejected.
///
/// Transformation:
/// - Confirms excludes are applied after include eligibility.
#[test]
fn asset_filters_exclude_overrides_include() {
    let filters = AssetFilters {
        includes: vec!["*.css".to_owned()],
        excludes: vec!["*generated*".to_owned()],
    };

    assert!(filters.allows(Path::new("assets/site.css")));
    assert!(!filters.allows(Path::new("assets/site.generated.css")));
}

/// Verifies asset patterns match normalized full paths and file names.
///
/// Inputs:
/// - Path-style and filename-style patterns.
///
/// Output:
/// - Matches for normalized slash paths and basename-only patterns.
///
/// Transformation:
/// - Normalizes platform separators before wildcard comparison.
#[test]
fn asset_pattern_matches_normalized_paths_and_file_names() {
    assert!(asset_pattern_matches(
        "assets/*.css",
        Path::new("assets/site.css")
    ));
    assert!(asset_pattern_matches(
        "site.css",
        Path::new("assets/site.css")
    ));
    assert!(asset_pattern_matches(
        "assets/*.css",
        Path::new("assets\\site.css")
    ));
    assert!(!asset_pattern_matches(
        "assets/*.css",
        Path::new("public/site.css")
    ));
}

/// Verifies wildcard matching honors anchored pattern edges.
///
/// Inputs:
/// - Prefix, suffix, contains, and exact wildcard patterns.
///
/// Output:
/// - Stable match decisions for anchored and unanchored forms.
///
/// Transformation:
/// - Exercises the ordered substring matcher used by asset filters.
#[test]
fn wildcard_match_honors_anchored_edges() {
    assert!(wildcard_match("*", "anything"));
    assert!(wildcard_match("app.*", "app.js"));
    assert!(!wildcard_match("app.*", "vendor/app.js"));
    assert!(wildcard_match("*.css", "site.css"));
    assert!(!wildcard_match("*.css", "site.css.map"));
    assert!(wildcard_match("*site*", "assets/site.css"));
    assert!(wildcard_match("assets/*/site.css", "assets/css/site.css"));
    assert!(!wildcard_match("assets/*/site.css", "public/css/site.css"));
}

/// Verifies wildcard matching rejects out-of-order segments.
///
/// Inputs:
/// - Pattern whose literal pieces appear in the wrong order in the value.
///
/// Output:
/// - `false` because wildcard segments must be ordered.
///
/// Transformation:
/// - Prevents broad contains-matching from ignoring segment order.
#[test]
fn wildcard_match_rejects_out_of_order_segments() {
    assert!(!wildcard_match("*app*site*", "site/app.css"));
}
