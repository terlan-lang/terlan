use super::super::*;

/// Verifies implication-constrained struct parameters survive the parser
/// handoff used by typechecking, documentation, and editor tooling.
#[test]
fn syntax_output_preserves_structural_generic_struct_implication() {
    let output = parse_module_as_syntax_output(
        r#"
module generic_struct_output.

pub struct Page[T => {title: String}] {
    model: T
}.
"#,
    )
    .expect("generic struct syntax output");

    match &output.declarations[0].payload {
        SyntaxDeclarationPayload::Struct {
            name,
            generic_params,
            fields,
            ..
        } => {
            assert_eq!(name, "Page");
            assert_eq!(generic_params, &vec!["T => {title: String}".to_string()]);
            assert_eq!(fields[0].annotation.text, "T");
        }
        other => panic!("unexpected declaration payload: {other:?}"),
    }
}

#[test]
fn module_syntax_output_wraps_ebnf_contract_and_metadata() {
    let output = parse_module_as_syntax_output(
        r#"
            module demo.

            import lib.Mod.
            type Item = Int.
            pub add(X: Int): Int -> X + 1.
            "#,
    )
    .expect("syntax output");

    assert_eq!(output.schema, SYNTAX_MODULE_OUTPUT_SCHEMA);
    assert_eq!(output.source_kind, SyntaxSourceKind::Module);
    assert_eq!(output.module_name, "demo");
    assert_eq!(output.contract.entry_rule.as_deref(), Some("Program"));
    assert_eq!(output.declarations.len(), 3);
    assert_eq!(output.declarations[0].class, "ImportDecl");
    assert_eq!(output.declarations[1].class, "TypeDecl");
    assert_eq!(output.declarations[2].class, "FunctionDecl");
    match &output.declarations[0].payload {
        SyntaxDeclarationPayload::Import {
            import_kind,
            module_name,
            ..
        } => {
            assert_eq!(*import_kind, SyntaxImportKind::Module);
            assert_eq!(module_name, "lib");
        }
        other => panic!("unexpected import payload: {other:?}"),
    }
    match &output.declarations[1].payload {
        SyntaxDeclarationPayload::Type {
            name,
            is_public,
            is_opaque,
            variants,
            ..
        } => {
            assert_eq!(name, "Item");
            assert!(!is_public);
            assert!(!is_opaque);
            assert_eq!(variants.len(), 1);
            assert_eq!(variants[0].text, "Int");
        }
        other => panic!("unexpected type payload: {other:?}"),
    }
    match &output.declarations[2].payload {
        SyntaxDeclarationPayload::Function {
            name,
            params,
            return_type,
            is_public,
            is_macro,
            clauses,
            ..
        } => {
            assert_eq!(name, "add");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "X");
            assert_eq!(params[0].annotation.text, "Int");
            assert_eq!(return_type.text, "Int");
            assert!(*is_public);
            assert!(!is_macro);
            assert_eq!(clauses.len(), 1);
            assert_eq!(clauses[0].patterns.len(), 1);
            assert_eq!(clauses[0].patterns[0].kind, SyntaxPatternKind::Var);
            assert_eq!(clauses[0].patterns[0].text.as_deref(), Some("X"));
            assert_eq!(clauses[0].body.kind, SyntaxExprKind::BinaryOp);
            assert_eq!(clauses[0].body.operator.as_deref(), Some("+"));
            assert_eq!(clauses[0].body.children.len(), 2);
            assert_eq!(clauses[0].body.children[0].text.as_deref(), Some("X"));
            assert_eq!(clauses[0].body.children[1].text.as_deref(), Some("1"));
            assert!(!clauses[0].has_guard);
            assert!(clauses[0].guard.is_none());
        }
        other => panic!("unexpected function payload: {other:?}"),
    }
    assert!(output.syntax_contract.fingerprint.starts_with("fnv1a64:"));

    let raw = serde_json::to_string(&output).expect("serialize syntax output");
    let decoded =
        serde_json::from_str::<SyntaxModuleOutput>(&raw).expect("deserialize syntax output");
    assert_eq!(decoded, output);
}

/// Verifies function parameter defaults are preserved in syntax output.
///
/// Inputs:
/// - A module with a function whose trailing parameter has an integer
///   default.
///
/// Output:
/// - Syntax output marks the parameter as defaulted and carries the lowered
///   default expression plus source-like default text.
///
/// Transformation:
/// - Projects parser-level default-parameter metadata into the formal
///   syntax-output contract for later typechecking and call lowering.
#[test]
fn syntax_output_preserves_function_parameter_defaults() {
    let output = parse_module_as_syntax_output(
        r#"
            module function_defaults.

            pub add(X: Int, Step: Int = 1): Int -> X + Step.
            "#,
    )
    .expect("syntax output");

    let SyntaxDeclarationPayload::Function { params, .. } = &output.declarations[0].payload else {
        panic!("expected function payload");
    };
    assert_eq!(params.len(), 2);
    assert!(!params[0].has_default);
    assert!(params[1].has_default);
    let default = params[1].default.as_ref().expect("function param default");
    assert_eq!(default.kind, SyntaxExprKind::Int);
    assert_eq!(default.text.as_deref(), Some("1"));
    assert_eq!(params[1].default_text.as_deref(), Some("1"));
}

/// Verifies declaration annotations are preserved in syntax output.
///
/// Inputs:
/// - A module with one path-only annotation and one metadata-block
///   annotation before declarations.
///
/// Output:
/// - Assertions over `SyntaxDeclarationOutput.annotations`.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and confirms
///   parser annotation metadata is serialized beside the routed
///   declarations.

/// Verifies declaration annotations are preserved in syntax output.
///
/// Inputs:
/// - A module with one path-only annotation and one metadata-block
///   annotation before declarations.
///
/// Output:
/// - Assertions over `SyntaxDeclarationOutput.annotations`.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and confirms
///   parser annotation metadata is serialized beside the routed
///   declarations.
#[test]
fn syntax_output_preserves_declaration_annotations() {
    let output = parse_module_as_syntax_output(
        r#"
            module annotation_output.

            @compiler.inline
            type Tagged = Atom["tagged"].

            @target.vm {
              application: true
            }
            run(): Int -> 1.
            "#,
    )
    .expect("annotation syntax output");

    assert_eq!(output.declarations.len(), 2);
    let type_annotations = &output.declarations[0].annotations;
    assert_eq!(type_annotations.len(), 1);
    assert_eq!(type_annotations[0].path, vec!["compiler", "inline"]);
    assert!(type_annotations[0].args.is_none());
    assert!(type_annotations[0].entries.is_empty());
    assert!(type_annotations[0].values.is_empty());

    let function_annotations = &output.declarations[1].annotations;
    assert_eq!(function_annotations.len(), 1);
    assert_eq!(function_annotations[0].path, vec!["target", "vm"]);
    let args = function_annotations[0]
        .args
        .as_deref()
        .expect("annotation args");
    assert!(args.starts_with('{'));
    assert!(args.ends_with('}'));
    assert!(args.contains("application"));
    assert!(args.contains("true"));
    assert_eq!(function_annotations[0].entries.len(), 1);
    assert_eq!(function_annotations[0].entries[0].key, vec!["application"]);
    assert_eq!(
        function_annotations[0].entries[0].value,
        SyntaxAnnotationValueOutput::Bool { value: true }
    );
    assert!(function_annotations[0].values.is_empty());
}

/// Verifies marker intrinsic annotations do not require metadata.
///
/// Inputs:
/// - A declaration annotated with marker-only intrinsic metadata.
///
/// Output:
/// - Assertions over annotation path, empty args, entries, and values.
///
/// Transformation:
/// - Proves source declarations can mark compiler-owned lowering without
///   repeating an internal intrinsic key in source metadata.

/// Verifies marker intrinsic annotations do not require metadata.
///
/// Inputs:
/// - A declaration annotated with marker-only intrinsic metadata.
///
/// Output:
/// - Assertions over annotation path, empty args, entries, and values.
///
/// Transformation:
/// - Proves source declarations can mark compiler-owned lowering without
///   repeating an internal intrinsic key in source metadata.
#[test]
fn syntax_output_preserves_marker_intrinsic_annotations() {
    let output = parse_module_as_syntax_output(
        r#"
            module annotation_value_output.

            @compiler.intrinsic
            to_string(value: Int): String -> "1".
            "#,
    )
    .expect("marker annotation syntax output");

    let annotations = &output.declarations[0].annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].path, vec!["compiler", "intrinsic"]);
    assert!(annotations[0].args.is_none());
    assert!(annotations[0].entries.is_empty());
    assert!(annotations[0].values.is_empty());
}

/// Verifies `@pure` is accepted on ordinary functions.
///
/// Inputs:
/// - A function declaration annotated with marker-only `@pure`.
///
/// Output:
/// - Syntax output preserving the `pure` annotation path.
///
/// Transformation:
/// - Exercises the built-in purity metadata marker before semantic effect
///   validation consumes it.
#[test]
fn syntax_output_accepts_pure_annotation_on_function() {
    let output = parse_module_as_syntax_output(
        r#"
            module pure_function_annotation.

            @pure
            normalize(value: Int): Int -> value.
            "#,
    )
    .expect("pure function annotation syntax output");

    let annotations = &output.declarations[0].annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].path, vec!["pure"]);
    assert!(annotations[0].entries.is_empty());
    assert!(annotations[0].values.is_empty());
}

/// Verifies `@pure` is accepted on receiver methods.
///
/// Inputs:
/// - An opaque type plus a receiver method annotated with marker-only
///   `@pure`.
///
/// Output:
/// - Syntax output preserving the method annotation path.
///
/// Transformation:
/// - Keeps pure receiver helpers available for later guard/template
///   validation without adding separate method metadata syntax.
#[test]
fn syntax_output_accepts_pure_annotation_on_method() {
    let output = parse_module_as_syntax_output(
        r#"
            module pure_method_annotation.

            opaque type Box[T].

            @pure
            (box: Box[Int]) value(): Int -> 1.
            "#,
    )
    .expect("pure method annotation syntax output");

    let method = output
        .declarations
        .iter()
        .find(|declaration| declaration.class == "MethodDecl")
        .expect("method declaration");
    assert_eq!(method.annotations[0].path, vec!["pure"]);
}

/// Verifies `@pure` is function/method-only metadata.
///
/// Inputs:
/// - A type declaration annotated with `@pure`.
///
/// Output:
/// - Stable syntax-output diagnostic.
///
/// Transformation:
/// - Prevents purity metadata from being attached to declarations whose
///   bodies cannot be effect-checked.
#[test]
fn syntax_output_rejects_pure_annotation_on_non_function() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_pure_annotation.

            @pure
            type Value = Int.
            "#,
    )
    .expect_err("@pure should reject non-function declarations");

    let message = format!("{error:?}");
    assert!(
        message.contains("@pure cannot annotate TypeDecl"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies `@pure` stays marker-only.
///
/// Inputs:
/// - A function declaration annotated with keyed `@pure` metadata.
///
/// Output:
/// - Stable syntax-output diagnostic rejecting metadata.
///
/// Transformation:
/// - Keeps purity semantics tied to validated function bodies rather than
///   source-level trusted options.
#[test]
fn syntax_output_rejects_pure_annotation_metadata() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_pure_metadata.

            @pure { trusted: true }
            normalize(value: Int): Int -> value.
            "#,
    )
    .expect_err("@pure should reject metadata");

    let message = format!("{error:?}");
    assert!(
        message.contains("@pure does not accept metadata"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies `@test` is function-only syntax metadata.
///
/// Inputs:
/// - A type declaration annotated with `@test`.
///
/// Output:
/// - Test passes when syntax output rejects the annotation before semantic
///   lowering.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and checks the
///   built-in annotation schema diagnostic.

/// Verifies `@test` is function-only syntax metadata.
///
/// Inputs:
/// - A type declaration annotated with `@test`.
///
/// Output:
/// - Test passes when syntax output rejects the annotation before semantic
///   lowering.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and checks the
///   built-in annotation schema diagnostic.
#[test]
fn syntax_output_rejects_test_annotation_on_non_function() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_test_annotation.

            @test
            type Value = Int.
            "#,
    )
    .expect_err("@test should reject non-function declarations");

    let message = format!("{error:?}");
    assert!(
        message.contains("@test cannot annotate TypeDecl"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn syntax_output_rejects_benchmark_annotation_on_non_function() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_benchmark_annotation.

            @benchmark
            type Value = Int.
            "#,
    )
    .expect_err("@benchmark should reject non-function declarations");

    let message = format!("{error:?}");
    assert!(
        message.contains("@benchmark cannot annotate TypeDecl"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn syntax_output_rejects_benchmark_annotation_metadata() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_benchmark_metadata.

            @benchmark { samples: 10 }
            measure(): Bool -> true.
            "#,
    )
    .expect_err("@benchmark should remain marker-only");

    let message = format!("{error:?}");
    assert!(
        message.contains("@benchmark does not accept metadata"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies `@test` stays marker-only.
///
/// Inputs:
/// - A function declaration annotated with keyed `@test` metadata.
///
/// Output:
/// - Test passes when syntax output rejects metadata on `@test`.
///
/// Transformation:
/// - Exercises the built-in marker annotation schema after parser metadata
///   has been converted into typed syntax output.

/// Verifies `@test` stays marker-only.
///
/// Inputs:
/// - A function declaration annotated with keyed `@test` metadata.
///
/// Output:
/// - Test passes when syntax output rejects metadata on `@test`.
///
/// Transformation:
/// - Exercises the built-in marker annotation schema after parser metadata
///   has been converted into typed syntax output.
#[test]
fn syntax_output_rejects_test_annotation_metadata() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_test_metadata.

            @test { name: "case" }
            passes(): Bool -> true.
            "#,
    )
    .expect_err("@test should reject metadata");

    let message = format!("{error:?}");
    assert!(
        message.contains("@test does not accept metadata"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies target-owned annotation schemas reject unknown keys.
///
/// Inputs:
/// - A function declaration annotated with an unknown `@target.vm` key.
///
/// Output:
/// - Test passes when syntax output reports a key-level schema error.
///
/// Transformation:
/// - Checks that target metadata is typechecked by syntax output instead of
///   being deferred to backend-specific string handling.

/// Verifies target-owned annotation schemas reject unknown keys.
///
/// Inputs:
/// - A function declaration annotated with an unknown `@target.vm` key.
///
/// Output:
/// - Test passes when syntax output reports a key-level schema error.
///
/// Transformation:
/// - Checks that target metadata is typechecked by syntax output instead of
///   being deferred to backend-specific string handling.
#[test]
fn syntax_output_rejects_unknown_target_vm_key() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_target_vm_key.

            @target.vm { unknown: true }
            run(): Int -> 1.
            "#,
    )
    .expect_err("@target.vm should reject unknown keys");

    let message = format!("{error:?}");
    assert!(
        message.contains("@target.vm has unknown key `unknown`"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies JS target annotations accept generated-binding metadata.
///
/// Inputs:
/// - A function declaration annotated with JS source-name, module,
///   namespace, global, and profile metadata.
///
/// Output:
/// - Test passes when syntax output preserves the typed annotation entries.
///
/// Transformation:
/// - Exercises the compiler-known JS annotation schema that generated
///   `std.js` bindings use before CoreIR or backend emission.
#[test]
fn syntax_output_accepts_target_js_annotation_metadata() {
    let output = parse_module_as_syntax_output(
        r#"
            module js_target_annotation_output.

            @target.js {
              name: "querySelector";
              source_module: "dom";
              namespace: "web.dom";
              global: true;
              profile: "browser"
            }
            pub query_selector(selector: String): String -> selector.
            "#,
    )
    .expect("target js annotation syntax output");

    let annotations = &output.declarations[0].annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].path, vec!["target", "js"]);
    assert_eq!(annotations[0].entries.len(), 5);
    assert!(annotations[0].values.is_empty());
    assert_eq!(annotations[0].entries[0].key, vec!["name"]);
    assert_eq!(
        annotations[0].entries[0].value,
        SyntaxAnnotationValueOutput::String {
            text: "\"querySelector\"".to_string()
        }
    );
    assert_eq!(annotations[0].entries[3].key, vec!["global"]);
    assert_eq!(
        annotations[0].entries[3].value,
        SyntaxAnnotationValueOutput::Bool { value: true }
    );
}

/// Verifies JS target annotations reject unknown keys.
///
/// Inputs:
/// - A function declaration annotated with an undeclared JS target key.
///
/// Output:
/// - Test passes when syntax output reports a stable schema diagnostic.
///
/// Transformation:
/// - Prevents generated JS metadata from silently accepting misspelled keys
///   that backend emission would otherwise interpret inconsistently.
#[test]
fn syntax_output_rejects_unknown_target_js_key() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_target_js_key.

            @target.js { source: "querySelector" }
            pub query_selector(selector: String): String -> selector.
            "#,
    )
    .expect_err("@target.js should reject unknown keys");

    let message = format!("{error:?}");
    assert!(
        message.contains("@target.js has unknown key `source`"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies JS target annotations reject wrong value types.
///
/// Inputs:
/// - A function declaration whose JS `name` metadata is a boolean.
///
/// Output:
/// - Test passes when syntax output reports the expected value type.
///
/// Transformation:
/// - Keeps generated binding metadata typed instead of backend-owned
///   stringly configuration.
#[test]
fn syntax_output_rejects_target_js_wrong_value_type() {
    let error = parse_module_as_syntax_output(
        r#"
            module bad_target_js_type.

            @target.js { name: true }
            pub query_selector(selector: String): String -> selector.
            "#,
    )
    .expect_err("@target.js should reject wrong value types");

    let message = format!("{error:?}");
    assert!(
        message.contains("annotation key `name` expects name or String"),
        "unexpected diagnostic: {message}"
    );
}

/// Verifies native annotations accept the current typed metadata shape.
///
/// Inputs:
/// - An opaque native type annotated with adapter, runtime, and worker
///   metadata.
///
/// Output:
/// - Test passes when syntax output preserves all typed `@native` entries.
///
/// Transformation:
/// - Parses a future native adapter contract shape through the formal syntax
///   output boundary without lowering it to a backend.

/// Verifies native annotations accept the current typed metadata shape.
///
/// Inputs:
/// - An opaque native type annotated with adapter, runtime, and worker
///   metadata.
///
/// Output:
/// - Test passes when syntax output preserves all typed `@native` entries.
///
/// Transformation:
/// - Parses a future native adapter contract shape through the formal syntax
///   output boundary without lowering it to a backend.
#[test]
fn syntax_output_accepts_native_annotation_metadata() {
    let output = parse_module_as_syntax_output(
        r#"
            module native_annotation_output.

            @native {
              adapter: "std_native_vector";
              runtime: "vm_native_worker";
              worker: true
            }
            pub opaque type Vector[T].
            "#,
    )
    .expect("native annotation syntax output");

    let annotations = &output.declarations[0].annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].path, vec!["native"]);
    assert_eq!(annotations[0].entries.len(), 3);
    assert!(annotations[0].values.is_empty());
    assert_eq!(annotations[0].entries[0].key, vec!["adapter"]);
    assert_eq!(
        annotations[0].entries[0].value,
        SyntaxAnnotationValueOutput::String {
            text: "\"std_native_vector\"".to_string()
        }
    );
    assert_eq!(annotations[0].entries[1].key, vec!["runtime"]);
    assert_eq!(
        annotations[0].entries[1].value,
        SyntaxAnnotationValueOutput::String {
            text: "\"vm_native_worker\"".to_string()
        }
    );
    assert_eq!(
        annotations[0].entries[2].value,
        SyntaxAnnotationValueOutput::Bool { value: true }
    );
}

/// Verifies user-declared annotation schemas survive syntax output.
///
/// Inputs:
/// - A public schema declaration with target, key, value-type, and option
///   metadata.
///
/// Output:
/// - Test passes when syntax output exposes the schema as
///   `AnnotationSchemaDecl` rather than raw text.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and inspects the
///   formal schema payload used by later annotation validation phases.

/// Verifies user-declared annotation schemas survive syntax output.
///
/// Inputs:
/// - A public schema declaration with target, key, value-type, and option
///   metadata.
///
/// Output:
/// - Test passes when syntax output exposes the schema as
///   `AnnotationSchemaDecl` rather than raw text.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and inspects the
///   formal schema payload used by later annotation validation phases.
#[test]
fn syntax_output_preserves_annotation_schema_declarations() {
    let output = parse_module_as_syntax_output(
        r#"
            module annotation_schema_output.

            pub annotation docs.example {
              applies_to: [function, method];
              name: String { required: true };
              enabled: Bool { default: false };
            }.
            "#,
    )
    .expect("annotation schema syntax output");

    assert_eq!(output.declarations[0].class, "AnnotationSchemaDecl");
    match &output.declarations[0].payload {
        SyntaxDeclarationPayload::AnnotationSchema {
            path,
            is_public,
            entries,
        } => {
            assert_eq!(path, &vec!["docs".to_string(), "example".to_string()]);
            assert!(*is_public);
            assert_eq!(entries.len(), 3);
            assert!(matches!(
                &entries[0],
                SyntaxAnnotationSchemaEntryOutput::AppliesTo { targets, .. }
                    if targets == &vec!["function".to_string(), "method".to_string()]
            ));
            assert!(matches!(
                &entries[1],
                SyntaxAnnotationSchemaEntryOutput::Key {
                    key,
                    value_type,
                    options,
                    ..
                } if key == &vec!["name".to_string()]
                    && value_type == "String"
                    && matches!(
                        options.as_slice(),
                        [SyntaxAnnotationKeyOptionOutput::Required { value: true, .. }]
                    )
            ));
            assert!(matches!(
                &entries[2],
                SyntaxAnnotationSchemaEntryOutput::Key {
                    key,
                    value_type,
                    options,
                    ..
                } if key == &vec!["enabled".to_string()]
                    && value_type == "Bool"
                    && matches!(
                        options.as_slice(),
                        [SyntaxAnnotationKeyOptionOutput::Default {
                            value: SyntaxAnnotationValueOutput::Bool { value: false },
                            ..
                        }]
                    )
            ));
        }
        other => panic!("unexpected annotation schema payload: {other:?}"),
    }
}

/// Verifies user-declared annotation schemas validate matching annotations.
///
/// Inputs:
/// - A schema declaration followed by a function annotated with matching
///   metadata.
///
/// Output:
/// - Test passes when the annotation is accepted and preserved.
///
/// Transformation:
/// - Proves user schema validation runs after declaration routing and
///   before any semantic/backend phase.

/// Verifies user-declared annotation schemas validate matching annotations.
///
/// Inputs:
/// - A schema declaration followed by a function annotated with matching
///   metadata.
///
/// Output:
/// - Test passes when the annotation is accepted and preserved.
///
/// Transformation:
/// - Proves user schema validation runs after declaration routing and
///   before any semantic/backend phase.
#[test]
fn syntax_output_accepts_user_declared_annotation_schema_usage() {
    let output = parse_module_as_syntax_output(
        r#"
            module user_annotation_schema_ok.

            annotation docs.example {
              applies_to: [function, method];
              name: String { required: true };
              tag: Name { repeatable: true };
            }.

            @docs.example { name: "demo"; tag: fast; tag: public }
            run(): Int -> 1.
            "#,
    )
    .expect("user annotation schema usage");

    assert_eq!(output.declarations.len(), 2);
    assert_eq!(
        output.declarations[1].annotations[0].path,
        vec!["docs", "example"]
    );
    assert_eq!(output.declarations[1].annotations[0].entries.len(), 3);
}
