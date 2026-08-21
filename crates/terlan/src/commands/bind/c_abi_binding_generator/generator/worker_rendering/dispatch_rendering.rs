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
    dispatch_chunk_count: usize,
    inline_match_arms: &str,
) -> String {
    if dispatch_chunk_count == 0 {
        return format!(
            "        match request.operation.as_str() {{\n{inline_match_arms}\n            _ => protocol_error(\"unknown_operation\", &request.operation),\n        }}"
        );
    }
    let mut calls = (0..dispatch_chunk_count)
        .map(|index| {
            format!(
                "        if native_boundary_dispatch_{index}::accepts_chunk_{index}(request.operation.as_str()) {{\n            return self.execute_chunk_{index}(request);\n        }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    calls.push_str("\n        protocol_error(\"unknown_operation\", &request.operation)");
    calls
}
