use crate::terlan_syntax::parse_module;
use crate::terlan_syntax::parse_tree::{
    AnnotationKeyOption, AnnotationSchemaEntry, AnnotationValue, Decl,
};

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
pub(super) fn formal_declaration_inventory_covers_parser_decl_classes() {
    let module = parse_module(
        r#"
        module declaration.inventory.

        import std.core.String.
        import type std.core.Option.
        import std.core.Option.{map as map_option, Option as MaybeOption}.
        import markdown "./readme.md" as readme.

        pub type Alias[T] = {Atom["ok"], value: T} | Atom["none"].
        pub opaque type Secret = Int.

        pub struct User {
          id: Int,
          name: String = ""
        }.

        pub constructor User {
          (id: Int, name: String): User -> {
            id: id,
            name: name
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
            Decl::Constant(_) => "constant_decl",
            Decl::ConstFunction(_) => "const_function_decl",
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
            Decl::Shape(_) => "shape_decl",
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
pub(super) fn module_paths_accept_template_package_segment() {
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
///
/// Output:
/// - Test passes when the import preserves `*` as the selected import item.
///
/// Transformation:
/// - Exercises wildcard import parsing without expanding symbols; semantic
///   expansion belongs to HIR/typecheck once provider interfaces are loaded.
#[test]
pub(super) fn parses_wildcard_imports() {
    let module = parse_module(
        r#"
        module app.Wildcard.

        import test.Other.{*}.

        pub main(): Int -> 1.
        "#,
    )
    .expect("parse wildcard imports");

    let Decl::Import(braced_import) = &module.declarations[0] else {
        panic!("expected braced wildcard import");
    };
    assert_eq!(braced_import.module_name, "test.Other");
    assert_eq!(braced_import.items[0].name, "*");
}

/// Verifies path-style wildcard imports are rejected.
///
/// Inputs:
/// - Legacy path-style wildcard import syntax.
///
/// Output:
/// - Test passes when the parser reports the braced-selector requirement.
///
/// Transformation:
/// - Keeps wildcard syntax visually distinct from declaration terminators.
#[test]
pub(super) fn rejects_path_style_wildcard_imports() {
    let err = parse_module(
        r#"
        module app.Wildcard.

        import test.Legacy.*.

        pub main(): Int -> 1.
        "#,
    )
    .expect_err("path-style wildcard imports must be rejected");

    assert!(err
        .message
        .contains("wildcard imports must use braced selector syntax"));
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
pub(super) fn parses_annotation_schema_declarations() {
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
pub(super) fn formal_type_family_inventory_preserves_type_expr_text() {
    let module = parse_module(
        r#"
        module types.family.inventory.

        pub type Maybe[T] = Atom["none"] | {Atom["some"], value: T}.
        type Pair = {left: Int, right: String}.
        type IgnoredField = {_: Int, value: String}.
        type Lookup[K, V] = {key: K, value: V}.
        type Mapper[A, B] = (A) -> B.
        type Nested = std.core.Option[String].
        type Names = [String].
        type LiteralUnion = Atom["empty"] | Atom["Interop.Empty"] | 0 | 1.5 | "ready".

        pub opaque type Secret[T] = {value: T}.
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
pub(super) fn formal_type_family_rejects_runtime_expression_tokens() {
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
pub(super) fn formal_method_receiver_inventory_preserves_validated_methods() {
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
pub(super) fn formal_method_receiver_inventory_preserves_mutable_receiver_marker() {
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
pub(super) fn formal_method_receiver_diagnostics_reject_invalid_method_heads() {
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
pub(super) fn formal_annotation_subjects_are_rejected_before_declaration_routing() {
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
pub(super) fn formal_declaration_annotation_before_function_still_parses() {
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
