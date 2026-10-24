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

#[test]
fn imported_generic_constructors_emit_relative_type_imports() {
    let mut artifacts = compile_syntax_module_through_phases_with_profile(
        "View.terl",
        "module app.consumer.View.\npub struct View { value: Int, }.\n",
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .unwrap();
    let crate::terlan_typeck::CoreType::Struct { fields, .. } =
        artifacts.core.types[0].core_body.as_mut().unwrap()
    else {
        panic!("record");
    };
    fields[0].ty = crate::terlan_typeck::CoreType::Apply {
        constructor: "app.host.Identity.Identity".into(),
        args: vec![crate::terlan_typeck::CoreType::Int],
    };
    let declarations = declarations::emit_core_module_to_typescript_declarations(&artifacts.core);
    assert!(
        declarations
            .contains("import type { Identity as $imported0 } from \"../host/Identity.js\";"),
        "{declarations}"
    );
    assert!(
        declarations.contains("value: $imported0<number>"),
        "{declarations}"
    );
}

#[test]
fn imported_nominal_types_emit_relative_type_imports() {
    let mut artifacts = compile_syntax_module_through_phases_with_profile(
        "View.terl",
        "module app.consumer.View.\npub struct View { coordinates: List[Int], }.\n",
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile public record");
    let crate::terlan_typeck::CoreType::Struct { fields, .. } =
        artifacts.core.types[0].core_body.as_mut().unwrap()
    else {
        panic!("record type");
    };
    fields[0].ty = crate::terlan_typeck::CoreType::List(Box::new(
        crate::terlan_typeck::CoreType::Named("app.geometry.Coordinates.Coordinate".into()),
    ));
    let declarations = declarations::emit_core_module_to_typescript_declarations(&artifacts.core);
    assert!(declarations
        .contains("import type { Coordinate as $imported0 } from \"../geometry/Coordinates.js\";"));
    assert!(declarations.contains("coordinates: Array<$imported0>"));
    assert!(!declarations.contains("\"app.geometry.Coordinates.Coordinate\""));
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

#[test]
fn javascript_scalar_wrappers_emit_native_types_without_runtime_imports() {
    let source = "module js_scalar_declarations.\nimport type std.js.Number.{JsNumber}.\nimport type std.js.String.{JsString}.\npub identity(text: JsString, count: JsNumber): JsString -> text.\n";
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "js_scalar_declarations.terl",
        source,
        crate::DiagnosticFormat::default(),
        None,
        crate::validation::native_policy::NativePolicy::default(),
        crate::validation::target_profile::TargetProfile::JsShared,
    )
    .expect("check native scalar signature");
    let declarations =
        super::super::declarations::emit_core_module_to_typescript_declarations(&artifacts.core);
    assert!(
        declarations.contains("identity(text: string, count: number): string"),
        "{declarations}"
    );
    assert!(!declarations.contains("import type"), "{declarations}");
}

#[test]
fn native_options_emit_tagged_runtime_types_without_missing_imports() {
    let source = "module js_option_declarations.\nimport std.core.Option.{Option, Some, None}.\npub type Values = Option[Int].\npub relay(value: Option[Int]): Option[Int] -> value.\n";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_option_declarations.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("check native option signature");
    let output = declarations::emit_core_module_to_typescript_declarations(&artifacts.core);
    assert!(
        output.contains("export type Values = \"none\" | [\"some\", number]"),
        "{output}"
    );
    assert!(
        output
            .contains("relay(value: \"none\" | [\"some\", number]): \"none\" | [\"some\", number]"),
        "{output}"
    );
    assert!(!output.contains("import type"), "{output}");
}
