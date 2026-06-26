#[cfg(test)]
mod tests {
    use crate::parse_tree::{
        AnnotationKeyOption, AnnotationSchemaEntry, AnnotationValue, Decl, Expr,
    };
    use crate::{parse_interface_module, parse_module};

    /// Verifies every parser-visible declaration class in the canonical
    /// declaration inventory.
    ///
    /// Inputs:
    /// - A module containing imports, type, opaque type, struct, constructor,
    ///   trait, method, template, config macro, and function declarations.
    ///
    /// Output:
    /// - Test passes when parser declaration variants appear in the expected
    ///   order and module identity is stored separately from declarations.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and maps each
    ///   declaration variant to the same inventory classes used by grammar
    ///   fixture validation.
    #[test]
    fn formal_declaration_inventory_covers_parser_decl_classes() {
        let module = parse_module(
            r#"
            module declaration.inventory.

            import std.core.String.
            import type std.core.Option.
            import std.core.Option.{map as map_option, Option as MaybeOption}.
            import markdown "./readme.md" as readme.

            pub type Alias[T] = {:ok, value: T} | :none.
            pub opaque type Secret = Int.

            pub struct User {
              id: Int,
              name: String = ""
            }.

            pub constructor User {
              (id: Int, name: String): User -> #{
                id := id,
                name := name
              }
            }.

            pub trait Show[T] {
              show(value: T): String.
            }.

            (self: User) display(): User -> self.

            template Card from "./card.html" {
              title: String
            }.

            pub annotation docs.example {
              applies_to: [function, method];
              name: String { required: true };
            }.

            target js {
              runtime: oxc
            }.

            pub identity(value: Int): Int -> value.
            "#,
        )
        .expect("parse declaration inventory");

        assert_eq!(module.name, "declaration.inventory");
        let classes = module
            .declarations
            .iter()
            .map(|decl| match decl {
                Decl::Import(_) => "import_decl",
                Decl::Type(type_decl) if type_decl.is_opaque => "opaque_type_decl",
                Decl::Type(_) => "type_decl",
                Decl::Struct(_) => "struct_decl",
                Decl::Constructor(_) => "constructor_decl",
                Decl::Function(_) => "function_decl",
                Decl::Method(_) => "method_decl",
                Decl::Trait(_) => "trait_decl",
                Decl::TraitImpl(_) => "trait_impl_decl",
                Decl::AnnotationSchema(_) => "annotation_schema_decl",
                Decl::Template(_) => "template_decl",
                Decl::Raw(_) => "raw_decl",
                Decl::Export(_) => panic!("source parser must not produce export declarations"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            classes,
            vec![
                "import_decl",
                "import_decl",
                "import_decl",
                "import_decl",
                "type_decl",
                "opaque_type_decl",
                "struct_decl",
                "constructor_decl",
                "trait_decl",
                "method_decl",
                "template_decl",
                "annotation_schema_decl",
                "raw_decl",
                "function_decl"
            ]
        );
    }

    /// Verifies the reserved `template` keyword can appear as a package path
    /// segment in qualified module names.
    ///
    /// Inputs:
    /// - A module declaration under `std.template`.
    /// - A direct item import through `std.template.Template`.
    /// - A braced import from the full `std.template.Template` module.
    /// - A following template declaration whose keyword must not be consumed as
    ///   part of the import path.
    ///
    /// Output:
    /// - Test passes when both paths are preserved as module names.
    ///
    /// Transformation:
    /// - Exercises the module-path parser without changing expression or
    ///   binding grammar, where `template` remains reserved.
    #[test]
    fn module_paths_accept_template_package_segment() {
        let module = parse_module(
            r#"
            module std.template.Template.

            import std.template.Template.
            import std.template.Template.{trusted}.

            template Layout from "./layout.terl.html" {
              body: Template.Html
            }.

            pub opaque type Html.
            "#,
        )
        .expect("parse std template module path");

        assert_eq!(module.name, "std.template.Template");
        let Decl::Import(import) = &module.declarations[0] else {
            panic!("expected import");
        };
        assert_eq!(import.module_name, "std.template");
        assert_eq!(import.items[0].name, "Template");
        let Decl::Import(braced_import) = &module.declarations[1] else {
            panic!("expected braced import");
        };
        assert_eq!(braced_import.module_name, "std.template.Template");
        assert_eq!(braced_import.items[0].name, "trusted");
        assert!(matches!(module.declarations[2], Decl::Template(_)));
    }

    /// Verifies wildcard module imports parse as selected import declarations.
    ///
    /// Inputs:
    /// - Canonical braced wildcard import syntax.
    /// - Compatibility path-style wildcard import syntax.
    ///
    /// Output:
    /// - Test passes when both imports preserve `*` as the selected import item.
    ///
    /// Transformation:
    /// - Exercises wildcard import parsing without expanding symbols; semantic
    ///   expansion belongs to HIR/typecheck once provider interfaces are loaded.
    #[test]
    fn parses_wildcard_imports() {
        let module = parse_module(
            r#"
            module app.Wildcard.

            import test.Other.{*}.
            import test.Legacy.*.

            pub main(): Int -> 1.
            "#,
        )
        .expect("parse wildcard imports");

        let Decl::Import(braced_import) = &module.declarations[0] else {
            panic!("expected braced wildcard import");
        };
        assert_eq!(braced_import.module_name, "test.Other");
        assert_eq!(braced_import.items[0].name, "*");

        let Decl::Import(path_import) = &module.declarations[1] else {
            panic!("expected path wildcard import");
        };
        assert_eq!(path_import.module_name, "test.Legacy");
        assert_eq!(path_import.items[0].name, "*");
    }

    /// Verifies annotation schema declarations parse as structured parse tree.
    ///
    /// Inputs:
    /// - A public annotation schema with declaration targets, a required string
    ///   key, a repeatable name key, and a default boolean key.
    ///
    /// Output:
    /// - Test passes when the parser preserves path, visibility, entries,
    ///   option values, default metadata, and spans.
    ///
    /// Transformation:
    /// - Parses source through `parse_module` and inspects the
    ///   `Decl::AnnotationSchema` payload directly.

    /// Verifies annotation schema declarations parse as structured parse tree.
    ///
    /// Inputs:
    /// - A public annotation schema with declaration targets, a required string
    ///   key, a repeatable name key, and a default boolean key.
    ///
    /// Output:
    /// - Test passes when the parser preserves path, visibility, entries,
    ///   option values, default metadata, and spans.
    ///
    /// Transformation:
    /// - Parses source through `parse_module` and inspects the
    ///   `Decl::AnnotationSchema` payload directly.
    #[test]
    fn parses_annotation_schema_declarations() {
        let module = parse_module(
            r#"
            module annotation.schema.inventory.

            pub annotation docs.example {
              applies_to: [function, method];
              name: String { required: true };
              tag: Name { repeatable: true; applies_to: function };
              enabled: Bool { default: false };
            }.
            "#,
        )
        .expect("parse annotation schema declaration");

        let Decl::AnnotationSchema(schema) = &module.declarations[0] else {
            panic!("expected annotation schema declaration");
        };

        assert!(schema.is_public);
        assert_eq!(schema.path, vec!["docs", "example"]);
        assert_eq!(schema.entries.len(), 4);

        match &schema.entries[0] {
            AnnotationSchemaEntry::AppliesTo { targets, .. } => {
                assert_eq!(targets, &vec!["function".to_string(), "method".to_string()]);
            }
            other => panic!("unexpected applies_to entry: {other:?}"),
        }

        match &schema.entries[1] {
            AnnotationSchemaEntry::Key {
                key,
                value_type,
                options,
                ..
            } => {
                assert_eq!(key, &vec!["name".to_string()]);
                assert_eq!(value_type.text, "String");
                assert!(matches!(
                    options.as_slice(),
                    [AnnotationKeyOption::Required { value: true, .. }]
                ));
            }
            other => panic!("unexpected key entry: {other:?}"),
        }

        match &schema.entries[2] {
            AnnotationSchemaEntry::Key {
                key,
                value_type,
                options,
                ..
            } => {
                assert_eq!(key, &vec!["tag".to_string()]);
                assert_eq!(value_type.text, "Name");
                assert_eq!(options.len(), 2);
                assert!(matches!(
                    options[0],
                    AnnotationKeyOption::Repeatable { value: true, .. }
                ));
                assert!(matches!(
                    &options[1],
                    AnnotationKeyOption::AppliesTo { targets, .. }
                        if targets == &vec!["function".to_string()]
                ));
            }
            other => panic!("unexpected tag key entry: {other:?}"),
        }

        match &schema.entries[3] {
            AnnotationSchemaEntry::Key {
                key,
                value_type,
                options,
                ..
            } => {
                assert_eq!(key, &vec!["enabled".to_string()]);
                assert_eq!(value_type.text, "Bool");
                assert!(matches!(
                    options.as_slice(),
                    [AnnotationKeyOption::Default {
                        value: AnnotationValue::Bool(false),
                        ..
                    }]
                ));
            }
            other => panic!("unexpected enabled key entry: {other:?}"),
        }
    }

    /// Verifies the A0.27 type-family syntax inventory.
    ///
    /// Inputs:
    /// - A module containing aliases, opaque aliases, unions, tuples, named
    ///   tuple fields, map types, arrow types, generic references, lists, and
    ///   type literals.
    ///
    /// Output:
    /// - Test passes when type declarations parse and preserve their type text
    ///   for later semantic/type-family validation.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and inspects
    ///   selected preserved `TypeExpr` text and opaque/public flags.

    /// Verifies the A0.27 type-family syntax inventory.
    ///
    /// Inputs:
    /// - A module containing aliases, opaque aliases, unions, tuples, named
    ///   tuple fields, map types, arrow types, generic references, lists, and
    ///   type literals.
    ///
    /// Output:
    /// - Test passes when type declarations parse and preserve their type text
    ///   for later semantic/type-family validation.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and inspects
    ///   selected preserved `TypeExpr` text and opaque/public flags.
    #[test]
    fn formal_type_family_inventory_preserves_type_expr_text() {
        let module = parse_module(
            r#"
            module types.family.inventory.

            pub type Maybe[T] = :none | {:some, value: T}.
            type Pair = {left: Int, right: String}.
            type IgnoredField = {_: Int, value: String}.
            type Lookup[K, V] = #{key := K, value => V}.
            type Mapper[A, B] = (A) -> B.
            type Nested = std.core.Option[String].
            type Names = [String].
            type LiteralUnion = :empty | :'Interop.Empty' | 0 | 1.5 | "ready".

            pub opaque type Secret[T] = #{value := T}.
            pub opaque type Handle.
            "#,
        )
        .expect("parse type-family inventory");

        assert_eq!(module.declarations.len(), 10);

        let Decl::Type(maybe) = &module.declarations[0] else {
            panic!("expected Maybe type");
        };
        assert!(maybe.is_public);
        assert_eq!(maybe.params, vec!["T"]);
        assert_eq!(maybe.variants.len(), 2);
        assert!(maybe.variants[0].text.contains("none"));
        assert!(maybe.variants[1].text.contains("value"));

        let Decl::Type(mapper) = &module.declarations[4] else {
            panic!("expected Mapper type");
        };
        assert_eq!(mapper.params, vec!["A", "B"]);
        assert_eq!(mapper.variants.len(), 1);
        assert!(mapper.variants[0].text.contains("->"));

        let Decl::Type(nested) = &module.declarations[5] else {
            panic!("expected Nested type");
        };
        assert!(nested.variants[0].text.contains("std.core.Option"));
        assert!(nested.variants[0].text.contains("[String]"));

        let Decl::Type(secret) = &module.declarations[8] else {
            panic!("expected Secret opaque type");
        };
        assert!(secret.is_public);
        assert!(secret.is_opaque);
        assert_eq!(secret.params, vec!["T"]);
        assert!(secret.variants[0].text.contains("value"));

        let Decl::Type(handle) = &module.declarations[9] else {
            panic!("expected Handle opaque type");
        };
        assert!(handle.is_public);
        assert!(handle.is_opaque);
        assert!(handle.variants.is_empty());
    }

    /// Verifies type-position diagnostics for runtime expression syntax.
    ///
    /// Inputs:
    /// - A type declaration whose right-hand side starts with a `case`
    ///   expression.
    ///
    /// Output:
    /// - Test passes when parsing fails before the type can enter later
    ///   compiler phases.
    ///
    /// Transformation:
    /// - Parses one malformed module and asserts the stable runtime-token
    ///   diagnostic remains attached to type parsing.

    /// Verifies type-position diagnostics for runtime expression syntax.
    ///
    /// Inputs:
    /// - A type declaration whose right-hand side starts with a `case`
    ///   expression.
    ///
    /// Output:
    /// - Test passes when parsing fails before the type can enter later
    ///   compiler phases.
    ///
    /// Transformation:
    /// - Parses one malformed module and asserts the stable runtime-token
    ///   diagnostic remains attached to type parsing.
    #[test]
    fn formal_type_family_rejects_runtime_expression_tokens() {
        let error = parse_module(
            r#"
            module bad.bad_type.

            type Foo = case x { y -> z }.
            "#,
        )
        .err()
        .expect("runtime expression in type should fail");

        assert!(
            error
                .message
                .contains("runtime expression token 'case' is not valid in type position"),
            "unexpected diagnostic: {}",
            error.message
        );
    }

    /// Verifies the A0.28 method receiver syntax baseline.
    ///
    /// Inputs:
    /// - A module with a struct and two receiver method declarations,
    ///   including receiver type arguments, method parameters, visibility, and
    ///   field access in a method body.
    ///
    /// Output:
    /// - Test passes when methods are accepted as structured `MethodDecl`
    ///   declarations and preserve receiver, method, and body data.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and inspects
    ///   the structured receiver-method parse tree used by later syntax output,
    ///   typechecking, and backend lowering.

    /// Verifies the A0.28 method receiver syntax baseline.
    ///
    /// Inputs:
    /// - A module with a struct and two receiver method declarations,
    ///   including receiver type arguments, method parameters, visibility, and
    ///   field access in a method body.
    ///
    /// Output:
    /// - Test passes when methods are accepted as structured `MethodDecl`
    ///   declarations and preserve receiver, method, and body data.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser and inspects
    ///   the structured receiver-method parse tree used by later syntax output,
    ///   typechecking, and backend lowering.
    #[test]
    fn formal_method_receiver_inventory_preserves_validated_methods() {
        let module = parse_module(
            r#"
            module methods.receiver.inventory.

            struct Box {
              value: Int
            }.

            (self: Box[Int]) value(): Int -> self.value.

            pub (self: Box[Int]) replace(value: Int): Box[Int] -> self.
            "#,
        )
        .expect("parse method receiver inventory");

        assert_eq!(module.declarations.len(), 3);
        assert!(matches!(&module.declarations[0], Decl::Struct(_)));

        let Decl::Method(value_method) = &module.declarations[1] else {
            panic!("expected first method");
        };
        assert_eq!(value_method.name, "value");
        assert_eq!(value_method.receiver.name, "self");
        assert_eq!(value_method.receiver.annotation.text, "Box[Int]");
        assert!(!value_method.receiver.is_mutable);

        let Decl::Method(replace_method) = &module.declarations[2] else {
            panic!("expected second method");
        };
        assert_eq!(replace_method.name, "replace");
        assert_eq!(replace_method.params.len(), 1);
        assert!(replace_method.is_public);
        assert!(!replace_method.receiver.is_mutable);
    }

    /// Verifies mutable receiver syntax is parsed without enabling semantics.
    ///
    /// Inputs:
    /// - A module with a receiver method declared as `(mut self: Box[Int])`.
    ///
    /// Output:
    /// - Test passes when the method is preserved as a structured declaration
    ///   and the receiver metadata records `is_mutable`.
    ///
    /// Transformation:
    /// - Parses the contextual `mut` marker before the receiver binding and
    ///   stores it on the receiver parameter for later semantic validation.

    /// Verifies mutable receiver syntax is parsed without enabling semantics.
    ///
    /// Inputs:
    /// - A module with a receiver method declared as `(mut self: Box[Int])`.
    ///
    /// Output:
    /// - Test passes when the method is preserved as a structured declaration
    ///   and the receiver metadata records `is_mutable`.
    ///
    /// Transformation:
    /// - Parses the contextual `mut` marker before the receiver binding and
    ///   stores it on the receiver parameter for later semantic validation.
    #[test]
    fn formal_method_receiver_inventory_preserves_mutable_receiver_marker() {
        let module = parse_module(
            r#"
            module methods.receiver.mutable.

            struct Box {
              value: Int
            }.

            pub (mut self: Box[Int]) replace(value: Int): Box[Int] -> self.
            "#,
        )
        .expect("parse mutable method receiver inventory");

        let Decl::Method(method) = &module.declarations[1] else {
            panic!("expected mutable receiver method");
        };
        assert_eq!(method.name, "replace");
        assert_eq!(method.receiver.name, "self");
        assert_eq!(method.receiver.annotation.text, "Box[Int]");
        assert!(method.receiver.is_mutable);
    }

    /// Verifies method receiver/name diagnostics required by A0.28.
    ///
    /// Inputs:
    /// - Three malformed method declarations with an upper-case receiver
    ///   binding, lower-case receiver type, and upper-case method name.
    ///
    /// Output:
    /// - Test passes when each malformed method fails with the expected stable
    ///   diagnostic fragment.
    ///
    /// Transformation:
    /// - Parses each module independently and compares the diagnostic message
    ///   against the receiver/method grammar rule that was violated.

    /// Verifies method receiver/name diagnostics required by A0.28.
    ///
    /// Inputs:
    /// - Three malformed method declarations with an upper-case receiver
    ///   binding, lower-case receiver type, and upper-case method name.
    ///
    /// Output:
    /// - Test passes when each malformed method fails with the expected stable
    ///   diagnostic fragment.
    ///
    /// Transformation:
    /// - Parses each module independently and compares the diagnostic message
    ///   against the receiver/method grammar rule that was violated.
    #[test]
    fn formal_method_receiver_diagnostics_reject_invalid_method_heads() {
        let cases = [
            (
                r#"
                module bad.uppercase_method_receiver_name.

                struct User {
                  id: Int
                }.

                (Self: User) identity(): User -> Self.
                "#,
                "expected lower-case method receiver name",
            ),
            (
                r#"
                module bad.lowercase_method_receiver_type.

                (self: user) identity(): user -> self.
                "#,
                "expected upper-case type name",
            ),
            (
                r#"
                module bad.uppercase_method_name.

                struct User {
                  id: Int
                }.

                (self: User) Rename(): User -> self.
                "#,
                "expected lower-case method name",
            ),
        ];

        for (source, expected) in cases {
            let error = parse_module(source)
                .err()
                .expect("invalid method head should fail");
            assert!(
                error.message.contains(expected),
                "expected diagnostic containing `{expected}`, got `{}`",
                error.message
            );
        }
    }

    /// Verifies unsupported annotation subjects fail with a stable diagnostic.
    ///
    /// Inputs:
    /// - Modules containing subject-bearing annotation forms that are
    ///   unambiguous without line-boundary information.
    ///
    /// Output:
    /// - Parser diagnostics containing the A0.32 unsupported-subject message.
    ///
    /// Transformation:
    /// - Parses each source module and confirms annotation subjects are stopped
    ///   before declaration routing or backend phases can observe them.

    /// Verifies unsupported annotation subjects fail with a stable diagnostic.
    ///
    /// Inputs:
    /// - Modules containing subject-bearing annotation forms that are
    ///   unambiguous without line-boundary information.
    ///
    /// Output:
    /// - Parser diagnostics containing the A0.32 unsupported-subject message.
    ///
    /// Transformation:
    /// - Parses each source module and confirms annotation subjects are stopped
    ///   before declaration routing or backend phases can observe them.
    #[test]
    fn formal_annotation_subjects_are_rejected_before_declaration_routing() {
        let cases = [
            r#"
            module bad.annotation_upper_subject.

            @compiler.inline User
            type User = Int.
            "#,
            r#"
            module bad.annotation_qualified_subject.

            @target std.core {
              enabled: true
            }
            type User = Int.
            "#,
            r#"
            module bad.annotation_literal_subject.

            @doc "User type"
            type User = Int.
            "#,
        ];

        for source in cases {
            let error = parse_module(source)
                .err()
                .expect("annotation subject should fail");
            assert!(
                error
                    .message
                    .contains("annotation subjects are not supported in Terlan 0.0.1"),
                "unexpected diagnostic: {}",
                error.message
            );
        }
    }

    /// Verifies declaration-leading annotations still support lower-case
    /// functions despite the subject rejection pass.
    ///
    /// Inputs:
    /// - A module with a declaration-leading `@test` annotation before a
    ///   lower-case function declaration.
    ///
    /// Output:
    /// - A parsed module containing one annotated function declaration.
    ///
    /// Transformation:
    /// - Exercises the ambiguous lower-identifier case that is intentionally
    ///   left to declaration parsing until lexer line-boundary data exists.

    /// Verifies declaration-leading annotations still support lower-case
    /// functions despite the subject rejection pass.
    ///
    /// Inputs:
    /// - A module with a declaration-leading `@test` annotation before a
    ///   lower-case function declaration.
    ///
    /// Output:
    /// - A parsed module containing one annotated function declaration.
    ///
    /// Transformation:
    /// - Exercises the ambiguous lower-identifier case that is intentionally
    ///   left to declaration parsing until lexer line-boundary data exists.
    #[test]
    fn formal_declaration_annotation_before_function_still_parses() {
        let module = parse_module(
            r#"
            module ok.annotation_function.

            @test
            passes(): Bool -> true.
            "#,
        )
        .expect("declaration-leading annotation");

        assert_eq!(module.declarations.len(), 1);
        assert_eq!(module.declaration_annotations.len(), 1);
        assert_eq!(module.declaration_annotations[0][0].path, vec!["test"]);
    }

    /// Verifies the A0.29 trait and primitive conformance syntax inventory.
    ///
    /// Inputs:
    /// - A module declaring `Show`, `Parse`, `Convertable`, and `Textual`
    ///   traits plus functions that call trait methods for primitive `Bool`.
    ///
    /// Output:
    /// - Test passes when trait declarations, super-trait references, method
    ///   signatures, and trait method calls are preserved by the parser.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser, inspects trait
    ///   declaration metadata, and confirms trait calls remain ordinary
    ///   function declarations for later semantic conformance resolution.

    /// Verifies the A0.29 trait and primitive conformance syntax inventory.
    ///
    /// Inputs:
    /// - A module declaring `Show`, `Parse`, `Convertable`, and `Textual`
    ///   traits plus functions that call trait methods for primitive `Bool`.
    ///
    /// Output:
    /// - Test passes when trait declarations, super-trait references, method
    ///   signatures, and trait method calls are preserved by the parser.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser, inspects trait
    ///   declaration metadata, and confirms trait calls remain ordinary
    ///   function declarations for later semantic conformance resolution.
    #[test]
    fn formal_trait_conformance_inventory_preserves_trait_surface() {
        let module = parse_module(
            r#"
            module traits.conformance.inventory.

            pub trait Show[T] {
              to_string(value: T): String.
            }.

            pub trait Parse[T] {
              from_string(value: String): Option[T].
            }.

            pub trait Convertable[From, To] {
              convert(value: From): To.
            }.

            pub trait Textual[T] extends Convertable[T, String], Convertable[String, T] {
            }.

            render_bool(value: Bool): String ->
              Show.to_string(value).

            parse_bool(value: String): Option[Bool] ->
              Parse.from_string(value).
            "#,
        )
        .expect("parse trait conformance inventory");

        assert_eq!(module.declarations.len(), 6);

        let Decl::Trait(show) = &module.declarations[0] else {
            panic!("expected Show trait");
        };
        assert!(show.is_public);
        assert_eq!(show.name, "Show");
        assert_eq!(show.params, vec!["T"]);
        assert_eq!(show.methods.len(), 1);
        assert_eq!(show.methods[0].name, "to_string");
        assert_eq!(show.methods[0].return_type.text, "String");

        let Decl::Trait(parse) = &module.declarations[1] else {
            panic!("expected Parse trait");
        };
        assert_eq!(parse.methods[0].name, "from_string");
        assert!(parse.methods[0].return_type.text.contains("Option"));

        let Decl::Trait(textual) = &module.declarations[3] else {
            panic!("expected Textual trait");
        };
        assert_eq!(textual.super_traits.len(), 2);
        assert!(textual.super_traits[0].contains("Convertable"));
        assert!(textual.super_traits[1].contains("String"));

        assert!(matches!(&module.declarations[4], Decl::Function(_)));
        assert!(matches!(&module.declarations[5], Decl::Function(_)));
    }

    /// Verifies declaration-site trait conformance syntax preserves the
    /// Java-style `implements` form without requiring an explicit impl block.
    ///
    /// Inputs:
    /// - A struct declaring `implements Show[User]`.
    /// - A receiver method satisfying that conformance.
    ///
    /// Output:
    /// - Parsed declaration shapes and conformance metadata.
    ///
    /// Transformation:
    /// - Parses the source through the formal recursive-descent parser and
    ///   confirms declaration-site conformance is preserved on the struct while
    ///   behavior remains an ordinary receiver method.

    /// Verifies declaration-site trait conformance syntax preserves the
    /// Java-style `implements` form without requiring an explicit impl block.
    ///
    /// Inputs:
    /// - A struct declaring `implements Show[User]`.
    /// - A receiver method satisfying that conformance.
    ///
    /// Output:
    /// - Parsed declaration shapes and conformance metadata.
    ///
    /// Transformation:
    /// - Parses the source through the formal recursive-descent parser and
    ///   confirms declaration-site conformance is preserved on the struct while
    ///   behavior remains an ordinary receiver method.
    #[test]
    fn formal_trait_conformance_syntax_supports_implements_with_receiver_method() {
        let module = parse_module(
            r#"
            module traits.conformance.forms.

            pub trait Show[T] {
              to_string(value: T): String.
            }.

            pub struct User implements Show[User] {
              id: Int,
              name: String
            }.

            pub (user: User) to_string(): String ->
              user.name.
            "#,
        )
        .expect("parse declaration-site conformance form");

        assert_eq!(module.declarations.len(), 3);

        let Decl::Trait(show) = &module.declarations[0] else {
            panic!("expected Show trait");
        };
        assert_eq!(show.methods.len(), 1);
        assert!(show.methods[0].default_body.is_none());

        let Decl::Struct(user) = &module.declarations[1] else {
            panic!("expected User struct");
        };
        assert_eq!(user.implements.len(), 1);
        assert_eq!(user.implements[0].text, "Show[User]");

        assert!(
            matches!(&module.declarations[2], Decl::Method(method) if method.name == "to_string")
        );
    }

    /// Verifies explicit trait implementation blocks are parsed as adapter
    /// conformances.
    ///
    /// Inputs:
    /// - A module with `impl Show[ExternalUser] for ExternalUser`.
    ///
    /// Output:
    /// - Parsed `TraitImplDecl` with one implementation method.
    ///
    /// Transformation:
    /// - Confirms explicit adapter conformance is structured separately from
    ///   declaration-site `implements` and from raw declarations.

    /// Verifies explicit trait implementation blocks are parsed as adapter
    /// conformances.
    ///
    /// Inputs:
    /// - A module with `impl Show[ExternalUser] for ExternalUser`.
    ///
    /// Output:
    /// - Parsed `TraitImplDecl` with one implementation method.
    ///
    /// Transformation:
    /// - Confirms explicit adapter conformance is structured separately from
    ///   declaration-site `implements` and from raw declarations.
    #[test]
    fn formal_trait_conformance_syntax_supports_explicit_impl_blocks() {
        let module = parse_module(
            r#"
            module traits.conformance.adapter.

            pub impl Show[ExternalUser] for ExternalUser {
              to_string(value: ExternalUser): String ->
                value.name.
            }.
            "#,
        )
        .expect("parse explicit conformance adapter");

        assert_eq!(module.declarations.len(), 1);
        let Decl::TraitImpl(external_impl) = &module.declarations[0] else {
            panic!("expected explicit trait impl");
        };
        assert!(external_impl.is_public);
        assert_eq!(external_impl.trait_ref.text, "Show[ExternalUser]");
        assert_eq!(external_impl.for_type.text, "ExternalUser");
        assert_eq!(external_impl.methods.len(), 1);
        assert_eq!(external_impl.methods[0].name, "to_string");
        assert_eq!(external_impl.methods[0].clauses.len(), 1);
    }

    /// Verifies traits may provide default method bodies.
    ///
    /// Inputs:
    /// - A trait with one signature-only method and one default method.
    ///
    /// Output:
    /// - Trait method metadata indicating which method owns a default body.
    ///
    /// Transformation:
    /// - Parses default trait behavior without introducing an external impl
    ///   declaration, matching the Java-style default-method model.

    /// Verifies traits may provide default method bodies.
    ///
    /// Inputs:
    /// - A trait with one signature-only method and one default method.
    ///
    /// Output:
    /// - Trait method metadata indicating which method owns a default body.
    ///
    /// Transformation:
    /// - Parses default trait behavior without introducing an external impl
    ///   declaration, matching the Java-style default-method model.
    #[test]
    fn formal_trait_conformance_syntax_supports_trait_default_methods() {
        let module = parse_module(
            r#"
            module traits.conformance.defaults.

            pub trait Show[T] {
              to_string(value: T): String.
              debug(value: T): String -> to_string(value).
            }.
            "#,
        )
        .expect("parse default trait method");

        let Decl::Trait(show) = &module.declarations[0] else {
            panic!("expected Show trait");
        };
        assert_eq!(show.methods.len(), 2);
        assert!(show.methods[0].default_body.is_none());
        assert!(show.methods[1].default_body.is_some());
    }

    /// Verifies trait method parameters may require mutability.
    ///
    /// Inputs:
    /// - A trait method with `mut` on its first parameter.
    ///
    /// Output:
    /// - Trait method parameter metadata preserving `is_mutable`.
    ///
    /// Transformation:
    /// - Parses mutable parameter syntax in trait contracts so collection
    ///   mutation traits can express receiver-like mutation requirements.

    /// Verifies trait method parameters may require mutability.
    ///
    /// Inputs:
    /// - A trait method with `mut` on its first parameter.
    ///
    /// Output:
    /// - Trait method parameter metadata preserving `is_mutable`.
    ///
    /// Transformation:
    /// - Parses mutable parameter syntax in trait contracts so collection
    ///   mutation traits can express receiver-like mutation requirements.
    #[test]
    fn formal_trait_methods_preserve_mutable_parameters() {
        let module = parse_module(
            r#"
            module traits.mutable.params.

            pub trait IndexSet[C, I, T] {
              set_at(mut collection: C, index: I, value: T): Unit.
            }.
            "#,
        )
        .expect("parse mutable trait parameter");

        let Decl::Trait(index_set) = &module.declarations[0] else {
            panic!("expected IndexSet trait");
        };
        let method = &index_set.methods[0];
        assert_eq!(method.params.len(), 3);
        assert!(method.params[0].is_mutable);
        assert!(!method.params[1].is_mutable);
        assert!(!method.params[2].is_mutable);
    }

    /// Verifies canonical callable constraint-list parsing.
    ///
    /// Inputs:
    /// - A module containing a generic function with `[Eq[A], Show[A]]` after
    ///   its parameter list.
    ///
    /// Output:
    /// - Parsed function declaration with preserved generic-bound strings.
    ///
    /// Transformation:
    /// - Exercises the canonical EBNF constraint-list position and confirms
    ///   constraints are kept for typechecker lowering.
    #[test]
    fn parses_function_declaration_with_constraint_list() {
        let source = r#"
module bounds_demo.

pub debug[A](X: A, Y: A)[Eq[A], Show[A]]: Text ->
    case Eq.equal(X, Y) {
        true -> Show.render(X);
        false -> "neq"
    }.
"#;

        let module = parse_module(source).expect("parse constraint-list function");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function declaration"),
        };
        assert_eq!(function.name, "debug");
        assert_eq!(function.params.len(), 2);
        assert_eq!(
            function.generic_bounds,
            vec!["Eq[A]".to_string(), "Show[A]".to_string()]
        );
    }

    /// Verifies function-like declarations accept trailing default parameters.
    ///
    /// Inputs:
    /// - A function and receiver method with trailing default values.
    ///
    /// Output:
    /// - Parsed callable declarations preserving the default expressions.
    ///
    /// Transformation:
    /// - Exercises the 0.0.5 callable default-parameter syntax without
    ///   requiring call-site omission semantics yet.
    #[test]
    fn parses_function_and_method_default_parameters() {
        let source = r#"
module defaults_demo.

pub add(X: Int, Step: Int = 1): Int ->
    X + Step.

pub (value: Int) clamp(Min: Int = 0, Max: Int = 10): Int ->
    value.
"#;

        let module = parse_module(source).expect("parse callable defaults");
        let Decl::Function(function) = &module.declarations[0] else {
            panic!("expected function declaration");
        };
        assert!(function.params[1].default.is_some());

        let Decl::Method(method) = &module.declarations[1] else {
            panic!("expected method declaration");
        };
        assert!(method.params[0].default.is_some());
        assert!(method.params[1].default.is_some());
    }

    /// Verifies required callable parameters may not follow defaults.
    ///
    /// Inputs:
    /// - A function whose second parameter has a default and third parameter
    ///   does not.
    ///
    /// Output:
    /// - Parse error anchored by the shared trailing-default rule.
    ///
    /// Transformation:
    /// - Locks down deterministic callable arity before omitted-argument
    ///   call-site semantics are implemented.
    #[test]
    fn rejects_required_parameter_after_function_default_parameter() {
        let source = r#"
module defaults_bad.

pub add(X: Int = 1, Step: Int): Int ->
    X + Step.
"#;

        let err = parse_module(source).expect_err("required param after default");
        assert_eq!(err.message, "default parameters must be trailing");
    }

    /// Verifies canonical constraint lists on non-function callable forms.
    ///
    /// Inputs:
    /// - A module containing a trait method, receiver method, and explicit impl
    ///   method with post-parameter constraint lists.
    ///
    /// Output:
    /// - Parsed declarations whose `generic_bounds` preserve each constraint
    ///   as type-reference text.
    ///
    /// Transformation:
    /// - Exercises all callable parser paths that share the canonical
    ///   `[TraitRef]` constraint-list syntax.

    /// Verifies canonical constraint lists on non-function callable forms.
    ///
    /// Inputs:
    /// - A module containing a trait method, receiver method, and explicit impl
    ///   method with post-parameter constraint lists.
    ///
    /// Output:
    /// - Parsed declarations whose `generic_bounds` preserve each constraint
    ///   as type-reference text.
    ///
    /// Transformation:
    /// - Exercises all callable parser paths that share the canonical
    ///   `[TraitRef]` constraint-list syntax.
    #[test]
    fn parses_method_trait_method_and_impl_method_constraint_lists() {
        let source = r#"
module bounds_surfaces.

struct User {
    name: String
}.

pub trait Show[T] {
    show[A](value: A)[Eq[A]]: String.
}.

pub (user: User) label[A](value: A)[Show[A]]: String ->
    Show.show(value).

pub impl Show[User] for User {
    show[A](value: A)[Eq[A]]: String ->
        "user".
}.
"#;

        let module = parse_module(source).expect("parse constraint-list surfaces");

        let trait_decl = match &module.declarations[1] {
            Decl::Trait(trait_decl) => trait_decl,
            _ => panic!("expected trait declaration"),
        };
        assert_eq!(
            trait_decl.methods[0].generic_bounds,
            vec!["Eq[A]".to_string()]
        );

        let method_decl = match &module.declarations[2] {
            Decl::Method(method_decl) => method_decl,
            _ => panic!("expected method declaration"),
        };
        assert_eq!(method_decl.generic_bounds, vec!["Show[A]".to_string()]);

        let impl_decl = match &module.declarations[3] {
            Decl::TraitImpl(impl_decl) => impl_decl,
            _ => panic!("expected trait impl declaration"),
        };
        assert_eq!(
            impl_decl.methods[0].generic_bounds,
            vec!["Eq[A]".to_string()]
        );
    }

    #[test]
    fn parses_module_and_item_doc_comments() {
        let source = r#"
//! Math helpers.
//! Second module line.

module mathx.

/// Adds one.
/// Second function line.
pub add(X: Int): Int ->
    X + 1.

/// Optional value.
pub type Option[T] =
      none
    | {some, T}.
"#;

        let module = parse_module(source).expect("parse docs");
        assert_eq!(module.docs, vec!["Math helpers.", "Second module line."]);
        match &module.declarations[0] {
            Decl::Function(function) => {
                assert_eq!(function.docs, vec!["Adds one.", "Second function line."]);
            }
            _ => panic!("expected documented function"),
        }
        match &module.declarations[1] {
            Decl::Type(type_decl) => {
                assert_eq!(type_decl.docs, vec!["Optional value."]);
            }
            _ => panic!("expected documented type"),
        }
    }

    #[test]
    fn parses_module_and_item_doc_block_comments() {
        let source = r#"
/**
 * Math helpers.
 *
 * @module mathx
 */
module mathx.

/**
 * Adds one.
 *
 * @param x The value to increment.
 * @returns The incremented value.
 */
@test
pub add(x: Int): Int ->
    x + 1.

/**
 * Optional value.
 *
 * @type T The wrapped value type.
 */
pub type Option[T] =
      none
    | {some, T}.
"#;

        let module = parse_module(source).expect("parse block docs");
        assert_eq!(module.docs, vec!["Math helpers.\n\n@module mathx"]);
        assert_eq!(module.declaration_annotations[0][0].path, vec!["test"]);
        match &module.declarations[0] {
            Decl::Function(function) => {
                assert_eq!(
                    function.docs,
                    vec![
                        "Adds one.\n\n@param x The value to increment.\n@returns The incremented value."
                    ]
                );
            }
            _ => panic!("expected documented function"),
        }
        match &module.declarations[1] {
            Decl::Type(type_decl) => {
                assert_eq!(
                    type_decl.docs,
                    vec!["Optional value.\n\n@type T The wrapped value type."]
                );
            }
            _ => panic!("expected documented type"),
        }
    }

    #[test]
    fn parses_public_constructor_with_varargs_and_defaults() {
        let source = r#"
module queue.

/// Builds queues.
pub constructor Queue[T] {
    (): Queue[T] ->
        empty();

    (Items: List[T]): Queue[T] ->
        from_list(Items);

    (...Items: T): Queue[T] ->
        from_list(Items)
}.

pub constructor Range {
    (Start: Int, End: Int, Step: Int = 1): Range ->
        make(Start, End, Step)
}.
"#;

        let module = parse_module(source).expect("parse constructors");
        match &module.declarations[0] {
            Decl::Constructor(constructor) => {
                assert!(constructor.is_public);
                assert_eq!(constructor.docs, vec!["Builds queues."]);
                assert_eq!(constructor.name, "Queue");
                assert_eq!(constructor.params, vec!["T"]);
                assert_eq!(constructor.clauses.len(), 3);
                assert!(constructor.clauses[2].params[0].is_varargs);
            }
            _ => panic!("expected queue constructor"),
        }
        match &module.declarations[1] {
            Decl::Constructor(constructor) => {
                let step = &constructor.clauses[0].params[2];
                assert_eq!(step.name, "Step");
                assert!(step.default.is_some());
            }
            _ => panic!("expected range constructor"),
        }
    }

    #[test]
    fn rejects_constructor_varargs_before_other_params() {
        let source = r#"
module bad.

pub constructor Queue[T] {
    (...Items: T, Last: T): Queue[T] ->
        from_list(Items)
}.
"#;

        let err = parse_module(source).expect_err("invalid varargs");
        assert_eq!(err.message, "constructor varargs parameter must be last");
    }

    #[test]
    fn rejects_ambiguous_constructor_clause_shapes() {
        let duplicate_exact = r#"
module bad.

pub constructor Pair {
    (A: Int): Pair ->
        make(A);

    (B: Binary): Pair ->
        make(B)
}.
"#;

        let err = parse_module(duplicate_exact).expect_err("ambiguous exact arity");
        assert_eq!(err.message, "constructor has ambiguous arity clauses");

        let overlapping_defaults = r#"
module bad.

pub constructor Range {
    (Start: Int, End: Int = 10): Range ->
        make(Start, End);

    (Start: Int): Range ->
        make(Start, 10)
}.
"#;

        let err = parse_module(overlapping_defaults).expect_err("ambiguous default arity");
        assert_eq!(err.message, "constructor has ambiguous arity clauses");

        let duplicate_varargs = r#"
module bad.

pub constructor Items[T] {
    (...Items: T): Items[T] ->
        Items;

    (First: T, ...Rest: T): Items[T] ->
        Rest
}.
"#;

        let err = parse_module(duplicate_varargs).expect_err("ambiguous varargs");
        assert_eq!(err.message, "constructor has ambiguous varargs clauses");
    }

    #[test]
    fn rejects_misplaced_module_doc_comments() {
        let source = r#"
module misplaced_docs.

//! Late module docs.
pub id(X: Int): Int ->
    X.
"#;

        let err = parse_module(source).expect_err("reject misplaced module docs");
        assert_eq!(
            err.message,
            "module doc comments (`//!`) must appear before the module declaration"
        );

        let interface_source = r#"
module misplaced_interface_docs.

//! Late module docs.
pub id(X: Int): Int.
"#;

        let interface_err =
            parse_interface_module(interface_source).expect_err("reject misplaced interface docs");
        assert_eq!(
            interface_err.message,
            "module doc comments (`//!`) must appear before the module declaration"
        );
    }

    #[test]
    fn rejects_misplaced_module_doc_blocks() {
        let source = r#"
module misplaced_doc_block.

/**
 * Late module docs.
 *
 * @module misplaced_doc_block
 */
pub id(x: Int): Int ->
    x.
"#;

        let err = parse_module(source).expect_err("reject misplaced module doc block");
        assert_eq!(
            err.message,
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        );

        let interface_source = r#"
module misplaced_interface_doc_block.

/**
 * Late module docs.
 *
 * @module misplaced_interface_doc_block
 */
pub id(x: Int): Int.
"#;

        let interface_err = parse_interface_module(interface_source)
            .expect_err("reject misplaced interface doc block");
        assert_eq!(
            interface_err.message,
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        );
    }

    #[test]
    fn parses_struct_field_doc_comments() {
        let source = r#"
module users.

/// A user account.
pub struct User {
    /// Stable internal ID.
    id: Int,

    /// Display name.
    name: Text
}.
"#;

        let module = parse_module(source).expect("parse struct docs");
        match &module.declarations[0] {
            Decl::Struct(struct_decl) => {
                assert_eq!(struct_decl.docs, vec!["A user account."]);
                assert_eq!(struct_decl.fields[0].docs, vec!["Stable internal ID."]);
                assert_eq!(struct_decl.fields[1].docs, vec!["Display name."]);
            }
            _ => panic!("expected documented struct"),
        }
    }

    /// Verifies `#field` syntax in struct declarations.
    ///
    /// Inputs:
    /// - A public struct with one public field and one private field.
    ///
    /// Output:
    /// - Test passes when the parser stores the clean field name and privacy
    ///   flag separately.
    ///
    /// Transformation:
    /// - Parses source-level private field spelling into canonical field
    ///   metadata for later typechecking and interface emission.
    #[test]
    fn parses_private_struct_field_declarations() {
        let source = r#"
module users.

pub struct User {
    id: Int,
    #email: String
}.
"#;

        let module = parse_module(source).expect("parse private struct fields");
        match &module.declarations[0] {
            Decl::Struct(struct_decl) => {
                assert_eq!(struct_decl.fields[0].name, "id");
                assert!(!struct_decl.fields[0].is_private);
                assert_eq!(struct_decl.fields[1].name, "email");
                assert!(struct_decl.fields[1].is_private);
            }
            _ => panic!("expected struct declaration"),
        }
    }

    #[test]
    fn parses_public_macro_declaration() {
        let source = r#"
module mathx.

pub macro unless(X: Expr, Y: Expr): Expr ->
    quote X.
"#;

        let tokens = crate::lexer::lex(source).unwrap();
        for token in tokens {
            println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
        }

        let module = parse_module(source).expect("parse");
        assert_eq!(module.name, "mathx");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Function(function) => assert!(function.is_macro),
            _ => panic!("expected function declaration"),
        }
    }

    #[test]
    fn parses_public_trait_as_decl() {
        let source = r#"
module trait_demo.

/// Show trait docs.
pub trait Show[A] {
    show(Value: A): Text.
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert!(trait_decl.is_public);
                assert_eq!(trait_decl.name, "Show");
                assert_eq!(trait_decl.params[0], "A");
                assert_eq!(trait_decl.docs, vec!["Show trait docs."]);
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_raw_block_declaration_without_trailing_dot() {
        let source = r#"
module native_meta.

target erlang with safe_native.

native core module ArrayNative {
    #[native(normal)]
    length[T](A: Array[T]): Int.
}
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 2);
        match &module.declarations[1] {
            Decl::Raw(raw) => {
                assert_eq!(raw.kind, "native");
                assert!(raw.text.contains("ArrayNative"));
            }
            _ => panic!("expected raw native declaration"),
        }
    }

    #[test]
    fn parses_public_struct_declaration() {
        let source = r#"
module users.

pub struct User {
    id: Int,
    name: Text,
    email: Text = :none
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Struct(struct_decl) => {
                assert!(struct_decl.is_public);
                assert_eq!(struct_decl.name, "User");
                assert_eq!(struct_decl.fields.len(), 3);
                assert_eq!(struct_decl.fields[0].name, "id");
                assert_eq!(struct_decl.fields[1].name, "name");
                assert_eq!(struct_decl.fields[2].name, "email");
                match &struct_decl.fields[2].default {
                    Some(default) => match default {
                        Expr::Atom(atom) => assert_eq!(atom, "none"),
                        _ => panic!("expected atom default expression"),
                    },
                    None => panic!("expected default expression"),
                }
            }
            _ => panic!("expected struct declaration"),
        }
    }

    #[test]
    fn parses_struct_includes_clause() {
        let source = r#"
module users.

pub struct User includes Person, Audited {
    id: Int
}.
"#;

        let module = parse_module(source).expect("parse struct includes");
        match &module.declarations[0] {
            Decl::Struct(struct_decl) => {
                assert_eq!(
                    struct_decl.includes,
                    vec!["Person".to_string(), "Audited".to_string()]
                );
            }
            _ => panic!("expected struct declaration"),
        }
    }

    #[test]
    fn rejects_legacy_struct_derives_clause() {
        let source = r#"
module users.

pub struct User derives Person {
    id: Int
}.
"#;

        let err = parse_module(source).expect_err("reject legacy derives clause");
        assert_eq!(err.message, "expected LBrace");
    }

    #[test]
    fn parses_trait_as_trait_decl() {
        let source = r#"
module traits.

pub trait Show {}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.name, "Show");
                assert!(trait_decl.params.is_empty());
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_trait_decl_extends() {
        let source = r#"
module traits.

pub trait Monoid[A] extends Semigroup[A], Eq[A] {
    combine(X: A, Y: A): A.
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.name, "Monoid");
                assert_eq!(trait_decl.params, vec!["A"]);
                assert_eq!(trait_decl.super_traits, vec!["Semigroup[A]", "Eq[A]"]);
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_function_declaration_with_angle_generic_bounds() {
        let source = r#"
module bounds_demo.

pub debug<A: Eq + Show>(X: A, Y: A): Text ->
    case Eq.equal(X, Y) {
        true -> Show.render(X);
        false -> "neq"
    }.
"#;

        let module = parse_module(source).expect("parse generic bounds function");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function declaration"),
        };
        assert_eq!(function.name, "debug");
        assert_eq!(function.params.len(), 2);
        assert_eq!(function.params[0].annotation.text, "A");
        assert_eq!(function.params[1].annotation.text, "A");
    }

    #[test]
    fn parses_trait_method_with_angle_generic_bounds() {
        let source = r#"
module bounds_trait.

pub trait Logger[A] {
    debug<A: Eq + Show>(Value: A): Text.
}.
"#;

        let module = parse_module(source).expect("parse trait method bounds");
        let trait_decl = match &module.declarations[0] {
            Decl::Trait(trait_decl) => trait_decl,
            _ => panic!("expected trait declaration"),
        };
        let method = &trait_decl.methods[0];
        assert_eq!(method.name, "debug");
        assert_eq!(method.params.len(), 1);
        assert_eq!(method.params[0].annotation.text, "A");
    }
}
