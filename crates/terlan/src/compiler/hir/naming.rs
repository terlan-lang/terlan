/// Derives the SafeNative backend module from a Terlan module path.
///
/// Inputs:
/// - `module`: source module path such as `std.data.Json`.
///
/// Output:
/// - Lower-snake SafeNative module name such as `std_data_json_safe_native`.
///
/// Transformation:
/// - Converts each path segment to lower snake case, joins segments with
///   underscores, and appends the SafeNative suffix.
pub fn module_path_to_safe_native_module(module: &str) -> String {
    let base = module
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(identifier_to_snake)
        .collect::<Vec<_>>()
        .join("_");
    format!("{base}_safe_native")
}

/// Converts one identifier segment to lower snake case.
///
/// Inputs:
/// - `segment`: module path segment in Terlan casing.
///
/// Output:
/// - Lower-snake representation.
///
/// Transformation:
/// - Inserts underscores before uppercase boundaries, including acronym-to-word
///   boundaries such as `HTMLElement`, and lowers alphabetic characters.
pub fn identifier_to_snake(segment: &str) -> String {
    normalize_identifier_to_snake(segment, false)
}

/// Converts an external source name into a Terlan identifier.
///
/// Inputs:
/// - `name`: external API name, commonly JavaScript/TypeScript camelCase,
///   PascalCase, acronym-heavy, or symbol-containing text.
///
/// Output:
/// - Lower-snake Terlan identifier that starts with a lowercase character and
///   does not collide with Terlan keywords.
///
/// Transformation:
/// - Normalizes acronym and camel-case boundaries, converts unsupported
///   characters to underscores, prefixes invalid starts with `value_`, and
///   appends `_` for keyword collisions.
pub fn source_name_to_terlan_identifier(name: &str) -> String {
    let normalized = normalize_identifier_to_snake(name, true);
    if is_terlan_keyword(&normalized) {
        format!("{normalized}_")
    } else {
        normalized
    }
}

/// Normalizes an identifier-like string to lower snake case.
///
/// Inputs:
/// - `segment`: source identifier text.
/// - `require_lower_start`: whether to make invalid starts legal for Terlan
///   value/member identifiers.
///
/// Output:
/// - Lower-snake identifier text.
///
/// Transformation:
/// - Inserts separators for lower-to-upper, digit-to-upper, and
///   acronym-to-word boundaries, collapses duplicate separators, and optionally
///   prefixes names that do not start with lowercase ASCII.
fn normalize_identifier_to_snake(segment: &str, require_lower_start: bool) -> String {
    let chars = segment.chars().collect::<Vec<_>>();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        let prev = index.checked_sub(1).and_then(|prev| chars.get(prev));
        let next = chars.get(index + 1);
        if should_insert_separator(prev.copied(), *ch, next.copied()) {
            push_identifier_char(&mut out, '_');
        }
        push_identifier_char(&mut out, ch.to_ascii_lowercase());
    }
    while out.ends_with('_') {
        out.pop();
    }
    if require_lower_start {
        while out.starts_with('_') {
            out.remove(0);
        }
    }
    if require_lower_start
        && (out.is_empty() || !out.chars().next().is_some_and(|ch| ch.is_ascii_lowercase()))
    {
        out.insert_str(0, "value_");
    }
    out
}

/// Appends one normalized identifier character.
///
/// Inputs:
/// - `output`: identifier text being built.
/// - `ch`: normalized source character.
///
/// Output:
/// - Mutated identifier text.
///
/// Transformation:
/// - Preserves ASCII alphanumeric and underscore characters, converts other
///   characters to underscores, and collapses repeated underscores.
fn push_identifier_char(output: &mut String, ch: char) {
    let next = if ch.is_ascii_alphanumeric() || ch == '_' {
        ch
    } else {
        '_'
    };
    if next == '_' && output.ends_with('_') {
        return;
    }
    output.push(next);
}

/// Returns whether a snake-case separator belongs before `current`.
///
/// Inputs:
/// - `prev`: previous source character, if any.
/// - `current`: current source character.
/// - `next`: next source character, if any.
///
/// Output:
/// - `true` when a separator should be inserted.
///
/// Transformation:
/// - Handles both `getElement` and acronym boundaries such as `HTMLElement`.
fn should_insert_separator(prev: Option<char>, current: char, next: Option<char>) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    if !current.is_ascii_uppercase() {
        return false;
    }
    prev.is_ascii_lowercase()
        || prev.is_ascii_digit()
        || (prev.is_ascii_uppercase() && next.is_some_and(|next| next.is_ascii_lowercase()))
}

/// Returns whether a generated identifier collides with Terlan syntax.
///
/// Inputs:
/// - `name`: lowercase identifier candidate.
///
/// Output:
/// - `true` when the candidate is a reserved Terlan keyword.
/// - `false` otherwise.
///
/// Transformation:
/// - Centralizes keyword collision handling for generated external bindings.
fn is_terlan_keyword(name: &str) -> bool {
    matches!(
        name,
        "after"
            | "and"
            | "annotation"
            | "as"
            | "case"
            | "constructor"
            | "catch"
            | "css"
            | "div"
            | "extends"
            | "false"
            | "file"
            | "for"
            | "from"
            | "html"
            | "if"
            | "implements"
            | "impl"
            | "includes"
            | "import"
            | "let"
            | "machine"
            | "macro"
            | "markdown"
            | "module"
            | "mut"
            | "native"
            | "nominal"
            | "not"
            | "opaque"
            | "or"
            | "pub"
            | "quote"
            | "rem"
            | "static"
            | "struct"
            | "target"
            | "template"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "unquote"
            | "when"
            | "where"
            | "with"
    )
}
