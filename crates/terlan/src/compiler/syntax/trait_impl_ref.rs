/// Restores implication binders into their semantic trait argument slots.
pub(crate) fn render_trait_impl_ref(trait_ref: &str, generic_params: &[String]) -> String {
    if generic_params.is_empty() {
        return trait_ref.to_string();
    }

    let Some(open) = trait_ref.find('[') else {
        return trait_ref.to_string();
    };
    let Some(inner) = trait_ref
        .strip_suffix(']')
        .map(|trait_ref| &trait_ref[open + 1..])
    else {
        return trait_ref.to_string();
    };
    let args = split_top_level_commas(inner)
        .into_iter()
        .map(|arg| {
            generic_params
                .iter()
                .find(|param| implication_subject(param).is_some_and(|subject| subject == arg))
                .cloned()
                .unwrap_or(arg)
        })
        .collect::<Vec<_>>();

    format!("{}[{}]", &trait_ref[..open], args.join(", "))
}

fn implication_subject(param: &str) -> Option<String> {
    param
        .split_once("=>")
        .map(|(subject, _)| subject.trim().to_string())
}

fn split_top_level_commas(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;

    for (index, ch) in text.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                parts.push(text[start..index].trim().to_string());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim().to_string());
    parts
}
