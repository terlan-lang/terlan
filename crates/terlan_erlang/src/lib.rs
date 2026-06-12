pub mod emit;
pub mod pretty;

pub use emit::emit_html_runtime_to_erlang;
pub use emit::try_emit_core_module_to_erlang_with_syntax_bridge;
pub use emit::try_emit_syntax_module_output_to_erlang;
pub use emit::try_emit_syntax_module_output_to_erlang_with_interfaces;
pub use emit::try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown;
pub use emit::try_emit_syntax_struct_headers_to_hrl;
