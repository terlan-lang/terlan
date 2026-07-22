use super::*;

pub(super) fn string_capture_name(text: &str) -> &str {
    text.split_once(':').map_or(text, |(name, _)| name).trim()
}

pub(super) fn rewrite_string_pattern_text(
    text: &str,
    captures: &[SyntaxPatternOutput],
    shape_name: &str,
) -> EbnfCompileResult<String> {
    let mut output = String::with_capacity(text.len());
    let mut remaining = text;
    let mut captures = captures.iter();
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let slot = &remaining[start + 2..];
        let end = slot.find('}').ok_or_else(|| {
            EbnfCompileError::Serialize(format!(
                "shape `{shape_name}` has malformed canonical string-pattern text"
            ))
        })?;
        let capture = captures.next().ok_or_else(|| {
            EbnfCompileError::Serialize(format!(
                "shape `{shape_name}` has inconsistent string-pattern capture metadata"
            ))
        })?;
        let capture_text = capture.text.as_deref().ok_or_else(|| {
            EbnfCompileError::Serialize(format!(
                "shape `{shape_name}` has a string capture without binding metadata"
            ))
        })?;
        output.push_str("${");
        output.push_str(capture_text);
        output.push('}');
        remaining = &slot[end + 1..];
    }
    if captures.next().is_some() {
        return Err(EbnfCompileError::Serialize(format!(
            "shape `{shape_name}` has inconsistent string-pattern capture metadata"
        )));
    }
    output.push_str(remaining);
    Ok(output)
}
