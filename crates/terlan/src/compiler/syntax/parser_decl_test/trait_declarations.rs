use crate::terlan_syntax::parse_tree::Decl;
use crate::terlan_syntax::{parse_interface_module, parse_module};

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

    assert!(matches!(&module.declarations[2], Decl::Method(method) if method.name == "to_string"));
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
    assert!(!external_impl.is_negative);
    assert!(external_impl.is_public);
    assert_eq!(external_impl.trait_ref.text, "Show[ExternalUser]");
    assert_eq!(external_impl.for_type.text, "ExternalUser");
    assert_eq!(external_impl.methods.len(), 1);
    assert_eq!(external_impl.methods[0].name, "to_string");
    assert_eq!(external_impl.methods[0].clauses.len(), 1);
}

/// Verifies the older Contract descriptor impl shape fails clearly.
///
/// Inputs:
/// - A `Contract[T] => { ... }` descriptor impl using the earlier planned
///   0.0.7 surface.
///
/// Output:
/// - Stable parser diagnostic describing the reserved descriptor form.
///
/// Transformation:
/// - Prevents the parser from reporting a vague missing `for` token while
///   contract descriptors are still outside the implemented AST/typechecker
///   model.
#[test]
fn rejects_reserved_contract_descriptor_impl_syntax() {
    let source = r#"
module contract_descriptor_reserved.

pub opaque type SecretKey.

pub impl Contract[SecretKey] =>
    {
        allow = [Drop],
        deny = [JsonEncode]
    }.
"#;

    let err = parse_module(source).expect_err("contract descriptor should be reserved");
    assert_eq!(
        err.message,
        "Contract impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for now"
    );
}

/// Verifies the simplified expression-bodied Contract impl shape fails
/// clearly until the contract AST and typechecker model land.
///
/// Inputs:
/// - A `Contract[T] -> Contract.secret` impl using the simplified planned
///   0.0.7 surface.
///
/// Output:
/// - Stable parser diagnostic describing the reserved contract impl form.
///
/// Transformation:
/// - Prevents the parser from treating expression-bodied contract impls as
///   malformed ordinary trait impl blocks while the contract model is
///   still pending.
#[test]
fn rejects_reserved_contract_expression_impl_syntax() {
    let source = r#"
module contract_expression_reserved.

pub opaque type SecretKey.

pub impl Contract[SecretKey] ->
    Contract.secret.
"#;

    let err = parse_module(source).expect_err("contract expression impl should be reserved");
    assert_eq!(
        err.message,
        "Contract impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for now"
    );
}

/// Verifies local negative trait impl declarations have structured AST.
///
/// Inputs:
/// - A local `impl not Trait[Type].` declaration using the planned 0.0.7
///   adversarial-type surface.
///
/// Output:
/// - A bodyless negative `TraitImplDecl` with separate trait and target.
///
/// Transformation:
/// - Preserves negative polarity for later coherence validation without
///   introducing a parallel declaration family.
#[test]
fn parses_negative_trait_impl_declaration() {
    let source = r#"
module negative_trait_impl_reserved.

pub opaque type SecretKey.

impl not JsonEncode[SecretKey].
	"#;

    let module = parse_module(source).expect("parse local negative impl");
    let Decl::TraitImpl(impl_decl) = &module.declarations[1] else {
        panic!("expected structured negative trait impl");
    };
    assert!(impl_decl.is_negative);
    assert!(!impl_decl.is_public);
    assert_eq!(impl_decl.trait_ref.text, "JsonEncode");
    assert_eq!(impl_decl.for_type.text, "SecretKey");
    assert!(impl_decl.methods.is_empty());
}

/// Verifies public negative trait impl declarations preserve visibility.
///
/// Inputs:
/// - A public `pub impl not Trait[Type].` declaration.
///
/// Output:
/// - A public bodyless negative `TraitImplDecl`.
///
/// Transformation:
/// - Keeps exported negative coherence metadata explicit in the parser AST.
#[test]
fn parses_public_negative_trait_impl_declaration() {
    let source = r#"
module negative_trait_impl_public_reserved.

pub opaque type SecretKey.

pub impl not JsonEncode[SecretKey].
	"#;

    let module = parse_module(source).expect("parse public negative impl");
    let Decl::TraitImpl(impl_decl) = &module.declarations[1] else {
        panic!("expected structured public negative trait impl");
    };
    assert!(impl_decl.is_negative);
    assert!(impl_decl.is_public);
    assert_eq!(impl_decl.trait_ref.text, "JsonEncode");
    assert_eq!(impl_decl.for_type.text, "SecretKey");
}

/// Verifies generic negative trait impl targets remain one target type.
///
/// Inputs:
/// - A public `pub impl not Trait[Wrapper[Type]].` declaration.
///
/// Output:
/// - A negative `TraitImplDecl` whose target retains nested type arguments.
///
/// Transformation:
/// - Uses delimiter-aware type parsing so nested brackets do not truncate
///   the target type.
#[test]
fn parses_generic_negative_trait_impl_target() {
    let source = r#"
module negative_trait_impl_generic_reserved.

pub opaque type SecretKey.

pub type Box[T] = {value: T}.

pub impl not JsonEncode[Box[SecretKey]].
	"#;

    let module = parse_module(source).expect("parse generic negative impl target");
    let Decl::TraitImpl(impl_decl) = &module.declarations[2] else {
        panic!("expected structured generic negative trait impl");
    };
    assert!(impl_decl.is_negative);
    assert_eq!(impl_decl.trait_ref.text, "JsonEncode");
    assert_eq!(impl_decl.for_type.text, "Box[SecretKey]");
}

/// Verifies negative trait impl declarations with bodies fail with the
/// a stable body-specific diagnostic.
///
/// Inputs:
/// - A public `pub impl not Trait[Type] { ... }.` declaration attempting
///   to attach an adapter body to a negative impl.
///
/// Output:
/// - Stable parser diagnostic rejecting the body.
///
/// Transformation:
/// - Enforces that negative impls are compile-time facts, never adapters.
#[test]
fn rejects_negative_trait_impl_with_body_declaration() {
    let source = r#"
module negative_trait_impl_body_reserved.

pub opaque type SecretKey.

pub impl not JsonEncode[SecretKey] {
    encode(value: SecretKey): String ->
        "secret".
}.
	"#;

    let err = parse_module(source).expect_err("body-bearing negative impl should fail");
    assert_eq!(
        err.message,
        "negative trait impl declarations cannot have a body"
    );
}

/// Verifies interface summaries preserve negative trait impl declarations.
///
/// Inputs:
/// - An interface module containing `impl not Trait[Type].`.
///
/// Output:
/// - Structured negative impl metadata matching source parsing.
///
/// Transformation:
/// - Keeps source and interface parsing aligned for exported coherence facts.
#[test]
fn parses_interface_negative_trait_impl_declaration() {
    let source = r#"
module negative_trait_impl_interface_reserved.

pub opaque type SecretKey.

impl not JsonEncode[SecretKey].
	"#;

    let module = parse_interface_module(source).expect("parse interface negative impl");
    let Decl::TraitImpl(impl_decl) = &module.declarations[1] else {
        panic!("expected structured interface negative trait impl");
    };
    assert!(impl_decl.is_negative);
    assert_eq!(impl_decl.trait_ref.text, "JsonEncode");
    assert_eq!(impl_decl.for_type.text, "SecretKey");
}

/// Verifies `policy` remains an ordinary function name, not a declaration
/// keyword.
///
/// Inputs:
/// - A valid function declaration whose name is the lower-case atom
///   `policy`.
///
/// Output:
/// - Test passes when the module parses with a normal function declaration.
///
/// Transformation:
/// - Proves the negative-impl pivot does not reserve `policy`.
#[test]
fn parses_function_named_policy() {
    let source = r#"
module policy_function_allowed.

policy(): Int ->
    1.
"#;

    let module = parse_module(source).expect("function named policy should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected policy function declaration");
    };
    assert_eq!(function.name, "policy");
}

/// Verifies old policy arrow syntax is not treated as the planned negative
/// impl form.
///
/// Inputs:
/// - A `policy Type => ...` declaration from the discarded design.
///
/// Output:
/// - Test passes when the parser does not reserve the discarded policy
///   descriptor form.
///
/// Transformation:
/// - Guards the decision that adversarial types use negative trait impls,
///   not descriptor arrows.
#[test]
fn rejects_policy_arrow_shape_as_not_negative_impl_declaration() {
    let source = r#"
module policy_arrow_rejected.

pub opaque type SecretKey.

policy SecretKey =>
    {
        allow = [Drop],
        deny = [JsonEncode]
    }.
"#;

    let err = parse_module(source).expect_err("arrow policy should not parse");
    assert_ne!(
            err.message,
            "negative trait impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for positive behavior for now"
        );
}

/// Verifies old policy syntax is not treated as the planned negative impl
/// form.
///
/// Inputs:
/// - A `policy Type with [...]` declaration from the discarded design.
///
/// Output:
/// - Test passes when the parser does not reserve the discarded policy form.
///
/// Transformation:
/// - Guards the decision that adversarial types use negative trait impls,
///   not a separate policy declaration.
#[test]
fn rejects_policy_with_shape_as_not_negative_impl_declaration() {
    let source = r#"
module policy_with_rejected.

pub opaque type SecretKey.

policy SecretKey
    with [Drop].
"#;

    let err = parse_module(source).expect_err("with policy should not parse");
    assert_ne!(
            err.message,
            "negative trait impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for positive behavior for now"
        );
}

/// Verifies implication constraints are rejected on struct fields.
///
/// Inputs:
/// - A struct field that attempts to decorate its type with `=>`.
///
/// Output:
/// - Stable parser diagnostic pointing users to struct-level `where`
///   constraints instead.
///
/// Transformation:
/// - Reserves the implication arrow for compile-time constraint lists and
///   prevents field declarations from becoming a second implication
///   surface.
#[test]
fn rejects_implication_arrow_on_struct_field_declaration() {
    let source = r#"
module field_implication_rejected.

pub struct Page {
    model: T => {title: String}
}.
"#;

    let err = parse_module(source).expect_err("field implication should be rejected");
    assert!(
        err.message
            .contains("implication constraints are not valid on struct fields")
            && err.message.contains("owning generic parameter list"),
        "unexpected diagnostic: {:?}",
        err
    );
}

/// Verifies implication constraints are rejected in ordinary type aliases.
///
/// Inputs:
/// - A type alias whose body attempts to use `=>` as a type relation.
///
/// Output:
/// - Stable parser diagnostic pointing to owning generic parameter lists.
///
/// Transformation:
/// - Keeps implication syntax out of arbitrary type-expression text until
///   the formal generic-parameter implication production lands.
#[test]
fn rejects_implication_arrow_in_ordinary_type_alias_body() {
    let source = r#"
module type_alias_implication_rejected.

pub type Projected = User => {name: String}.
"#;

    let err = parse_module(source).expect_err("type alias implication should be rejected");
    assert!(
        err.message
            .contains("implication constraints are only valid in generic parameter constraints"),
        "unexpected diagnostic: {:?}",
        err
    );
}

/// Verifies type aliases may own implication-constrained generic parameters.
#[test]
fn parses_structural_implication_in_type_alias_generic_parameter() {
    let source = r#"
module generic_alias_implication.

pub type Named[T => {name: String}] = T.
"#;

    let module = parse_module(source).expect("parse generic alias implication");
    let alias = match &module.declarations[0] {
        Decl::Type(alias) => alias,
        _ => panic!("expected type alias declaration"),
    };
    assert_eq!(alias.params, vec!["T => {name: String}".to_string()]);
    assert_eq!(alias.variants[0].text, "T");
}

/// Verifies positive structural implications parse only as generic
/// parameter constraints.
#[test]
fn parses_structural_implication_in_function_generic_parameter() {
    let source = r#"
module generic_implication.

pub display_name[T => {name: String, profile: {title: String}}](value: T): String ->
    value.name.
"#;

    let module = parse_module(source).expect("parse structural implication parameter");
    let function = match &module.declarations[0] {
        Decl::Function(function) => function,
        _ => panic!("expected function declaration"),
    };
    assert_eq!(
        function.generic_params,
        vec!["T => {name: String, profile: {title: String}}"]
    );
}

/// Verifies structs use the same implication-constrained generic parameter
/// syntax as callables instead of introducing a declaration-specific form.
#[test]
fn parses_structural_implication_in_struct_generic_parameter() {
    let source = r#"
module generic_struct_implication.

pub struct Page[T => {title: String}] {
    model: T
}.
"#;

    let module = parse_module(source).expect("parse generic struct implication");
    let struct_decl = match &module.declarations[0] {
        Decl::Struct(struct_decl) => struct_decl,
        _ => panic!("expected struct declaration"),
    };
    assert_eq!(struct_decl.generic_params, vec!["T => {title: String}"]);
    assert_eq!(struct_decl.fields[0].annotation.text, "T");
}

/// Verifies implication targets cannot silently widen to dynamic evidence.
#[test]
fn rejects_non_structural_generic_implication_target() {
    let source = r#"
module generic_implication_dynamic.

pub identity[T => Dynamic](value: T): T -> value.
"#;

    let error = parse_module(source).expect_err("dynamic implication target should fail");
    assert_eq!(
        error.message,
        "implication target must be a closed structural field shape"
    );
}

/// Verifies a structural implication cannot claim evidence from no fields.
#[test]
fn rejects_empty_structural_generic_implication_target() {
    let source = r#"
module generic_implication_empty.

pub identity[T => {}](value: T): T -> value.
"#;

    let error = parse_module(source).expect_err("empty implication target should fail");
    assert_eq!(
        error.message,
        "implication target must contain at least one field"
    );
}

/// Verifies the shape-synonym declaration surface is parse-preserved.
///
/// Inputs:
/// - A local `shape Name(...) = ...` declaration using the planned 0.0.7
///   surface.
///
/// Output:
/// - A structured shape declaration preserving the future expansion
///   surface.
///
/// Transformation:
/// - Prevents the parser from treating `shape` as an ordinary function
///   name while keeping later typechecking responsible for rejecting the
///   unsupported expansion phase.
#[test]
fn parses_shape_synonym_declaration_as_structured_decl() {
    let source = r#"
module shape_synonym_raw.

shape OkResponse(body) =
    {status, body} where status in 200..299.
"#;

    let module = parse_module(source).expect("shape synonym should parse as structured");
    let [Decl::Shape(shape)] = module.declarations.as_slice() else {
        panic!("expected one shape declaration");
    };
    assert_eq!(shape.name, "OkResponse");
    assert_eq!(shape.params, ["body"]);
    assert_eq!(shape.body, "{ status , body }");
    assert_eq!(shape.guard.as_deref(), Some("status in 200 .. 299"));
    assert!(!shape.is_public);
    assert!(shape.text.starts_with("shape OkResponse"));
}

/// Verifies exported shape-synonym declarations are parsed with public
/// visibility retained in structured metadata and preserved text.
///
/// Inputs:
/// - A public `pub shape Name(...) = ...` declaration.
///
/// Output:
/// - A structured shape declaration whose text starts with `pub shape`.
///
/// Transformation:
/// - Locks the public declaration boundary for exported matching APIs
///   without enabling semantic shape expansion yet.
#[test]
fn parses_public_shape_synonym_declaration_as_structured_decl() {
    let source = r#"
module shape_synonym_public_raw.

pub shape Route(method, path, id) =
    {method, path, id}.
"#;

    let module = parse_module(source).expect("public shape synonym should parse as structured");
    let [Decl::Shape(shape)] = module.declarations.as_slice() else {
        panic!("expected one public shape declaration");
    };
    assert_eq!(shape.name, "Route");
    assert_eq!(shape.params, ["method", "path", "id"]);
    assert_eq!(shape.body, "{ method , path , id }");
    assert!(shape.guard.is_none());
    assert!(shape.is_public);
    assert!(shape.text.starts_with("pub shape Route"));
}

/// Verifies shape declarations preserve string-capture pattern bodies.
///
/// Inputs:
/// - A public shape synonym whose raw body contains `${...}` captures.
///
/// Output:
/// - Structured shape metadata preserving parameters, body, guard, and text.
///
/// Transformation:
/// - Locks the parser-level shape-backed string-pattern fixture before
///   semantic shape expansion is enabled.
#[test]
fn parses_shape_synonym_with_string_capture_body() {
    let source = r#"
module shape_synonym_string_capture_raw.

pub shape UserAsset(id, file) =
    "users/${id: Int}/assets/${file}" where id > 0.
"#;

    let module = parse_module(source).expect("shape synonym string capture should parse");
    let [Decl::Shape(shape)] = module.declarations.as_slice() else {
        panic!("expected one public shape declaration");
    };
    assert_eq!(shape.name, "UserAsset");
    assert_eq!(shape.params, ["id", "file"]);
    assert_eq!(shape.body, "\"users/${id: Int}/assets/${file}\"");
    assert_eq!(shape.guard.as_deref(), Some("id > 0"));
    assert!(shape.is_public);
    assert!(shape.text.contains("\"users/${id: Int}/assets/${file}\""));
}

/// Verifies malformed lower-case shape-synonym declarations fail before
/// structured preservation.
///
/// Inputs:
/// - A future-shaped `shape name(...) = ...` declaration whose name is not
///   a valid exported shape name.
///
/// Output:
/// - Stable parser diagnostic requiring upper-case shape names.
///
/// Transformation:
/// - Prevents a near-miss shape declaration from entering the raw
///   expansion pipeline with an invalid exported shape name.
#[test]
fn rejects_lowercase_shape_synonym_declaration() {
    let source = r#"
module shape_synonym_lowercase_rejected.

shape ok_response(body) =
    body.
"#;

    let err = parse_module(source).expect_err("lowercase shape synonym should be rejected");
    assert_eq!(err.message, "shape synonym names must be upper-case");
}

/// Verifies shape-synonym declarations are parse-preserved in interface
/// summaries like source modules.
///
/// Inputs:
/// - An interface module containing the future `shape Name(...) = ...`
///   declaration surface.
///
/// Output:
/// - A structured shape declaration in the parsed interface module.
///
/// Transformation:
/// - Keeps source and interface parsing aligned so exported shape APIs
///   can be represented before expansion support lands.
#[test]
fn parses_interface_shape_synonym_declaration_as_structured_decl() {
    let source = r#"
module shape_synonym_interface_raw.

shape OkResponse(body) =
    body.
"#;

    let module =
        parse_interface_module(source).expect("interface shape synonym should parse as structured");
    let [Decl::Shape(shape)] = module.declarations.as_slice() else {
        panic!("expected one interface shape declaration");
    };
    assert_eq!(shape.name, "OkResponse");
    assert_eq!(shape.params, ["body"]);
    assert_eq!(shape.body, "body");
}

/// Verifies the contextual `shape` reservation does not take away ordinary
/// current function declarations named `shape`.
///
/// Inputs:
/// - A valid function declaration whose name is the lower-case atom
///   `shape`.
///
/// Output:
/// - Parsed module containing one function named `shape`.
///
/// Transformation:
/// - Proves the reserved-feature guard is narrow enough to protect the
///   future syntax without broad keyword pollution.
#[test]
fn parses_ordinary_function_named_shape() {
    let source = r#"
module shape_function_name.

shape(value: Int): Int ->
    value.
"#;

    let module = parse_module(source).expect("ordinary function named shape should parse");
    let [Decl::Function(function)] = module.declarations.as_slice() else {
        panic!("expected one function declaration");
    };
    assert_eq!(function.name, "shape");
}
