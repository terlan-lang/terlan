use std::path::Path;

/// Asset include/exclude filters for static output copying.
///
/// Inputs:
/// - `includes`: optional wildcard patterns that allow matching assets.
/// - `excludes`: wildcard patterns that reject matching assets.
///
/// Output:
/// - Filter state consumed by static asset copying.
///
/// Transformation:
/// - A path is allowed when it matches at least one include, or no includes are
///   configured, and does not match any exclude.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AssetFilters {
    pub(crate) includes: Vec<String>,
    pub(crate) excludes: Vec<String>,
}

impl AssetFilters {
    /// Returns whether a static asset path should be copied.
    ///
    /// Inputs:
    /// - `path`: resolved asset source path.
    ///
    /// Output:
    /// - `true` when include/exclude rules allow the path.
    ///
    /// Transformation:
    /// - Matches patterns against both normalized full paths and file names.
    pub(crate) fn allows(&self, path: &Path) -> bool {
        let included = self.includes.is_empty()
            || self
                .includes
                .iter()
                .any(|pattern| asset_pattern_matches(pattern, path));
        let excluded = self
            .excludes
            .iter()
            .any(|pattern| asset_pattern_matches(pattern, path));

        included && !excluded
    }
}

/// Returns whether an asset pattern matches a path.
///
/// Inputs:
/// - `pattern`: wildcard pattern.
/// - `path`: resolved path to test.
///
/// Output:
/// - `true` when the pattern matches the normalized path or filename.
///
/// Transformation:
/// - Normalizes separators to `/` before wildcard matching.
fn asset_pattern_matches(pattern: &str, path: &Path) -> bool {
    let normalized_path = path.to_string_lossy().replace('\\', "/");
    if wildcard_match(pattern, &normalized_path) {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| wildcard_match(pattern, name))
}

/// Matches a simple `*` wildcard pattern.
///
/// Inputs:
/// - `pattern`: wildcard pattern with zero or more `*` wildcards.
/// - `value`: candidate text.
///
/// Output:
/// - `true` when `value` satisfies `pattern`.
///
/// Transformation:
/// - Performs ordered substring matching with anchored edges when the pattern
///   does not start or end with `*`.
fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }

    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let mut rest = value;
    let mut parts = pattern
        .split('*')
        .filter(|part| !part.is_empty())
        .peekable();

    if !starts_with_wildcard {
        let Some(first) = parts.next() else {
            return true;
        };
        let Some(stripped) = rest.strip_prefix(first) else {
            return false;
        };
        rest = stripped;
    }

    while let Some(part) = parts.next() {
        if parts.peek().is_none() && !ends_with_wildcard {
            return rest.ends_with(part);
        }
        let Some(position) = rest.find(part) else {
            return false;
        };
        rest = &rest[position + part.len()..];
    }

    true
}

#[cfg(test)]
#[path = "filters_test.rs"]
mod filters_test;
