/// Verifies that TypeScript declarations read public signatures from
/// CoreIR metadata.
///
/// Inputs:
/// - A checked Terlan module containing public/private functions and a
///   public result type.
///
/// Output:
/// - Assertions over generated TypeScript declaration source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits declarations from
///   `CoreModule`, and checks type/function visibility plus CoreIR type
///   mapping.
#[test]
fn emit_core_module_to_typescript_declarations_uses_core_surface() {
    let source = "\
module js_core_declarations.

pub type Result[T, E] =
      {ok, T}
    | {error, E}.

pub add(A: Int, B: Int): Int ->
    A + B.

hidden(A: Int): Int ->
    A.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_declarations.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let declarations = declarations::emit_core_module_to_typescript_declarations(&artifacts.core);

    assert!(declarations.contains("export type Result<T, E>"));
    assert!(declarations.contains("export function add(A: number, B: number): number;"));
    assert!(!declarations.contains("hidden"));
}

/// Verifies fallback TypeScript type text atom mapping escapes string payloads.
///
/// Inputs:
/// - A lowercase atom-like type text containing a quote and backslash.
///
/// Output:
/// - TypeScript string-literal union member text with escaped payload.
///
/// Transformation:
/// - Exercises the fallback declaration mapper path that receives rendered
///   type-body text rather than structured CoreType atoms.
#[test]
fn typescript_declaration_type_text_escapes_lowercase_atom_literals() {
    let mapped = declarations::typer_type_to_typescript("it\"s\\ready");

    assert_eq!(mapped, r#""it\"s\\ready""#);
}
use super::*;
