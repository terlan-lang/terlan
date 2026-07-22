use super::lower_syntax_module_output_to_core;
use super::test_support::*;

/// Verifies impl implications retain source metadata while semantic trait
/// arguments remain ordinary type variables.
#[test]
fn syntax_output_preserves_generic_trait_impl_structural_implication() {
    let output = crate::terlan_syntax::parse_module_as_syntax_output(
        r#"
module implication_generic_impl_output.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.
"#,
    )
    .expect("generic trait impl syntax output");

    let impl_decl = output
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            crate::terlan_syntax::SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                generic_params,
                ..
            } => Some((trait_ref, generic_params)),
            _ => None,
        })
        .expect("generic trait impl declaration");

    assert_eq!(impl_decl.0.text, "Render[T]");
    assert_eq!(impl_decl.1, &["T => {title: String}".to_string()]);
}

/// Verifies generic impl evidence permits field projection in the method body
/// and dispatches for a matching concrete structure.
#[test]
fn syntax_output_accepts_generic_trait_impl_structural_implication() {
    let source = r#"
module implication_generic_impl.

pub struct Profile {
    title: String
}.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.

pub display(profile: Profile): String -> Render.render(profile).
"#;
    let diagnostics = check_syntax_output(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");

    let module = crate::terlan_syntax::parse_module_as_syntax_output(source)
        .expect("generic structural impl syntax output");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    assert!(
        core.contract_text()
            .contains("core=Call(__terlan_structural_impl_Render_render_1;Var(profile))"),
        "CoreIR must erase structural trait dispatch:\n{}",
        core.contract_text()
    );
}

/// Verifies generic impl dispatch fails closed when the concrete argument does
/// not prove the implementation's structural implication.
#[test]
fn syntax_output_rejects_unproven_generic_trait_impl_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_generic_impl_rejected.

pub struct Account {
    id: Int
}.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.

pub display(account: Account): String -> Render.render(account).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "at `Render.render` call site: no impl for trait method Render.render with provided arguments [Account]"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies public generic impl evidence survives interface generation and is
/// available to trait dispatch in an importing module.
#[test]
fn syntax_output_accepts_imported_generic_trait_impl_structural_implication() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_generic_impl_consumer.

import implication_generic_impl_provider.{Profile, Render}.

pub display(profile: Profile): String -> Render.render(profile).
"#,
        r#"
module implication_generic_impl_provider.

pub struct Profile {
    title: String
}.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies a generic struct validates its inferred type argument against its
/// structural implication and preserves that argument through field access.
#[test]
fn syntax_output_accepts_generic_struct_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_generic_struct.

pub struct User {
    title: String
}.

pub struct Page[T => {title: String}] {
    model: T
}.

pub render(user: User): String ->
    let page = Page {model: user};
    page.model.title.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies inferred generic struct arguments fail closed when their concrete
/// shape does not satisfy the declaration implication.
#[test]
fn syntax_output_rejects_unproven_generic_struct_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_generic_struct_rejected.

pub struct Account {
    id: Int
}.

pub struct Page[T => {title: String}] {
    model: T
}.

pub render(account: Account): Page[Account] ->
    Page {model: account}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Page` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies a concrete alias argument satisfies its retained implication.
#[test]
fn syntax_output_accepts_proven_type_alias_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias.

pub struct Profile {
    title: String
}.

pub type Titled[T => {title: String}] = T.

pub render(value: Titled[Profile]): String -> value.title.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies callable evidence may satisfy a constrained alias application.
#[test]
fn syntax_output_accepts_forwarded_type_alias_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_forwarded.

pub type Titled[T => {title: String}] = T.

pub identity[T => {title: String}](value: Titled[T]): T -> value.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies an imported constrained alias rejects an incompatible argument.
#[test]
fn syntax_output_rejects_unproven_imported_type_alias_structural_implication() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_type_alias_consumer_rejected.

import implication_type_alias_provider.{Titled}.

pub struct Account {
    id: Int
}.

pub render(value: Titled[Account]): String -> "invalid".
"#,
        r#"
module implication_type_alias_provider.

pub type Titled[T => {title: String}] = T.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies concrete constrained aliases are valid in struct fields.
#[test]
fn syntax_output_accepts_proven_type_alias_in_struct_field() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_struct_field.

pub struct Profile {
    title: String
}.

pub type Titled[T => {title: String}] = T.

pub struct Card {
    profile: Titled[Profile]
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies a struct may forward its structural evidence into a field alias.
#[test]
fn syntax_output_accepts_forwarded_type_alias_in_generic_struct_field() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_generic_struct_field.

pub type Titled[T => {title: String}] = T.

pub struct Card[T => {title: String}] {
    profile: Titled[T]
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies struct field aliases reject incompatible concrete arguments.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_struct_field() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_struct_field_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub struct Card {
    account: Titled[Account]
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies an alias body cannot bypass another alias's implication.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_alias_body() {
    let diagnostics = check_syntax_output(
        r#"
module implication_nested_type_alias_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.
pub type AccountTitle = Titled[Account].
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies constructor signatures accept aliases with proven arguments.
#[test]
fn syntax_output_accepts_proven_type_alias_in_constructor_signature() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_constructor.

pub struct Profile {
    title: String
}.

pub type Titled[T => {title: String}] = T.

pub constructor Card {
    (profile: Titled[Profile]): Dynamic -> profile
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies constructor signatures reject aliases with incompatible arguments.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_constructor_signature() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_constructor_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub constructor Card {
    (account: Titled[Account]): Dynamic -> account
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies constructor returns cannot hide incompatible alias arguments.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_constructor_return() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_constructor_return_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub constructor Card {
    (): Titled[Account] -> Account {id: 1}
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies template props accept aliases with proven arguments.
#[test]
fn syntax_output_accepts_proven_type_alias_in_template_prop() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_template.

pub struct Profile {
    title: String
}.

pub type Titled[T => {title: String}] = T.

template Card from "./card.terl.html" {
    profile: Titled[Profile]
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies template props reject aliases with incompatible arguments.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_template_prop() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_template_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

template Card from "./card.terl.html" {
    account: Titled[Account]
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies trait method signatures accept aliases with proven arguments.
#[test]
fn syntax_output_accepts_proven_type_alias_in_trait_signature() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_trait.

pub struct Profile {
    title: String
}.

pub type Titled[T => {title: String}] = T.

pub trait Presenter {
    present(value: Titled[Profile]): Titled[Profile].
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies trait parameters cannot hide incompatible alias arguments.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_trait_parameter() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_trait_parameter_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub trait Presenter {
    present(value: Titled[Account]): String.
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies trait returns cannot hide incompatible alias arguments.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_trait_return() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_trait_return_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub trait Presenter {
    present(): Titled[Account].
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies trait methods may forward method-local structural evidence.
#[test]
fn syntax_output_accepts_forwarded_type_alias_in_generic_trait_method() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_trait_generic.

pub type Titled[T => {title: String}] = T.

pub trait Presenter {
    present[T => {title: String}](value: Titled[T]): Titled[T].
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies unconstrained trait method generics cannot satisfy alias evidence.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_generic_trait_method() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_trait_generic_rejected.

pub type Titled[T => {title: String}] = T.

pub trait Presenter {
    present[T](value: Titled[T]): String.
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires T0 => #{title: Binary}"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies an explicit impl may specialize a trait parameter through a
/// constrained alias whose concrete argument proves the required shape.
#[test]
fn syntax_output_accepts_proven_type_alias_in_explicit_impl_signature() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_explicit_impl.

pub struct Profile {
    title: String
}.

pub type Titled[T => {title: String}] = T.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[Profile] for Profile {
    render(value: Titled[Profile]): String -> value.title.
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies explicit impl parameters cannot erase an incompatible constrained
/// alias while specializing the implemented trait signature.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_explicit_impl_parameter() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_explicit_impl_parameter_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[Account] for Account {
    render(value: Titled[Account]): String -> "invalid".
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies explicit impl returns cannot erase an incompatible constrained
/// alias while specializing the implemented trait signature.
#[test]
fn syntax_output_rejects_unproven_type_alias_in_explicit_impl_return() {
    let diagnostics = check_syntax_output(
        r#"
module implication_type_alias_explicit_impl_return_rejected.

pub struct Account {
    id: Int
}.

pub type Titled[T => {title: String}] = T.

pub trait Render[T] {
    render(value: T): T.
}.

pub impl Render[Account] for Account {
    render(value: Account): Titled[Account] -> value.
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "unproven_implication: `Titled` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies structural implication evidence permits typed generic field access.
#[test]
fn syntax_output_accepts_proven_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_proven.

pub struct User {
    name: String,
    age: Int
}.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render(user: User): String -> display_name(user).
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies nested structural evidence is checked recursively.
#[test]
fn syntax_output_accepts_nested_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_nested.

pub struct Profile {
    title: String
}.

pub struct User {
    profile: Profile
}.

pub title[T => {profile: {title: String}}](value: T): String ->
    value.profile.title.

pub render(user: User): String -> title(user).
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies callers can forward structural evidence when they prove a
/// stronger shape than the callee requires.
#[test]
fn syntax_output_accepts_forwarded_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_forwarded.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render[T => {name: String, age: Int}](value: T): String ->
    display_name(value).
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies receiver methods consume structural evidence for their generic
/// receiver type and validate it at method call sites.
#[test]
fn syntax_output_accepts_receiver_method_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_receiver_method.

pub struct Presenter {
    prefix: String
}.

pub struct User {
    name: String
}.

pub (presenter: Presenter) display_name[T => {name: String}](value: T): String ->
    value.name.

pub render(presenter: Presenter, user: User): String ->
    presenter.display_name(user).
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies receiver-method implications reject concrete receivers without
/// the required field shape.
#[test]
fn syntax_output_rejects_unproven_receiver_method_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_receiver_method_rejected.

pub struct Presenter {
    prefix: String
}.

pub struct Account {
    id: Int
}.

pub (presenter: Presenter) display_name[T => {name: String}](value: T): String ->
    value.name.

pub render(presenter: Presenter, account: Account): String ->
    presenter.display_name(account).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` requires field `name` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies a concrete type without a required field fails closed.
#[test]
fn syntax_output_rejects_missing_structural_implication_field() {
    let diagnostics = check_syntax_output(
        r#"
module implication_missing_field.

pub struct Account {
    id: Int
}.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render(account: Account): String -> display_name(account).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` requires field `name` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}
