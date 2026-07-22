
/// Returns a whitespace-normalized declaration line.
///
/// Inputs:
/// - `line`: raw source line containing a Rust item declaration.
///
/// Output:
/// - Single-line signature text for baseline comparison.
///
/// Transformation:
/// - Trims leading/trailing whitespace, collapses internal whitespace, and
///   removes trailing body/opening markers that are not part of the signature.
fn normalized_signature(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches([' ', '{'])
        .to_string()
}

/// Returns whether an item has adjacent Rustdoc.
///
/// Inputs:
/// - `lines`: source file split into lines.
/// - `item_index`: zero-based index of the item declaration line.
///
/// Output:
/// - `true` when `///`, `/** ... */`, or `//!` documentation is adjacent.
/// - `false` otherwise.
///
/// Transformation:
/// - Walks upward past attributes attached to the item and checks the closest
///   documentation comment block.
fn line_has_rustdoc(lines: &[String], item_index: usize) -> bool {
    let mut index = item_index;
    while index > 0 {
        let previous_index = index - 1;
        if lines[previous_index].trim().starts_with("#[") {
            index = previous_index;
        } else {
            break;
        }
    }
    if index == 0 {
        return false;
    }

    let mut doc_index = index - 1;
    let previous = lines[doc_index].trim();
    if previous.starts_with("///") || previous.starts_with("//!") {
        return true;
    }
    if previous.ends_with("*/") {
        loop {
            let text = lines[doc_index].trim();
            if text.starts_with("/**") || text.starts_with("/*!") {
                return true;
            }
            if text.starts_with("/*") {
                return false;
            }
            if doc_index == 0 {
                break;
            }
            doc_index -= 1;
        }
    }
    previous.starts_with("/**") || previous.starts_with("/*!")
}

/// Returns whether a line should be skipped as an escaped fixture string.
///
/// Inputs:
/// - `line`: current source line.
/// - `active`: whether the previous source line opened an escaped string.
///
/// Output:
/// - Updated escaped-string state.
/// - `true` when the current line is part of the string and should be skipped.
///
/// Transformation:
/// - Tracks Rust test fixtures written as `"\` followed by source-like lines
///   ending in `\n\`, which otherwise look like real Rust declarations.
fn escaped_string_state(line: &str, active: bool) -> (bool, bool) {
    let stripped = line.trim();
    if active {
        return (!stripped.ends_with("\","), true);
    }
    if stripped == r#""\"# {
        return (true, true);
    }
    (false, false)
}

/// Returns whether a line should be skipped as a Rust raw string literal.
///
/// Inputs:
/// - `line`: current source line.
/// - `terminator`: active raw-string terminator such as `"#`, or `None`.
/// - `raw_string_open_pattern`: compiled raw-string opener pattern.
///
/// Output:
/// - Updated raw-string terminator state.
/// - `true` when the current line is part of a raw string and should be
///   skipped.
///
/// Transformation:
/// - Tracks raw strings such as `r#"..."#` and `r###"..."###` so embedded
///   Terlan/Rust-like fixture text is not counted as real Rust declarations.
fn raw_string_state(
    line: &str,
    terminator: Option<&str>,
    raw_string_open_pattern: &Regex,
) -> (Option<String>, bool) {
    if let Some(terminator) = terminator {
        return (
            if line.contains(terminator) {
                None
            } else {
                Some(terminator.to_string())
            },
            true,
        );
    }

    let Some(raw_start) = raw_string_open_pattern.captures(line) else {
        return (None, false);
    };
    let Some(full_match) = raw_start.get(0) else {
        return (None, false);
    };
    let hashes = raw_start.get(1).map(|item| item.as_str()).unwrap_or("");
    let raw_terminator = format!("\"{hashes}");
    let remainder = &line[full_match.end()..];
    (
        if remainder.contains(&raw_terminator) {
            None
        } else {
            Some(raw_terminator)
        },
        true,
    )
}

/// Returns undocumented Rust items keyed by baseline identity.
///
/// Inputs:
/// - `items`: discovered Rust items.
///
/// Output:
/// - Mapping from baseline key to undocumented item.
///
/// Transformation:
/// - Filters documented declarations away and keeps the remaining item records
///   for diagnostics and baseline writing.
fn undocumented_items(items: &[RustItem]) -> BTreeMap<String, RustItem> {
    items
        .iter()
        .filter(|item| !item.documented)
        .map(|item| (item.key(), item.clone()))
        .collect()
}

/// Reads the undocumented Rustdoc migration baseline.
///
/// Inputs:
/// - `root`: repository root containing `tools/quality/rustdoc_missing_baseline.tsv`.
///
/// Output:
/// - Set of item keys allowed to remain undocumented.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses tab-separated path/kind/name/signature rows into comparable keys.
fn read_rustdoc_baseline(root: &Path) -> QualityResult<(BTreeSet<String>, Vec<String>)> {
    let path = rustdoc_baseline_path(root);
    let mut baseline = BTreeSet::new();
    let mut diagnostics = Vec::new();
    if !path.exists() {
        diagnostics.push(format!(
            "{}: missing baseline; run with --write-baseline",
            path.display()
        ));
        return Ok((baseline, diagnostics));
    }

    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read baseline: {err}", path.display()))?;
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.split('\t').count() != 4 {
            diagnostics.push(format!(
                "{}:{}: expected path<TAB>kind<TAB>name<TAB>signature",
                path.display(),
                index + 1
            ));
            continue;
        }
        baseline.insert(line.to_string());
    }
    Ok((baseline, diagnostics))
}

/// Validates undocumented items against the baseline.
///
/// Inputs:
/// - `current`: current undocumented Rust items.
/// - `baseline`: checked-in undocumented-item baseline keys.
///
/// Output:
/// - Diagnostics for new undocumented items and stale baseline entries.
///
/// Transformation:
/// - Treats existing undocumented declarations as migration debt while
///   blocking new undocumented functions or types from entering the tree.
fn check_rustdoc_baseline(
    current: &BTreeMap<String, RustItem>,
    baseline: &BTreeSet<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for key in baseline {
        if !current.contains_key(key) {
            diagnostics.push(format!("{key}: stale Rustdoc baseline row"));
        }
    }
    for (key, item) in current {
        if !baseline.contains(key) {
            diagnostics.push(format!(
                "{}:{}: undocumented {} `{}`; add Rustdoc or update reviewed baseline",
                item.path.display(),
                item.line,
                item.kind,
                item.name
            ));
        }
    }
    diagnostics
}

/// Returns the Rustdoc baseline path.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Path to `tools/quality/rustdoc_missing_baseline.tsv`.
///
/// Transformation:
/// - Centralizes the baseline path used by read and write paths.
fn rustdoc_baseline_path(root: &Path) -> PathBuf {
    root.join("tools")
        .join("quality")
        .join("rustdoc_missing_baseline.tsv")
}

/// Reads the file-size quality baseline.
///
/// Inputs:
/// - `root`: repository root containing `tools/quality/rust_file_size_baseline.tsv`.
///
/// Output:
/// - Mapping from repository-relative path to exact oversized-file line count.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses tab-separated path/count rows into typed baseline values.
fn read_size_baseline(root: &Path) -> QualityResult<(BTreeMap<PathBuf, usize>, Vec<String>)> {
    let path = root
        .join("tools")
        .join("quality")
        .join("rust_file_size_baseline.tsv");
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read baseline: {err}", path.display()))?;
    let mut baseline = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 2 {
            diagnostics.push(format!(
                "{}:{}: expected path<TAB>lines",
                path.display(),
                index + 1
            ));
            continue;
        }
        match fields[1].parse::<usize>() {
            Ok(lines) => {
                baseline.insert(PathBuf::from(fields[0]), lines);
            }
            Err(_) => diagnostics.push(format!(
                "{}:{}: invalid line count `{}`",
                path.display(),
                index + 1,
                fields[1]
            )),
        }
    }
    Ok((baseline, diagnostics))
}

/// Reads the inline-test quality baseline.
///
/// Inputs:
/// - `root`: repository root containing `tools/quality/rust_inline_test_baseline.txt`.
///
/// Output:
/// - Set of repository-relative paths allowed to contain `#[cfg(test)]`.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses one path per line while allowing comments and blank lines.
fn read_inline_test_baseline(root: &Path) -> QualityResult<(BTreeSet<PathBuf>, Vec<String>)> {
    let path = root
        .join("tools")
        .join("quality")
        .join("rust_inline_test_baseline.txt");
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read baseline: {err}", path.display()))?;
    let mut baseline = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.contains('\t') {
            diagnostics.push(format!(
                "{}:{}: expected one path per line",
                path.display(),
                index + 1
            ));
            continue;
        }
        baseline.insert(PathBuf::from(line));
    }
    Ok((baseline, diagnostics))
}

/// Returns implementation files that contain inline Rust test configuration.
///
/// Inputs:
/// - `root`: repository root.
/// - `files`: measured Rust files.
///
/// Output:
/// - Repository-relative implementation paths containing `#[cfg(test)]`.
///
/// Transformation:
/// - Ignores adjacent `*_test.rs` test modules because those are the required
///   test layout.
/// - Reads implementation Rust source files and allows adjacent path-based
///   test modules while rejecting other inline test configuration markers.
fn files_with_inline_tests(root: &Path, files: &[RustFile]) -> QualityResult<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    for file in files {
        let logical_path = logical_rust_source_path(root, &file.path);
        if logical_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_test.rs"))
        {
            continue;
        }
        let text = fs::read_to_string(root.join(&file.path)).map_err(|err| {
            format!(
                "{}: failed to read source: {err}",
                root.join(&file.path).display()
            )
        })?;
        if has_inline_test_marker(&text) {
            paths.insert(logical_path);
        }
    }
    Ok(paths)
}

/// Resolves a numbered include fragment to the Rust module that owns it.
///
/// Files named `*_part_NNN.rs` are physical slices of the adjacent wrapper,
/// not independent module boundaries. Policy checks therefore attribute their
/// inline test markers to the wrapper while file-size checks still measure
/// every physical file independently.
fn logical_rust_source_path(root: &Path, path: &Path) -> PathBuf {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return path.to_path_buf();
    };
    let Some((owner_stem, part)) = stem.rsplit_once("_part_") else {
        return path.to_path_buf();
    };
    if part.len() != 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return path.to_path_buf();
    }
    let owner = path.with_file_name(format!("{owner_stem}.rs"));
    if root.join(&owner).is_file() {
        owner
    } else {
        path.to_path_buf()
    }
}

/// Validates line-count limits against the baseline.
///
/// Inputs:
/// - Current measured Rust files.
/// - Baseline maximum line counts.
///
/// Output:
/// - Diagnostics for new oversized files, any baseline drift, obsolete rows,
///   and stale rows.
///
/// Transformation:
/// - Enforces hard limits for new files and an exact ratchet for existing debt,
///   so reductions immediately remove all headroom for later growth.
fn check_file_sizes(files: &[RustFile], baseline: &BTreeMap<PathBuf, usize>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let current = files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();

    for (path, allowed) in baseline {
        let Some(file) = current.get(path) else {
            diagnostics.push(format!("{}: stale file-size baseline row", path.display()));
            continue;
        };
        if file.lines <= file.limit() {
            diagnostics.push(format!(
                "{}: obsolete file-size baseline row; {} lines is within limit {}",
                path.display(),
                file.lines,
                file.limit()
            ));
        } else if file.lines > *allowed {
            diagnostics.push(format!(
                "{}: {} lines exceeds baseline {allowed}; split before adding code",
                path.display(),
                file.lines
            ));
        } else if file.lines < *allowed {
            diagnostics.push(format!(
                "{}: baseline {allowed} exceeds current {} lines; lower baseline to preserve the reduction",
                path.display(),
                file.lines
            ));
        }
    }

    for file in files {
        let limit = file.limit();
        if file.lines <= limit {
            continue;
        }
        if !baseline.contains_key(&file.path) {
            diagnostics.push(format!(
                "{}: {} lines exceeds {limit}; split file or add reviewed baseline",
                file.path.display(),
                file.lines
            ));
        }
    }
    diagnostics
}

/// Validates inline test usage against the baseline.
///
/// Inputs:
/// - Current files containing `#[cfg(test)]`.
/// - Baseline files allowed to contain inline tests.
///
/// Output:
/// - Diagnostics for new inline test files and stale baseline rows.
///
/// Transformation:
/// - Prevents new inline-test debt while allowing current debt to be migrated
///   out over time.
fn check_inline_tests(current: &BTreeSet<PathBuf>, baseline: &BTreeSet<PathBuf>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for path in baseline {
        if !current.contains(path) {
            diagnostics.push(format!(
                "{}: stale inline-test baseline row",
                path.display()
            ));
        }
    }
    for path in current {
        if !baseline.contains(path) {
            diagnostics.push(format!(
                "{}: new inline #[cfg(test)] block; move tests to adjacent *_test.rs",
                path.display()
            ));
        }
    }
    diagnostics
}

/// Renders a named failure block.
///
/// Inputs:
/// - `name`: check label.
/// - `diagnostics`: diagnostic messages.
///
/// Output:
/// - Stable multi-line failure message.
///
/// Transformation:
/// - Preserves the previous Python script's output shape so Makefile and CI
///   logs remain familiar.
fn render_failure(name: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{name}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}
#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
