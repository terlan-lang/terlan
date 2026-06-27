use super::SqlParameterBindingError;

/// Masks SQL comments, strings, and quoted identifiers with whitespace.
///
/// Inputs:
/// - `raw`: SQL source text.
///
/// Output:
/// - String with comments and quoted segments replaced by spaces/newlines.
///
/// Transformation:
/// - Preserves byte positions for keyword searching without allowing SQL words
///   inside comments and strings to affect projection parsing.
pub(super) fn mask_sql_literals_and_comments(raw: &str) -> String {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if current == '-' && next == Some('-') {
            index = mask_sql_line_comment(&chars, index, &mut output);
        } else if current == '/' && next == Some('*') {
            index = mask_sql_block_comment(&chars, index, &mut output);
        } else if current == '\'' || current == '"' {
            index = mask_sql_quoted_segment(&chars, index, current, &mut output);
        } else {
            output.push(current);
            index += 1;
        }
    }
    output
}

/// Finds a SQL word outside parentheses in masked SQL text.
///
/// Inputs:
/// - `masked`: SQL text after comments and quoted segments are masked.
/// - `word`: lowercase SQL word to find.
/// - `start`: byte offset to start scanning.
///
/// Output:
/// - Byte offset of the first top-level matching word.
///
/// Transformation:
/// - Scans character-by-character and tracks parentheses so nested subqueries
///   do not terminate the outer projection.
pub(super) fn find_top_level_sql_word(masked: &str, word: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in masked
        .char_indices()
        .skip_while(|(index, _)| *index < start)
    {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => {
                if masked
                    .get(index..)
                    .is_some_and(|rest| rest.len() >= word.len())
                    && find_sql_word(masked, word, index) == Some(index)
                {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Tokenizes SQL text while ignoring comments and quoted literal bodies.
///
/// Inputs:
/// - `raw`: SQL source text captured by the parser.
///
/// Output:
/// - Lowercase word-like and number-like SQL tokens.
///
/// Transformation:
/// - Drops comments and quoted segments, then emits alphanumeric/underscore
///   runs as lowercase tokens for lightweight shape inference.
pub(super) fn sql_words_without_literals_or_comments(raw: &str) -> Vec<String> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if current == '-' && next == Some('-') {
            flush_sql_word(&mut word, &mut words);
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }

        if current == '/' && next == Some('*') {
            flush_sql_word(&mut word, &mut words);
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = (index + 2).min(chars.len());
            continue;
        }

        if current == '\'' || current == '"' {
            flush_sql_word(&mut word, &mut words);
            index = skip_sql_quoted_segment(&chars, index, current);
            continue;
        }

        if current.is_ascii_alphanumeric() || current == '_' {
            word.push(current.to_ascii_lowercase());
        } else {
            flush_sql_word(&mut word, &mut words);
        }

        index += 1;
    }

    flush_sql_word(&mut word, &mut words);
    words
}

/// Copies one SQL line comment to output.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: index of the first `-` in a `--` comment opener.
/// - `output`: SQL output accumulator.
///
/// Output:
/// - Index at the newline or end of input after the copied comment.
///
/// Transformation:
/// - Preserves comment text exactly so binding rewrites do not disturb source
///   layout or comments.
pub(super) fn copy_sql_line_comment(chars: &[char], start: usize, output: &mut String) -> usize {
    let mut index = start;
    while index < chars.len() {
        let current = chars[index];
        output.push(current);
        index += 1;
        if current == '\n' {
            break;
        }
    }
    index
}

/// Copies one SQL block comment to output.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: index of the `/` in a `/*` comment opener.
/// - `output`: SQL output accumulator.
///
/// Output:
/// - Index immediately after the copied block comment, or end of input when
///   the comment is unterminated.
///
/// Transformation:
/// - Preserves block comment text exactly and prevents `${...}` text inside
///   comments from becoming runtime parameters.
pub(super) fn copy_sql_block_comment(chars: &[char], start: usize, output: &mut String) -> usize {
    let mut index = start;
    while index < chars.len() {
        let current = chars[index];
        output.push(current);
        if current == '*' && chars.get(index + 1) == Some(&'/') {
            output.push('/');
            return index + 2;
        }
        index += 1;
    }
    index
}

/// Copies one SQL quoted string or quoted identifier segment to output.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: index at the opening quote.
/// - `quote`: quote character.
/// - `output`: SQL output accumulator.
///
/// Output:
/// - Index immediately after the quoted segment, or end of input when the
///   quote is unterminated.
///
/// Transformation:
/// - Preserves quoted text exactly and handles doubled SQL quotes.
pub(super) fn copy_sql_quoted_segment(
    chars: &[char],
    start: usize,
    quote: char,
    output: &mut String,
) -> usize {
    output.push(chars[start]);
    let mut index = start + 1;
    while index < chars.len() {
        output.push(chars[index]);
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                output.push(quote);
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    index
}

/// Reads one Terlan expression source inside a SQL interpolation.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: index immediately after `${`.
///
/// Output:
/// - Interpolation source text and index after the closing brace, or an error.
///
/// Transformation:
/// - Tracks nested braces and quoted Terlan strings so the whole expression
///   becomes one SQL parameter island.
pub(super) fn read_sql_interpolation_source(
    chars: &[char],
    start: usize,
) -> Result<(String, usize), SqlParameterBindingError> {
    let mut index = start;
    let mut depth = 1usize;
    let mut quote = None;

    while index < chars.len() {
        let current = chars[index];
        if let Some(current_quote) = quote {
            if current == '\\' && current_quote == '"' && index + 1 < chars.len() {
                index += 2;
                continue;
            }
            if current == current_quote {
                quote = None;
            }
            index += 1;
            continue;
        }

        if current == '"' || current == '\'' {
            quote = Some(current);
            index += 1;
            continue;
        }

        match current {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((chars[start..index].iter().collect(), index + 1));
                }
            }
            _ => {}
        }

        index += 1;
    }

    Err(SqlParameterBindingError::UnterminatedInterpolation)
}

/// Finds a SQL word in masked SQL text.
///
/// Inputs:
/// - `masked`: SQL text after comments and quoted segments are masked.
/// - `word`: lowercase SQL word to find.
/// - `start`: byte offset to start searching.
///
/// Output:
/// - Byte offset of the first matching word boundary occurrence.
///
/// Transformation:
/// - Performs ASCII case-insensitive matching while requiring non-identifier
///   boundaries on both sides.
pub(super) fn find_sql_word(masked: &str, word: &str, start: usize) -> Option<usize> {
    let lower = masked.to_ascii_lowercase();
    let mut search_start = start;
    while search_start < lower.len() {
        let relative = lower.get(search_start..)?.find(word)?;
        let index = search_start + relative;
        let before = index
            .checked_sub(1)
            .and_then(|before| lower.as_bytes().get(before))
            .copied();
        let after = lower.as_bytes().get(index + word.len()).copied();
        if !is_sql_identifier_byte(before) && !is_sql_identifier_byte(after) {
            return Some(index);
        }
        search_start = index + word.len();
    }
    None
}

/// Returns whether an optional byte is an SQL identifier byte.
///
/// Inputs:
/// - `byte`: optional ASCII byte adjacent to a candidate SQL word.
///
/// Output:
/// - `true` when the byte can continue an SQL identifier.
///
/// Transformation:
/// - Treats absent boundaries as non-identifier boundaries.
fn is_sql_identifier_byte(byte: Option<u8>) -> bool {
    byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Masks a SQL line comment into the output string.
///
/// Inputs:
/// - `chars`: SQL characters.
/// - `start`: index at the first `-` of a `--` comment.
/// - `output`: masked SQL output accumulator.
///
/// Output:
/// - Index after the comment newline or end of input.
///
/// Transformation:
/// - Replaces comment content with spaces while preserving newlines.
fn mask_sql_line_comment(chars: &[char], start: usize, output: &mut String) -> usize {
    let mut index = start;
    while index < chars.len() {
        let current = chars[index];
        output.push(if current == '\n' { '\n' } else { ' ' });
        index += 1;
        if current == '\n' {
            break;
        }
    }
    index
}

/// Masks a SQL block comment into the output string.
///
/// Inputs:
/// - `chars`: SQL characters.
/// - `start`: index at the `/` of a `/*` comment.
/// - `output`: masked SQL output accumulator.
///
/// Output:
/// - Index after the comment terminator or end of input.
///
/// Transformation:
/// - Replaces comment content with spaces while preserving newlines.
fn mask_sql_block_comment(chars: &[char], start: usize, output: &mut String) -> usize {
    let mut index = start;
    while index < chars.len() {
        let current = chars[index];
        output.push(if current == '\n' { '\n' } else { ' ' });
        if current == '*' && chars.get(index + 1) == Some(&'/') {
            output.push(' ');
            return index + 2;
        }
        index += 1;
    }
    index
}

/// Masks a SQL quoted string or identifier into the output string.
///
/// Inputs:
/// - `chars`: SQL characters.
/// - `start`: index at the opening quote.
/// - `quote`: quote character.
/// - `output`: masked SQL output accumulator.
///
/// Output:
/// - Index after the quoted segment or end of input.
///
/// Transformation:
/// - Replaces quoted content with spaces while preserving newlines and doubled
///   SQL quote escapes.
fn mask_sql_quoted_segment(
    chars: &[char],
    start: usize,
    quote: char,
    output: &mut String,
) -> usize {
    output.push(' ');
    let mut index = start + 1;
    while index < chars.len() {
        let current = chars[index];
        output.push(if current == '\n' { '\n' } else { ' ' });
        if current == quote {
            if chars.get(index + 1) == Some(&quote) {
                output.push(' ');
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    index
}

/// Skips one SQL quoted string or quoted identifier segment.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: character index at the opening quote.
/// - `quote`: opening quote character.
///
/// Output:
/// - Index after the quoted segment, or the end of input when unterminated.
///
/// Transformation:
/// - Handles doubled SQL quotes as escaped quote characters.
fn skip_sql_quoted_segment(chars: &[char], start: usize, quote: char) -> usize {
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    chars.len()
}

/// Flushes the current SQL token accumulator into an output vector.
///
/// Inputs:
/// - `word`: mutable token accumulator.
/// - `words`: output token list.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Moves the accumulated token into `words` when non-empty, then clears the
///   accumulator for the next token.
fn flush_sql_word(word: &mut String, words: &mut Vec<String>) {
    if !word.is_empty() {
        words.push(std::mem::take(word));
    }
}
