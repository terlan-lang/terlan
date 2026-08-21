/// Parses a continued Make list variable while preserving declaration order.
pub(super) fn parse_make_list_variable_values(makefile: &str, variable: &str) -> Vec<String> {
    let prefix = format!("{variable} :=");
    let mut values = Vec::new();
    let mut collecting = false;
    for line in makefile.lines() {
        let trimmed = line.trim();
        if !collecting {
            let Some(rest) = trimmed.strip_prefix(&prefix) else {
                continue;
            };
            collecting = true;
            values.extend(
                rest.trim_end_matches('\\')
                    .split_whitespace()
                    .map(str::to_owned),
            );
            if !trimmed.ends_with('\\') {
                break;
            }
            continue;
        }
        values.extend(
            trimmed
                .trim_end_matches('\\')
                .split_whitespace()
                .map(str::to_owned),
        );
        if !trimmed.ends_with('\\') {
            break;
        }
    }
    values
}

#[cfg(test)]
#[path = "makefile_list_test.rs"]
mod tests;
