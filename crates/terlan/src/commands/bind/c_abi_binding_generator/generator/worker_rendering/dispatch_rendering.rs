use super::*;

pub(super) fn render_dispatch_modules(chunk_count: usize) -> String {
    let mut modules = (0..chunk_count)
        .map(|index| {
            format!(
                "#[path = \"native_boundary_helper/dispatch_{index}.rs\"]\nmod native_boundary_dispatch_{index};"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !modules.is_empty() {
        modules.push_str("\n\n");
    }
    modules
}

pub(super) fn render_dispatch_calls(
    functions: &[&CAbiBindingFunction],
    dispatch_chunks: &[String],
    inline_match_arms: &str,
) -> String {
    if dispatch_chunks.is_empty() {
        return format!(
            "        match request.operation.as_str() {{\n{inline_match_arms}\n            _ => protocol_error(\"unknown_operation\", &request.operation),\n        }}"
        );
    }
    let mut calls = functions
        .chunks(32)
        .enumerate()
        .map(|(index, chunk_functions)| {
            let operations = chunk_functions
                .iter()
                .map(|function| format!("{:?}", function.operation))
                .collect::<Vec<_>>()
                .join(" | ");
            format!(
                "        if matches!(request.operation.as_str(), {operations}) {{\n            return self.execute_chunk_{index}(request);\n        }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    calls.push_str("\n        protocol_error(\"unknown_operation\", &request.operation)");
    calls
}
