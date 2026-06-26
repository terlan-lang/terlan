#[cfg(test)]
mod tests {
    use crate::parse_tree::Decl;
    use crate::{parse_interface_module, parse_module};

    #[test]
    fn interface_parser_accepts_macros_and_types() {
        let source = r#"
module iface.
pub macro expand(X: Expr, Y: Expr): Expr.
pub type Flag = Bool.
"#;
        let tokens = crate::lexer::lex(source).unwrap();
        for token in tokens {
            println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
        }

        let module = parse_interface_module(source).expect("parse interface");
        assert_eq!(module.declarations.len(), 2);
        assert!(matches!(&module.declarations[0], Decl::Function(_)));
        assert!(matches!(&module.declarations[1], Decl::Type(_)));
    }

    /// Verifies interface files can summarize explicit trait conformance
    /// declarations.
    ///
    /// Inputs:
    /// - A `.terli`-style module containing a trait declaration and
    ///   `pub impl TraitRef for Type` signature block.
    ///
    /// Output:
    /// - Structured trait and trait implementation declarations.
    ///
    /// Transformation:
    /// - Exercises the interface declaration router after `pub` and proves
    ///   conformance summaries preserve signatures without requiring method
    ///   bodies.

    /// Verifies interface files can summarize explicit trait conformance
    /// declarations.
    ///
    /// Inputs:
    /// - A `.terli`-style module containing a trait declaration and
    ///   `pub impl TraitRef for Type` signature block.
    ///
    /// Output:
    /// - Structured trait and trait implementation declarations.
    ///
    /// Transformation:
    /// - Exercises the interface declaration router after `pub` and proves
    ///   conformance summaries preserve signatures without requiring method
    ///   bodies.
    #[test]
    fn interface_parser_preserves_pub_impl_declarations() {
        let source = r#"
module trait_iface.

pub trait Show[A] {
    show(Value: A): Text.
}.

pub impl Show[Int] for Int {
    show(Value: Int): Text.
}.
"#;

        let module = parse_interface_module(source).expect("parse interface impl");
        assert_eq!(module.declarations.len(), 2);
        assert!(matches!(&module.declarations[0], Decl::Trait(_)));
        let Decl::TraitImpl(impl_decl) = &module.declarations[1] else {
            panic!("expected trait impl declaration");
        };
        assert_eq!(impl_decl.trait_ref.text, "Show[Int]");
        assert_eq!(impl_decl.for_type.text, "Int");
        assert_eq!(impl_decl.methods.len(), 1);
        assert!(impl_decl.methods[0].clauses.is_empty());
    }

    /// Verifies interface files may summarize public type headers.
    ///
    /// Inputs:
    /// - A `.terli`-style module containing `pub type ExternalUser.`.
    ///
    /// Output:
    /// - Parsed type declaration with no variants.
    ///
    /// Transformation:
    /// - Exercises interface-only type parsing so generated `.typi` files can
    ///   preserve nominal public types without requiring source-form bodies.

    /// Verifies interface files may summarize public type headers.
    ///
    /// Inputs:
    /// - A `.terli`-style module containing `pub type ExternalUser.`.
    ///
    /// Output:
    /// - Parsed type declaration with no variants.
    ///
    /// Transformation:
    /// - Exercises interface-only type parsing so generated `.typi` files can
    ///   preserve nominal public types without requiring source-form bodies.
    #[test]
    fn interface_parser_accepts_bodyless_public_type_headers() {
        let source = r#"
module provider_iface.

pub type ExternalUser.
"#;

        let module = parse_interface_module(source).expect("parse bodyless interface type");
        assert_eq!(module.declarations.len(), 1);
        let Decl::Type(type_decl) = &module.declarations[0] else {
            panic!("expected type declaration");
        };
        assert_eq!(type_decl.name, "ExternalUser");
        assert!(type_decl.variants.is_empty());
        assert!(type_decl.is_public);
    }

    #[test]
    fn parses_dotted_imports_and_qualified_remote_calls() {
        let source = r#"
module algebra_demo.

import std.Algebra.{Semigroup, Monoid, Sum}.
import std.Collections.List.

pub total(Xs: List[Int]): Int ->
    Sum.value(std.Algebra.combine_all(List.map(Xs, (X) -> Sum(X)))).
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 3);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.module_name, "std.Algebra");
                assert_eq!(import.items.len(), 3);
                assert!(import.is_selected);
            }
            _ => panic!("expected import"),
        }
        match &module.declarations[1] {
            Decl::Import(import) => {
                assert_eq!(import.module_name, "std.Collections");
                assert_eq!(import.items[0].name, "List");
                assert!(!import.is_selected);
            }
            _ => panic!("expected import"),
        }
    }

    #[test]
    fn parses_file_imports() {
        let source = r#"
module templates_demo.

import file "./templates/user_card.terl.html" as UserCard.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.kind, crate::parse_tree::ImportKind::File);
                assert_eq!(
                    import.source_path.as_deref(),
                    Some("./templates/user_card.terl.html")
                );
                assert_eq!(import.items.len(), 1);
                assert_eq!(import.items[0].name, "UserCard");
            }
            _ => panic!("expected file import"),
        }
    }

    #[test]
    fn parses_css_imports() {
        let source = r#"
module styles_demo.

import css "./styles/page.css" as PageCss.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.kind, crate::parse_tree::ImportKind::Css);
                assert_eq!(import.source_path.as_deref(), Some("./styles/page.css"));
                assert_eq!(import.items.len(), 1);
                assert_eq!(import.items[0].name, "PageCss");
            }
            _ => panic!("expected css import"),
        }
    }

    #[test]
    fn parses_markdown_imports() {
        let source = r#"
module posts_demo.

import markdown "./posts/hello.md" as HelloPost.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Import(import) => {
                assert_eq!(import.kind, crate::parse_tree::ImportKind::Markdown);
                assert_eq!(import.source_path.as_deref(), Some("./posts/hello.md"));
                assert_eq!(import.items.len(), 1);
                assert_eq!(import.items[0].name, "HelloPost");
            }
            _ => panic!("expected markdown import"),
        }
    }

    #[test]
    fn parses_static_route_declarations_as_raw_declarations() {
        let source = r#"
module site.

static route "/" ->
    home().
"#;

        let module = parse_module(source).expect("parse static route declaration");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Raw(raw) => {
                assert_eq!(raw.kind, "static");
                assert!(raw.text.contains("route"));
                assert!(raw.text.contains("/"));
                assert!(raw.text.contains("home"));
            }
            _ => panic!("expected raw static route declaration"),
        }
    }

    #[test]
    fn parses_template_declarations() {
        let source = r#"
module template_demo.

template Page from "./templates/page.terl.html" {
    title: Text,
    user: User = default_user()
}.
"#;

        let module = parse_module(source).expect("parse");
        assert_eq!(module.declarations.len(), 1);
        match &module.declarations[0] {
            Decl::Template(template) => {
                assert_eq!(template.name, "Page");
                assert_eq!(template.source_path, "./templates/page.terl.html");
                assert_eq!(template.props.len(), 2);
                assert_eq!(template.props[0].name, "title");
                assert_eq!(template.props[0].annotation.text, "Text");
                assert!(template.props[0].default.is_none());
                assert_eq!(template.props[1].name, "user");
                assert_eq!(template.props[1].annotation.text, "User");
                assert!(template.props[1].default.is_some());
            }
            _ => panic!("expected template declaration"),
        }
    }

    /// Verifies template declaration defaults follow callable trailing rules.
    ///
    /// Inputs:
    /// - A template declaration with a defaulted property followed by a
    ///   required property.
    ///
    /// Output:
    /// - Parse error naming the template default ordering rule.
    ///
    /// Transformation:
    /// - Treats template properties as generated callable parameters so
    ///   omitted/defaulted template props behave predictably.
    #[test]
    fn rejects_required_template_property_after_default_property() {
        let source = r#"
module template_default_bad.

template Page from "./templates/page.terl.html" {
    title: Binary = "Untitled",
    body: Binary
}.
"#;

        let err = parse_module(source).expect_err("required prop after default");
        assert_eq!(err.message, "template default properties must be trailing");
    }

    #[test]
    fn parses_qualified_type_names_in_function_signatures() {
        let source = r#"
module opaque_demo.

pub make(Value: Int): users_opaque_interface.UserId ->
    users_opaque_interface.user_id(Value).

pub declared(Value: users_opaque_interface.UserId): users_opaque_interface.UserId ->
    Value.
"#;

        let module = parse_module(source).expect("parse qualified type names");
        assert_eq!(module.declarations.len(), 2);
        match &module.declarations[0] {
            Decl::Function(function) => {
                assert_eq!(function.return_type.text, "users_opaque_interface.UserId");
            }
            _ => panic!("expected function"),
        }
        match &module.declarations[1] {
            Decl::Function(function) => {
                assert_eq!(
                    function.params[0].annotation.text,
                    "users_opaque_interface.UserId"
                );
                assert_eq!(function.return_type.text, "users_opaque_interface.UserId");
                assert_eq!(function.clauses.len(), 1);
            }
            _ => panic!("expected function signature"),
        }
    }

    #[test]
    fn parses_hkt_type_params_and_variance_surface_syntax() {
        let source = r#"
module hkt_demo.

pub type Kleisli[F[_], -A, B] =
    kleisli(run: (A) -> F[B]).

pub example(K: Kleisli[Result[_, db_error], Text, Int]): Kleisli[DbResult, Text, Int] ->
    K.
"#;

        let module = parse_module(source).expect("parse hkt module");
        match &module.declarations[0] {
            Decl::Type(type_decl) => {
                assert_eq!(type_decl.params.len(), 3);
                assert!(type_decl.params[0].contains("F"));
                assert!(type_decl.params[0].contains("_"));
                assert!(type_decl.params[1].contains("-"));
            }
            _ => panic!("expected type declaration"),
        }
    }

    #[test]
    fn parses_binary_hkt_type_parameter_surface_syntax() {
        let source = r#"
module binary_hkt_demo.

pub trait Bifunctor[F[_, _]] {
    first(value: F[A, B], fn: (A) -> C): F[C, B].
}.
"#;

        let module = parse_module(source).expect("parse binary hkt trait");
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.params, vec!["F[_, _]".to_string()]);
            }
            _ => panic!("expected trait declaration"),
        }
    }

    #[test]
    fn parses_variance_markers_on_higher_kind_slots() {
        let source = r#"
module variance_slot_hkt_demo.

pub trait Producer[F[+_]] {
    value(input: F[A]): F[A].
}.

pub trait Consumer[F[-_]] {
    consume(input: F[A]): Unit.
}.
"#;

        let module = parse_module(source).expect("parse variance slot hkt traits");
        match &module.declarations[0] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.params, vec!["F[+_]".to_string()]);
            }
            _ => panic!("expected producer trait declaration"),
        }
        match &module.declarations[1] {
            Decl::Trait(trait_decl) => {
                assert_eq!(trait_decl.params, vec!["F[-_]".to_string()]);
            }
            _ => panic!("expected consumer trait declaration"),
        }
    }

    #[test]
    fn rejects_hkt_type_parameter_with_concrete_slot() {
        let source = r#"
module invalid_hkt_demo.

pub trait Monad[M[T]] {
    value(input: M[T]): M[T].
}.
"#;

        let error = parse_module(source).expect_err("concrete hkt slots should be rejected");
        assert!(
            error
                .message
                .contains("higher-kinded type parameter slots must be `_`, `+_`, or `-_`"),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn parses_terli_style_interface_with_pub_signatures() {
        let source = r#"
module cache_contract.

pub type Cache = Int.

pub get(Cache: Cache, Key: Binary): Result[Binary, not_found].
pub put(Cache: Cache, Key: Binary, Value: Binary): ok.
"#;

        let module = parse_interface_module(source).expect("parse interface");
        assert_eq!(module.declarations.len(), 3);
        assert!(matches!(
            &module.declarations[0],
            Decl::Struct(_) | Decl::Type(_)
        ));
        assert!(matches!(&module.declarations[1], Decl::Function(_)));
        assert!(matches!(&module.declarations[2], Decl::Function(_)));

        if let Decl::Type(cache_type) = &module.declarations[0] {
            assert!(cache_type.is_public);
            assert_eq!(cache_type.name, "Cache");
        } else {
            panic!("expected type declaration");
        }
    }

    /// Verifies release core collection contracts stay parseable.
    ///
    /// Inputs:
    /// - Release source modules for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    ///
    /// Output:
    /// - Test passes when all three release modules parse as normal source
    ///   modules and keep their canonical module names.
    ///
    /// Transformation:
    /// - Parses release contracts with compiler intrinsic annotations and
    ///   placeholder bodies without typechecking or backend emission, proving
    ///   the P0.3 release source shape remains grammar-stable.
    #[test]
    fn parses_release_core_collection_contracts() {
        let contracts = [
            (
                "std.collections.Map",
                include_str!("../../../std/collections/map.terl"),
            ),
            (
                "std.collections.List",
                include_str!("../../../std/collections/list.terl"),
            ),
            (
                "std.collections.Set",
                include_str!("../../../std/collections/set.terl"),
            ),
        ];

        for (expected_module, source) in contracts {
            let module = parse_module(source).expect("parse release collection contract");
            assert_eq!(module.name, expected_module);
        }
    }

    /// Verifies release iterator/iterable modules stay parseable.
    ///
    /// Inputs:
    /// - Release source modules for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    ///
    /// Output:
    /// - Test passes when both modules parse in source mode and keep their
    ///   canonical module names.
    ///
    /// Transformation:
    /// - Parses release traversal modules without typechecking or backend
    ///   emission, proving P0.4b exposes traversal contracts while allowing
    ///   source-implemented helpers such as `Iterator.each`.
    #[test]
    fn parses_release_traversal_contracts() {
        let contracts = [
            (
                "std.collections.Iterator",
                include_str!("../../../std/collections/iterator.terl"),
            ),
            (
                "std.collections.Iterable",
                include_str!("../../../std/collections/iterable.terl"),
            ),
        ];

        for (expected_module, source) in contracts {
            let module = parse_module(source).expect("parse release collection trait module");
            assert_eq!(module.name, expected_module);
        }
    }

    /// Verifies existential type binders do not terminate declarations early.
    ///
    /// Inputs:
    /// - A type alias using `exists T. Box[T]`.
    ///
    /// Output:
    /// - Parsed type declaration with the full existential body preserved.
    ///
    /// Transformation:
    /// - Distinguishes the existential binder separator `.` from the final
    ///   declaration terminator `.` while preserving type text for typecheck.
    #[test]
    fn parses_existential_type_alias_body() {
        let module = parse_module(
            r#"
module existential_syntax.

pub type Box[T] = {value: T}.
pub type AnyBox = exists T. Box[T].
"#,
        )
        .expect("parse existential alias");

        let Decl::Type(any_box) = &module.declarations[1] else {
            panic!("expected type declaration");
        };
        assert_eq!(any_box.name, "AnyBox");
        assert_eq!(any_box.variants[0].text, "exists T.Box[T]");
    }
}
