//! Utilities for parsing `native core module` blocks.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFunctionSignature {
    pub name: String,
    pub arity: usize,
    pub params: Vec<(String, String)>,
    pub return_type: String,
}

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

pub fn extract_native_scheduler(source: &str) -> Option<String> {
    let (_, block) = extract_native_block_and_source(source)?;
    let block = normalize_native_spacing(block);
    block.lines().map(|line| line.trim()).find_map(|trimmed| {
        trimmed.strip_prefix("#[nif(").and_then(|rest| {
            rest.strip_suffix(")]")
                .map(|scheduler| scheduler.trim().to_string())
                .filter(|scheduler| !scheduler.is_empty())
        })
    })
}

pub fn extract_native_function_signatures(source: &str) -> Vec<NativeFunctionSignature> {
    let Some((_, block)) = extract_native_block_and_source(source) else {
        return Vec::new();
    };
    let block = normalize_native_spacing(block);
    let bytes = block.as_bytes();
    let mut i = 0usize;
    let mut signatures = Vec::new();

    while let Some(attr_pos) = block[i..].find("#[nif(") {
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

fn parse_native_return_type(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(":")
        .trim()
        .trim_end_matches('.')
        .trim_end_matches('}')
        .trim()
        .to_string()
}

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

fn find_matching_paren(input: &str, open_idx: usize) -> Option<usize> {
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
mod tests {
    use super::*;

    #[test]
    fn parses_native_block_signatures_and_types() {
        let source = "native core module VecNative {\n    #[nif(normal)]\n    empty[T](): Vec[T].\n\n    #[nif(normal)]\n    push[T](V: Vec[T], Item: T): Vec[T].\n}";

        assert_eq!(
            extract_native_module_name(source).as_deref(),
            Some("VecNative")
        );
        assert_eq!(extract_native_scheduler(source).as_deref(), Some("normal"));

        let signatures = extract_native_function_signatures(source);
        assert_eq!(signatures.len(), 2);

        assert_eq!(signatures[0].name, "empty");
        assert_eq!(signatures[0].arity, 0);
        assert_eq!(signatures[0].params.len(), 0);
        assert_eq!(signatures[0].return_type, "Vec[T]");

        assert_eq!(signatures[1].name, "push");
        assert_eq!(signatures[1].arity, 2);
        assert_eq!(
            signatures[1].params[0],
            ("V".to_string(), "Vec[T]".to_string())
        );
        assert_eq!(
            signatures[1].params[1],
            ("Item".to_string(), "T".to_string())
        );
        assert_eq!(signatures[1].return_type, "Vec[T]");
    }

    #[test]
    fn parses_native_signatures_from_block_without_newlines() {
        let source = "native core module VecNative { #[nif(normal)] empty[T](): Vec[T]. #[nif(normal)] push[T](V: Vec[T], Item: T): Vec[T]. }";

        let signatures = extract_native_function_signatures(source);
        assert_eq!(signatures.len(), 2);
        assert_eq!(signatures[0].name, "empty");
        assert_eq!(signatures[1].name, "push");
        assert_eq!(signatures[1].arity, 2);
        assert_eq!(signatures[1].return_type, "Vec[T]");
    }
}
