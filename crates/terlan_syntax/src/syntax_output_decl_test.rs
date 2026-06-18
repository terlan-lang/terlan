use super::*;

#[cfg(test)]
mod tests {
    use super::*;

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
            type Tagged = :tagged.

            @target.erlang {
              otp_application: true
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
        assert_eq!(function_annotations[0].path, vec!["target", "erlang"]);
        let args = function_annotations[0]
            .args
            .as_deref()
            .expect("annotation args");
        assert!(args.starts_with('{'));
        assert!(args.ends_with('}'));
        assert!(args.contains("otp_application"));
        assert!(args.contains("true"));
        assert_eq!(function_annotations[0].entries.len(), 1);
        assert_eq!(
            function_annotations[0].entries[0].key,
            vec!["otp_application"]
        );
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
    /// - A function declaration annotated with an unknown `@target.erlang` key.
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
    /// - A function declaration annotated with an unknown `@target.erlang` key.
    ///
    /// Output:
    /// - Test passes when syntax output reports a key-level schema error.
    ///
    /// Transformation:
    /// - Checks that target metadata is typechecked by syntax output instead of
    ///   being deferred to backend-specific string handling.
    #[test]
    fn syntax_output_rejects_unknown_target_erlang_key() {
        let error = parse_module_as_syntax_output(
            r#"
            module bad_target_erlang_key.

            @target.erlang { unknown: true }
            run(): Int -> 1.
            "#,
        )
        .expect_err("@target.erlang should reject unknown keys");

        let message = format!("{error:?}");
        assert!(
            message.contains("@target.erlang has unknown key `unknown`"),
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
              runtime: "rust_tokio";
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

    /// Verifies user schema required keys are enforced.
    ///
    /// Inputs:
    /// - A schema with `required: true` and a matching annotation missing that
    ///   key.
    ///
    /// Output:
    /// - Test passes when syntax output reports the missing required key.
    ///
    /// Transformation:
    /// - Exercises required-key enforcement before semantic lowering.

    /// Verifies user schema required keys are enforced.
    ///
    /// Inputs:
    /// - A schema with `required: true` and a matching annotation missing that
    ///   key.
    ///
    /// Output:
    /// - Test passes when syntax output reports the missing required key.
    ///
    /// Transformation:
    /// - Exercises required-key enforcement before semantic lowering.
    #[test]
    fn syntax_output_rejects_user_annotation_missing_required_key() {
        let error = parse_module_as_syntax_output(
            r#"
            module user_annotation_schema_missing.

            annotation docs.example {
              applies_to: function;
              name: String { required: true };
            }.

            @docs.example {}
            run(): Int -> 1.
            "#,
        )
        .expect_err("missing required annotation key");

        let message = format!("{error:?}");
        assert!(
            message.contains("@docs.example missing required key `name`"),
            "unexpected diagnostic: {message}"
        );
    }

    /// Verifies user schemas reject unknown metadata keys.
    ///
    /// Inputs:
    /// - A schema with one legal key and a matching annotation using another
    ///   key.
    ///
    /// Output:
    /// - Test passes when syntax output reports an unknown key.
    ///
    /// Transformation:
    /// - Prevents user-declared schemas from silently accepting misspelled
    ///   metadata.

    /// Verifies user schemas reject unknown metadata keys.
    ///
    /// Inputs:
    /// - A schema with one legal key and a matching annotation using another
    ///   key.
    ///
    /// Output:
    /// - Test passes when syntax output reports an unknown key.
    ///
    /// Transformation:
    /// - Prevents user-declared schemas from silently accepting misspelled
    ///   metadata.
    #[test]
    fn syntax_output_rejects_user_annotation_unknown_key() {
        let error = parse_module_as_syntax_output(
            r#"
            module user_annotation_schema_unknown_key.

            annotation docs.example {
              applies_to: function;
              name: String;
            }.

            @docs.example { label: "demo" }
            run(): Int -> 1.
            "#,
        )
        .expect_err("unknown annotation key");

        let message = format!("{error:?}");
        assert!(
            message.contains("@docs.example has unknown key `label`"),
            "unexpected diagnostic: {message}"
        );
    }

    /// Verifies user schemas reject wrong value types.
    ///
    /// Inputs:
    /// - A schema requiring `Bool` and an annotation providing `String`.
    ///
    /// Output:
    /// - Test passes when syntax output reports the expected value type.
    ///
    /// Transformation:
    /// - Checks typed annotation metadata against user-declared key schemas.

    /// Verifies user schemas reject wrong value types.
    ///
    /// Inputs:
    /// - A schema requiring `Bool` and an annotation providing `String`.
    ///
    /// Output:
    /// - Test passes when syntax output reports the expected value type.
    ///
    /// Transformation:
    /// - Checks typed annotation metadata against user-declared key schemas.
    #[test]
    fn syntax_output_rejects_user_annotation_wrong_value_type() {
        let error = parse_module_as_syntax_output(
            r#"
            module user_annotation_schema_wrong_type.

            annotation docs.example {
              applies_to: function;
              enabled: Bool;
            }.

            @docs.example { enabled: "yes" }
            run(): Int -> 1.
            "#,
        )
        .expect_err("wrong annotation value type");

        let message = format!("{error:?}");
        assert!(
            message.contains("annotation key `enabled` expects Bool"),
            "unexpected diagnostic: {message}"
        );
    }

    /// Verifies user schemas reject duplicate non-repeatable keys.
    ///
    /// Inputs:
    /// - A schema with a default non-repeatable key and an annotation repeating
    ///   it.
    ///
    /// Output:
    /// - Test passes when syntax output reports the duplicate key.
    ///
    /// Transformation:
    /// - Applies the schema repeatability default of false to user metadata.

    /// Verifies user schemas reject duplicate non-repeatable keys.
    ///
    /// Inputs:
    /// - A schema with a default non-repeatable key and an annotation repeating
    ///   it.
    ///
    /// Output:
    /// - Test passes when syntax output reports the duplicate key.
    ///
    /// Transformation:
    /// - Applies the schema repeatability default of false to user metadata.
    #[test]
    fn syntax_output_rejects_user_annotation_duplicate_non_repeatable_key() {
        let error = parse_module_as_syntax_output(
            r#"
            module user_annotation_schema_duplicate_key.

            annotation docs.example {
              applies_to: function;
              tag: Name;
            }.

            @docs.example { tag: fast; tag: public }
            run(): Int -> 1.
            "#,
        )
        .expect_err("duplicate annotation key");

        let message = format!("{error:?}");
        assert!(
            message.contains("@docs.example key `tag` is not repeatable"),
            "unexpected diagnostic: {message}"
        );
    }

    /// Verifies key-level user schema targets are enforced.
    ///
    /// Inputs:
    /// - A schema whose key is only legal on methods, applied to a function.
    ///
    /// Output:
    /// - Test passes when syntax output reports the key target mismatch.
    ///
    /// Transformation:
    /// - Applies key-level `applies_to` restrictions after the annotation path
    ///   itself has matched the declaration.

    /// Verifies key-level user schema targets are enforced.
    ///
    /// Inputs:
    /// - A schema whose key is only legal on methods, applied to a function.
    ///
    /// Output:
    /// - Test passes when syntax output reports the key target mismatch.
    ///
    /// Transformation:
    /// - Applies key-level `applies_to` restrictions after the annotation path
    ///   itself has matched the declaration.
    #[test]
    fn syntax_output_rejects_user_annotation_key_target_mismatch() {
        let error = parse_module_as_syntax_output(
            r#"
            module user_annotation_schema_key_target.

            annotation docs.example {
              applies_to: [function, method];
              receiver: Name { applies_to: method };
            }.

            @docs.example { receiver: User }
            run(): Int -> 1.
            "#,
        )
        .expect_err("key target mismatch");

        let message = format!("{error:?}");
        assert!(
            message.contains("@docs.example key `receiver` cannot annotate FunctionDecl"),
            "unexpected diagnostic: {message}"
        );
    }

    /// Verifies receiver methods are emitted as formal method declarations.
    ///
    /// Inputs:
    /// - A module containing a receiver-style method declaration.
    ///
    /// Output:
    /// - Assertions over declaration class, receiver metadata, method params,
    ///   return type, and body payload.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and confirms
    ///   receiver-method syntax is no longer downgraded to raw output.

    /// Verifies receiver methods are emitted as formal method declarations.
    ///
    /// Inputs:
    /// - A module containing a receiver-style method declaration.
    ///
    /// Output:
    /// - Assertions over declaration class, receiver metadata, method params,
    ///   return type, and body payload.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and confirms
    ///   receiver-method syntax is no longer downgraded to raw output.
    #[test]
    fn syntax_output_preserves_receiver_methods_as_method_decls() {
        let output = parse_module_as_syntax_output(
            r#"
            module method_output.

            (self: User) identity(): User -> self.
            "#,
        )
        .expect("method syntax output");

        assert_eq!(output.declarations.len(), 1);
        assert_eq!(output.declarations[0].class, "MethodDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                clauses,
                ..
            } => {
                assert_eq!(receiver.name, "self");
                assert_eq!(receiver.annotation.text, "User");
                assert!(!receiver.is_mutable);
                assert_eq!(name, "identity");
                assert!(params.is_empty());
                assert_eq!(return_type.text, "User");
                assert_eq!(clauses.len(), 1);
                assert_eq!(clauses[0].body.kind, SyntaxExprKind::Var);
            }
            other => panic!("unexpected method payload: {other:?}"),
        }
    }

    /// Verifies mutable receiver metadata survives syntax output.
    ///
    /// Inputs:
    /// - A module containing a receiver method declared with contextual `mut`.
    ///
    /// Output:
    /// - Assertions over the method payload showing `receiver.mutable = true`.
    ///
    /// Transformation:
    /// - Parses source through syntax output and preserves the receiver
    ///   mutability marker without lowering or resolving its semantics.

    /// Verifies mutable receiver metadata survives syntax output.
    ///
    /// Inputs:
    /// - A module containing a receiver method declared with contextual `mut`.
    ///
    /// Output:
    /// - Assertions over the method payload showing `receiver.mutable = true`.
    ///
    /// Transformation:
    /// - Parses source through syntax output and preserves the receiver
    ///   mutability marker without lowering or resolving its semantics.
    #[test]
    fn syntax_output_preserves_mutable_receiver_marker() {
        let output = parse_module_as_syntax_output(
            r#"
            module method_output_mutable.

            pub (mut self: User) rename(name: String): User -> self.
            "#,
        )
        .expect("method syntax output");

        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Method { receiver, name, .. } => {
                assert_eq!(name, "rename");
                assert_eq!(receiver.name, "self");
                assert_eq!(receiver.annotation.text, "User");
                assert!(receiver.is_mutable);
            }
            other => panic!("unexpected method payload: {other:?}"),
        }
    }

    /// Verifies release core collection contracts survive formal syntax output.
    ///
    /// Inputs:
    /// - Release source contracts for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    ///
    /// Output:
    /// - Test passes when the formal syntax-output boundary preserves each
    ///   collection module name and each mutable receiver method required by
    ///   the P0.3 contract.
    ///
    /// Transformation:
    /// - Parses release contracts through `parse_module_as_syntax_output`,
    ///   filters method declarations, and checks receiver mutability without
    ///   typechecking or backend lowering.

    /// Verifies release core collection contracts survive formal syntax output.
    ///
    /// Inputs:
    /// - Release source contracts for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    ///
    /// Output:
    /// - Test passes when the formal syntax-output boundary preserves each
    ///   collection module name and each mutable receiver method required by
    ///   the P0.3 contract.
    ///
    /// Transformation:
    /// - Parses release contracts through `parse_module_as_syntax_output`,
    ///   filters method declarations, and checks receiver mutability without
    ///   typechecking or backend lowering.
    #[test]
    fn syntax_output_preserves_release_core_collection_contracts() {
        let contracts = [
            (
                "std.collections.Map",
                include_str!("../../../std/collections/map.terl"),
                vec![
                    ("put", "map", "Map[K, V]", true),
                    ("remove", "map", "Map[K, V]", true),
                    ("clear", "map", "Map[K, V]", true),
                ],
            ),
            (
                "std.collections.List",
                include_str!("../../../std/collections/list.terl"),
                vec![
                    ("push", "list", "List[T]", true),
                    ("clear", "list", "List[T]", true),
                ],
            ),
            (
                "std.collections.Set",
                include_str!("../../../std/collections/set.terl"),
                vec![
                    ("add", "set", "Set[T]", true),
                    ("remove", "set", "Set[T]", true),
                    ("clear", "set", "Set[T]", true),
                ],
            ),
        ];

        for (expected_module, source, expected_methods) in contracts {
            let output = parse_module_as_syntax_output(source)
                .expect("syntax output release collection contract");
            assert_eq!(output.module_name, expected_module);

            for (expected_name, expected_receiver_name, expected_receiver_type, is_mutable) in
                expected_methods
            {
                let method = output
                    .declarations
                    .iter()
                    .find_map(|declaration| match &declaration.payload {
                        SyntaxDeclarationPayload::Method { name, receiver, .. }
                            if name == expected_name =>
                        {
                            Some(receiver)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| {
                        panic!("missing method `{expected_name}` in {expected_module}")
                    });

                assert_eq!(method.name, expected_receiver_name);
                assert_eq!(method.annotation.text, expected_receiver_type);
                assert_eq!(method.is_mutable, is_mutable);
            }
        }
    }

    /// Verifies release iterator/iterable contracts survive syntax output.
    ///
    /// Inputs:
    /// - Release interface contracts for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    ///
    /// Output:
    /// - Test passes when syntax output preserves `Iterator.next` as a
    ///   function signature and `Iterable.iterator` as a trait method
    ///   signature.
    ///
    /// Transformation:
    /// - Parses release contracts through interface syntax output and inspects
    ///   structured declarations without typechecking or backend lowering.

    /// Verifies release iterator/iterable contracts survive syntax output.
    ///
    /// Inputs:
    /// - Release interface contracts for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    ///
    /// Output:
    /// - Test passes when syntax output preserves `Iterator.next` as a
    ///   function signature and `Iterable.iterator` as a trait method
    ///   signature.
    ///
    /// Transformation:
    /// - Parses release contracts through interface syntax output and inspects
    ///   structured declarations without typechecking or backend lowering.
    #[test]
    fn syntax_output_preserves_release_traversal_contracts() {
        let iterator_output =
            parse_module_as_syntax_output(include_str!("../../../std/collections/iterator.terl"))
                .expect("syntax output iterator contract");
        assert_eq!(iterator_output.module_name, "std.collections.Iterator");
        let iterator_function_shapes: Vec<String> = iterator_output
            .declarations
            .iter()
            .filter_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    return_type,
                    ..
                } => Some(format!(
                    "{}({}) -> {}",
                    name,
                    params
                        .iter()
                        .map(|param| param.annotation.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    return_type.text
                )),
                _ => None,
            })
            .collect();
        assert!(
            iterator_output.declarations.iter().any(|declaration| {
                matches!(
                    &declaration.payload,
                    SyntaxDeclarationPayload::Function {
                        name,
                        params,
                        return_type,
                        ..
                    } if name == "next"
                        && params.len() == 1
                        && params[0].annotation.text == "Iterator[T]"
                        && return_type.text == "Option[Step[T]]"
                )
            }),
            "iterator function shapes: {iterator_function_shapes:?}"
        );

        let iterable_output =
            parse_module_as_syntax_output(include_str!("../../../std/collections/iterable.terl"))
                .expect("syntax output iterable contract");
        assert_eq!(iterable_output.module_name, "std.collections.Iterable");
        let trait_decl = iterable_output
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Trait { name, methods, .. } if name == "Iterable" => {
                    Some(methods)
                }
                _ => None,
            })
            .expect("Iterable trait declaration");
        assert!(trait_decl.iter().any(|method| {
            method.name == "iterator"
                && method.params.len() == 1
                && method.params[0].annotation.text == "C"
                && method.return_type.text == "Iterator[T]"
        }));
    }

    /// Verifies canonical config declarations are exposed as structured syntax
    /// output instead of raw declarations.
    ///
    /// Inputs:
    /// - A module containing parser-preserved target config syntax.
    ///
    /// Outputs:
    /// - A `ConfigDecl` declaration class and `Config` payload with target text.
    ///
    /// Transformation:
    /// - Parses through the existing raw parser branch, then normalizes the
    ///   syntax-output payload to match the EBNF `ConfigDecl` contract.

    /// Verifies canonical config declarations are exposed as structured syntax
    /// output instead of raw declarations.
    ///
    /// Inputs:
    /// - A module containing parser-preserved target config syntax.
    ///
    /// Outputs:
    /// - A `ConfigDecl` declaration class and `Config` payload with target text.
    ///
    /// Transformation:
    /// - Parses through the existing raw parser branch, then normalizes the
    ///   syntax-output payload to match the EBNF `ConfigDecl` contract.
    #[test]
    fn syntax_output_normalizes_config_declarations() {
        let output = parse_module_as_syntax_output(
            r#"
            module config_output.

            target erlang {
              otp_application: true;
              adapter: postgres;
              features: [sockets, ssl];
              options: #{ssl: false, retries: 3}
            }.
            "#,
        )
        .expect("config syntax output");

        assert_eq!(output.declarations.len(), 1);
        assert_eq!(output.declarations[0].class, "ConfigDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Config {
                name,
                target,
                text,
                entries,
            } => {
                assert_eq!(name, "target");
                assert_eq!(target, "erlang");
                assert!(text.starts_with("target erlang {"));
                assert_eq!(entries.len(), 4);
                assert_eq!(entries[0].key, "otp_application");
                assert_eq!(
                    entries[0].value,
                    SyntaxConfigValueOutput::Bool { value: true }
                );
                assert_eq!(entries[1].key, "adapter");
                assert_eq!(
                    entries[1].value,
                    SyntaxConfigValueOutput::Symbol {
                        value: "postgres".to_string()
                    }
                );
                assert_eq!(entries[2].key, "features");
                assert_eq!(
                    entries[2].value,
                    SyntaxConfigValueOutput::List {
                        values: vec![
                            SyntaxConfigValueOutput::Symbol {
                                value: "sockets".to_string()
                            },
                            SyntaxConfigValueOutput::Symbol {
                                value: "ssl".to_string()
                            }
                        ]
                    }
                );
                assert_eq!(entries[3].key, "options");
                assert_eq!(
                    entries[3].value,
                    SyntaxConfigValueOutput::Map {
                        entries: vec![
                            SyntaxConfigEntryOutput {
                                key: "ssl".to_string(),
                                value: SyntaxConfigValueOutput::Bool { value: false },
                            },
                            SyntaxConfigEntryOutput {
                                key: "retries".to_string(),
                                value: SyntaxConfigValueOutput::Int {
                                    value: "3".to_string()
                                },
                            }
                        ]
                    }
                );
            }
            other => panic!("unexpected config payload: {other:?}"),
        }
    }

    #[test]
    fn interface_syntax_output_marks_source_kind() {
        let output = parse_interface_module_as_syntax_output(
            r#"
            module demo.

            export demo/1.
            "#,
        )
        .expect("interface syntax output");

        assert_eq!(output.source_kind, SyntaxSourceKind::Interface);
        assert_eq!(output.declarations.len(), 1);
        assert_eq!(output.declarations[0].class, "ExportDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Export { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "demo");
                assert_eq!(items[0].arity, 1);
            }
            other => panic!("unexpected export payload: {other:?}"),
        }
    }

    #[test]
    fn syntax_output_includes_struct_constructor_trait_and_template_signatures() {
        let output = parse_module_as_syntax_output(
            r#"
            module rich.

            pub struct User derives Person {
                /// Stable internal ID.
                id: Int,
                name: Text = :guest
            }.

            pub constructor Queue[T] {
                (Items: List[T], Limit: Int = 10): Queue[T] ->
                    from_list(Items)
            }.

            pub trait Show[A] {
                show(Value: A): Text.
            }.

            template Page from "./page.terl.html" {
                title: Text
            }.
            "#,
        )
        .expect("rich syntax output");

        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                derives,
                implements,
                fields,
            } => {
                assert_eq!(name, "User");
                assert!(*is_public);
                assert_eq!(derives, &vec!["Person".to_string()]);
                assert!(implements.is_empty());
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "id");
                assert_eq!(fields[0].annotation.text, "Int");
                assert_eq!(fields[0].docs, vec!["Stable internal ID."]);
                assert!(!fields[0].has_default);
                assert_eq!(fields[1].name, "name");
                assert_eq!(fields[1].annotation.text, "Text");
                assert!(fields[1].has_default);
                let default = fields[1].default.as_ref().expect("field default");
                assert_eq!(default.kind, SyntaxExprKind::Atom);
                assert_eq!(default.text.as_deref(), Some("guest"));
            }
            other => panic!("unexpected struct payload: {other:?}"),
        }

        match &output.declarations[1].payload {
            SyntaxDeclarationPayload::Constructor {
                name,
                params,
                is_public,
                clauses,
            } => {
                assert_eq!(name, "Queue");
                assert_eq!(params, &vec!["T".to_string()]);
                assert!(*is_public);
                assert_eq!(clauses.len(), 1);
                assert_eq!(clauses[0].params[0].name, "Items");
                assert_eq!(clauses[0].params[0].annotation.text, "List[T]");
                assert_eq!(clauses[0].params[1].name, "Limit");
                assert!(clauses[0].params[1].has_default);
                let default = clauses[0].params[1]
                    .default
                    .as_ref()
                    .expect("constructor param default");
                assert_eq!(default.kind, SyntaxExprKind::Int);
                assert_eq!(default.text.as_deref(), Some("10"));
                assert_eq!(clauses[0].return_type.text, "Queue[T]");
            }
            other => panic!("unexpected constructor payload: {other:?}"),
        }

        match &output.declarations[2].payload {
            SyntaxDeclarationPayload::Trait {
                name,
                params,
                is_public,
                methods,
                ..
            } => {
                assert_eq!(name, "Show");
                assert_eq!(params, &vec!["A".to_string()]);
                assert!(*is_public);
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].name, "show");
                assert_eq!(methods[0].params[0].name, "Value");
                assert_eq!(methods[0].params[0].annotation.text, "A");
                assert_eq!(methods[0].return_type.text, "Text");
            }
            other => panic!("unexpected trait payload: {other:?}"),
        }

        match &output.declarations[3].payload {
            SyntaxDeclarationPayload::Template {
                name,
                source_path,
                props,
            } => {
                assert_eq!(name, "Page");
                assert_eq!(source_path, "./page.terl.html");
                assert_eq!(props.len(), 1);
                assert_eq!(props[0].name, "title");
                assert_eq!(props[0].annotation.text, "Text");
            }
            other => panic!("unexpected template payload: {other:?}"),
        }
    }
}
