//! Utilities for parsing `native core module` blocks.

/// Native function signature extracted from a `native core module` block.
///
/// Inputs:
/// - Textual native function declarations annotated with `#[native(...)]`.
///
/// Output:
/// - Function name, arity, parameter names/types, and return type text.
///
/// Transformation:
/// - Captures just enough signature metadata for native policy and interface
///   checks without parsing implementation bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFunctionSignature {
    pub name: String,
    pub arity: usize,
    pub params: Vec<(String, String)>,
    pub return_type: String,
}

/// Extracts the declared native module name.
///
/// Inputs:
/// - `source`: Terlan source text that may contain `native core module Name`.
///
/// Output:
/// - Native module name when present.
///
/// Transformation:
/// - Scans line-by-line for the native module header and reads the name before
///   the opening block or trailing whitespace.
pub fn extract_native_module_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed
            .strip_prefix("native core module ")
            .or_else(|| trimmed.strip_prefix("native core module\t"))?;
        let name_part = rest.split('{').next().unwrap_or("").trim();
        let name = name_part.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    })
}

/// Extracts the first native scheduler annotation from a native block.
///
/// Inputs:
/// - `source`: Terlan source text containing a native block.
///
/// Output:
/// - Scheduler string from `#[native(...)]` when present.
///
/// Transformation:
/// - Finds the native block, normalizes spacing, and reads the first native
///   annotation payload without interpreting scheduler semantics.
pub fn extract_native_scheduler(source: &str) -> Option<String> {
    let (_, block) = extract_native_block_and_source(source)?;
    let block = normalize_native_spacing(block);
    block.lines().map(|line| line.trim()).find_map(|trimmed| {
        trimmed.strip_prefix("#[native(").and_then(|rest| {
            rest.strip_suffix(")]")
                .map(|scheduler| scheduler.trim().to_string())
                .filter(|scheduler| !scheduler.is_empty())
        })
    })
}

/// Extracts annotated native function signatures from source text.
///
/// Inputs:
/// - `source`: Terlan source text containing a `native core module` block.
///
/// Output:
/// - Ordered native function signatures that follow `#[native(...)]`
///   annotations.
///
/// Transformation:
/// - Normalizes native-block spacing, scans annotated declarations through the
///   terminating top-level `.`, and parses each declaration into signature
///   metadata.
pub fn extract_native_function_signatures(source: &str) -> Vec<NativeFunctionSignature> {
    let Some((_, block)) = extract_native_block_and_source(source) else {
        return Vec::new();
    };
    let block = normalize_native_spacing(block);
    let bytes = block.as_bytes();
    let mut i = 0usize;
    let mut signatures = Vec::new();

    while let Some(attr_pos) = block[i..].find("#[native(") {
        let attr_pos = i + attr_pos;
        let Some(attr_end) = block[attr_pos..]
            .find(")]")
            .map(|offset| attr_pos + offset + 2)
        else {
            break;
        };
        let mut pos = attr_end;
        while pos < block.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= block.len() {
            break;
        }

        let signature_start = pos;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        while pos < block.len() {
            let ch = bytes[pos];
            match ch {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'.' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    let raw_signature = &block[signature_start..pos];
                    if let Some(sig) = parse_native_function_signature(raw_signature) {
                        signatures.push(sig);
                    }
                    break;
                }
                _ => {}
            }
            pos += 1;
        }
        i = pos + 1;
    }

    signatures
}

/// Extracts the body of the first native core module block.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - Start offset after the opening brace and block body text.
///
/// Transformation:
/// - Finds `native core module`, then balances braces to return only the block
///   contents.
fn extract_native_block_and_source(source: &str) -> Option<(usize, &str)> {
    let Some(native_start) = source.find("native core module ") else {
        return None;
    };
    let after_start = &source[native_start..];
    let open_index = after_start.find('{')?;

    let mut depth = 0isize;
    let mut close_index = None;
    let mut index = open_index;
    for ch in after_start[index..].chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close_index = Some(index);
                    break;
                }
            }
            _ => {}
        }
        index += ch.len_utf8();
    }

    let close_index = close_index?;
    Some((
        native_start + open_index + 1,
        &source[native_start + open_index + 1..native_start + close_index],
    ))
}

/// Normalizes lightweight spacing variants inside native declarations.
///
/// Inputs:
/// - `source`: raw native block body.
///
/// Output:
/// - String with known harmless spacing variants collapsed.
///
/// Transformation:
/// - Rewrites annotation, punctuation, and delimiter spacing so the simple
///   native signature scanner can remain deterministic.
fn normalize_native_spacing(source: &str) -> String {
    let mut normalized = source.to_string();
    normalized = normalized.replace("# [", "#[");
    normalized = normalized.replace(" ]", "]");
    normalized = normalized.replace("[ ", "[");
    normalized = normalized.replace("( ", "(");
    normalized = normalized.replace(" (", "(");
    normalized = normalized.replace(" )", ")");
    normalized = normalized.replace(":", ":");
    normalized = normalized.replace(" : ", ":");
    normalized = normalized.replace(" ,", ",");
    normalized = normalized.replace(", ", ",");
    normalized = normalized.replace(" .", ".");
    normalized
}

/// Parses one native function signature declaration.
///
/// Inputs:
/// - `line`: native function declaration text without the leading annotation.
///
/// Output:
/// - Parsed `NativeFunctionSignature`, or `None` when the declaration is not a
///   supported signature shape.
///
/// Transformation:
/// - Locates the parameter list, extracts the name, counts arity, parses
///   parameter annotations, and normalizes return type text.
fn parse_native_function_signature(line: &str) -> Option<NativeFunctionSignature> {
    let signature = line
        .trim()
        .trim_end_matches('.')
        .trim()
        .trim_end_matches('}')
        .trim();
    if !signature.contains('(') || !signature.contains(')') {
        return None;
    }

    let open = signature.find('(')?;
    let close = find_matching_paren(signature, open)?;
    if close < open {
        return None;
    }

    let name = parse_native_function_name(&signature[..open])?;
    let params_src = &signature[open + 1..close];
    let arity = native_signature_arity(params_src);

    let params = parse_native_function_params(params_src);
    let return_type = parse_native_return_type(&signature[close + 1..]);

    Some(NativeFunctionSignature {
        name,
        arity,
        params,
        return_type,
    })
}

/// Parses the function name before a native parameter list.
///
/// Inputs:
/// - `prefix`: text before the opening `(`.
///
/// Output:
/// - Function name when present.
///
/// Transformation:
/// - Trims generic/type-parameter suffixes and whitespace from the declaration
///   prefix.
fn parse_native_function_name(prefix: &str) -> Option<String> {
    let name = prefix
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == '[')
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Parses native function parameter declarations.
///
/// Inputs:
/// - `src`: comma-separated parameter list body.
///
/// Output:
/// - Ordered `(name, type_text)` parameter pairs.
///
/// Transformation:
/// - Splits at top-level commas, keeps parameters with `name: Type` shape, and
///   strips trailing declaration punctuation from type annotations.
fn parse_native_function_params(src: &str) -> Vec<(String, String)> {
    split_top_level(src, ',')
        .into_iter()
        .filter_map(|param| {
            let text = param.trim();
            if text.is_empty() {
                return None;
            }

            let colon = text.find(':')?;
            let (name, annotation) = text.split_at(colon);
            let mut annotation = annotation[1..].trim();
            if let Some((first, _)) = annotation.split_once('.') {
                annotation = first.trim();
            }
            Some((name.trim().to_string(), annotation.to_string()))
        })
        .collect()
}

/// Normalizes native function return type text.
///
/// Inputs:
/// - `raw`: text after the closing parameter-list parenthesis.
///
/// Output:
/// - Return type text without `:`, `.`, `}`, or surrounding whitespace.
///
/// Transformation:
/// - Performs syntax-level cleanup without validating type names.
fn parse_native_return_type(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(":")
        .trim()
        .trim_end_matches('.')
        .trim_end_matches('}')
        .trim()
        .to_string()
}

/// Splits text at a delimiter outside nested delimiters.
///
/// Inputs:
/// - `input`: source text to split.
/// - `delimiter`: delimiter character to split on.
///
/// Output:
/// - Ordered text segments.
///
/// Transformation:
/// - Tracks parentheses, brackets, and braces so nested generic or tuple-like
///   forms do not split the parameter list.
fn split_top_level(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut paren_depth = 0isize;
    let mut bracket_depth = 0isize;
    let mut brace_depth = 0isize;
    let mut last = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            c if c == delimiter && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                parts.push(input[last..idx].to_string());
                last = idx + c.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(input[last..].to_string());
    parts
}

/// Counts native function arity from a parameter-list body.
///
/// Inputs:
/// - `args`: text inside the parameter list.
///
/// Output:
/// - Number of top-level comma-separated parameters.
///
/// Transformation:
/// - Uses delimiter-depth tracking so nested type syntax does not affect arity.
fn native_signature_arity(args: &str) -> usize {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut paren_depth = 0isize;
    let mut bracket_depth = 0isize;
    let mut brace_depth = 0isize;
    let mut commas = 0usize;

    for ch in args.chars() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => commas += 1,
            _ => {}
        }
    }

    commas + 1
}

/// Finds the closing parenthesis matching an opening parenthesis.
///
/// Inputs:
/// - `input`: source text containing the parenthesis.
/// - `open_idx`: byte offset of the opening parenthesis.
///
/// Output:
/// - Byte offset of the matching closing parenthesis.
///
/// Transformation:
/// - Walks nested parentheses with a depth counter and returns the close that
///   balances the requested opening parenthesis.
pub(crate) fn find_matching_paren(input: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in input.char_indices().skip(open_idx) {
        match ch {
            '(' => depth += 1,
            ')' if depth == 1 => return Some(offset),
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[path = "native_test.rs"]
mod native_test;
