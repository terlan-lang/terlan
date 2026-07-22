use super::test_support::{check_syntax_output, check_syntax_output_with_interface};

const TRAIT_PREAMBLE: &str = "\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) show(): String ->\n\
    user.name.\n\
";

const TRAIT_RECEIVER_PREAMBLE: &str = "\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[User] for User {\n\
    show(value: User): String ->\n\
        value.name.\n\
}.\n\
";

const PURE_TRAIT_PREAMBLE: &str = "\
pub trait Show[T] {\n\
    @pure\n\
    show(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) show(): String ->\n\
    user.name.\n\
";

const PURE_TRAIT_RECEIVER_PREAMBLE: &str = "\
pub trait Show[T] {\n\
    @pure\n\
    show(value: T): String.\n\
}.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[User] for User {\n\
    show(value: User): String ->\n\
        value.name.\n\
}.\n\
";

#[test]
fn ordinary_function_accepts_trait_call_without_purity_contract() {
    let source = format!(
        "module purity.trait_ordinary.\n\n{TRAIT_PREAMBLE}\npub render(user: User): String ->\n    Show.show(user).\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_function_rejects_trait_call_without_purity_contract() {
    let source = format!(
        "module purity.trait_direct.\n\n{TRAIT_PREAMBLE}\n@pure\npub render(user: User): String ->\n    Show.show(user).\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("effectful trait call without a purity contract")));
}

#[test]
fn trait_call_effect_propagates_through_local_helper() {
    let source = format!(
        "module purity.trait_transitive.\n\n{TRAIT_PREAMBLE}\nrender_step(user: User): String ->\n    Show.show(user).\n\n@pure\npub render(user: User): String ->\n    render_step(user).\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("effectful local function call")));
}

#[test]
fn ordinary_function_accepts_receiver_style_trait_call() {
    let source = format!(
        "module purity.trait_receiver_ordinary.\n\n{TRAIT_RECEIVER_PREAMBLE}\npub render(user: User): String ->\n    user.show().\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_function_rejects_receiver_style_trait_call_without_contract() {
    let source = format!(
        "module purity.trait_receiver_direct.\n\n{TRAIT_RECEIVER_PREAMBLE}\n@pure\npub render(user: User): String ->\n    user.show().\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("effectful receiver-style trait call without a purity contract")));
}

#[test]
fn receiver_style_trait_effect_propagates_through_helper() {
    let source = format!(
        "module purity.trait_receiver_transitive.\n\n{TRAIT_RECEIVER_PREAMBLE}\nrender_step(user: User): String ->\n    user.show().\n\n@pure\npub render(user: User): String ->\n    render_step(user).\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("effectful local function call")));
}

#[test]
fn concrete_receiver_method_precedes_trait_fallback_classification() {
    let source = format!(
        "module purity.trait_receiver_precedence.\n\n{TRAIT_PREAMBLE}\n@pure\npub render(user: User): String ->\n    user.show().\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_function_accepts_qualified_trait_call_with_purity_contract() {
    let source = format!(
        "module purity.trait_positive.\n\n{PURE_TRAIT_PREAMBLE}\n@pure\npub render(user: User): String ->\n    Show.show(user).\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_function_accepts_receiver_trait_call_with_purity_contract() {
    let source = format!(
        "module purity.trait_receiver_positive.\n\n{PURE_TRAIT_RECEIVER_PREAMBLE}\n@pure\npub render(user: User): String ->\n    user.show().\n"
    );
    let diagnostics = check_syntax_output(&source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_function_accepts_imported_trait_call_with_purity_contract() {
    let diagnostics = check_syntax_output_with_interface(
        "module purity.imported_trait_positive.\n\n\
import traits.{Show}.\n\n\
@pure\n\
pub render(value: Dynamic): String ->\n\
    Show.show(value).\n",
        "module traits.Show.\n\n\
pub trait Show[T] {\n\
    @pure\n\
    show(value: T): String.\n\
}.\n\n\
pub impl Show[Dynamic] for Dynamic {\n\
    show(value: Dynamic): String.\n\
}.\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_trait_default_rejects_effectful_body() {
    let diagnostics = check_syntax_output(
        "module purity.trait_default_impure.\n\n\
pub trait Replace[T] {\n\
    @pure\n\
    replace(items: List[T]): Unit ->\n\
        items[0] = items[0].\n\
}.\n",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
        "trait default method replace required pure by its contract must be pure; found indexed assignment"
    )), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_trait_rejects_effectful_explicit_impl_body() {
    let diagnostics = check_syntax_output(
        "module purity.trait_impl_impure.\n\n\
pub trait Replace[T] {\n\
    @pure\n\
    replace(items: List[T]): Unit.\n\
}.\n\n\
pub impl Replace[Int] for List[Int] {\n\
    replace(items: List[Int]): Unit ->\n\
        items[0] = 1.\n\
}.\n",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
        "impl method replace required pure by its trait contract must be pure; found indexed assignment"
    )), "diagnostics: {diagnostics:?}");
}

#[test]
fn pure_trait_rejects_effectful_declared_implements_receiver_body() {
    let diagnostics = check_syntax_output(
        "module purity.trait_receiver_impl_impure.\n\n\
pub trait Replace[T] {\n\
    @pure\n\
    replace(value: T, items: List[Int]): Unit.\n\
}.\n\n\
pub struct Box implements Replace[Box] {\n\
    value: Int\n\
}.\n\n\
pub (box: Box) replace(items: List[Int]): Unit ->\n\
    items[0] = box.value.\n",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
        "receiver method replace required pure by its trait contract must be pure; found indexed assignment"
    )), "diagnostics: {diagnostics:?}");
}
