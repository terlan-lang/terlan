use super::Variance;

/// Normalizes generic type parameter text.
pub(super) fn normalize_type_param_name(param: &str) -> String {
    let trimmed = param.trim().trim_start_matches('-').trim_start_matches('+');
    if let Some(rest) = trimmed.strip_prefix("const ") {
        return rest
            .split_once(':')
            .map(|(name, _)| name.trim())
            .unwrap_or(rest.trim())
            .to_string();
    }
    let trimmed = trimmed
        .split_once("=>")
        .map(|(name, _)| name.trim())
        .unwrap_or(trimmed);
    if let Some(open) = trimmed.find('[') {
        trimmed[..open].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extracts variance from one generic type parameter declaration.
pub(super) fn type_param_variance(param: &str) -> Variance {
    match param.trim().chars().next() {
        Some('+') => Variance::Covariant,
        Some('-') => Variance::Contravariant,
        _ => Variance::Invariant,
    }
}

/// Extracts variance metadata in declaration order.
pub(super) fn type_param_variances(params: &[String]) -> Vec<Variance> {
    params
        .iter()
        .map(|param| type_param_variance(param))
        .collect()
}
