//! End-to-end regressions for interface-first module development.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn check(path: &Path, flags: &[&str]) -> ExitCode {
    let mut args = vec![path.to_string_lossy().into_owned()];
    args.extend(flags.iter().map(|flag| flag.to_string()));
    super::run(
        CliCommand {
            verb: Some("check".into()),
            args,
        },
        CliState::default(),
    )
}

fn ledger(root: &Path) {
    write(root, "ledger/Money.terli", "module ledger.Money.\npub opaque type Money.\n@pure\npub of_pence(pence: Int): Money.\n@pure\npub add(left: Money, right: Money): Money.\n");
    write(root, "ledger/Book.terli", "module ledger.Book.\nimport ledger.Money.{Money}.\npub opaque type Book.\npub empty(): Book.\npub deposit(amount: Money, book: Book): Book.\npub balance(book: Book): Money.\n");
    write(root, "ledger/Client.terl", "module ledger.Client.\nimport ledger.Money.{Money, of_pence}.\nimport ledger.Book.{empty, deposit, balance}.\npub example(): Money -> balance(deposit(of_pence(1000), empty())).\n");
}

#[test]
fn interface_first_checks_local_include_chains_and_rejects_cycles() {
    let dir = TestDirectory::new("interfaces", "local_include_chains");
    write(dir.path(), "api.terli", "module api. pub struct Leaf includes Middle { leaf: String }. pub struct Middle includes Base { middle: Bool }. pub struct Base { code: Int }.");
    write(
        dir.path(),
        "client.terl",
        "module client. import api.{Leaf}. pub code(value: Leaf): Int -> value.code.",
    );
    assert_eq!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
    write(dir.path(), "api.terl", "module api. pub struct Leaf { code: Int, middle: Bool, leaf: String }. pub struct Middle { code: Int, middle: Bool }. pub struct Base { code: Int }.");
    assert_eq!(check(dir.path(), &[]), ExitCode::SUCCESS);
    write(dir.path(), "api.terl", "module api. pub struct Leaf { middle: Bool, leaf: String }. pub struct Middle { code: Int, middle: Bool }. pub struct Base { code: Int }.");
    assert_ne!(check(dir.path(), &[]), ExitCode::SUCCESS);
    write(
        dir.path(),
        "api.terli",
        "module api. pub struct A includes B { a: Int }. pub struct B includes A { b: Int }.",
    );
    assert_ne!(check(&dir.path().join("api.terli"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_checks_interfaces_and_real_client_without_implementations() {
    let dir = TestDirectory::new("interfaces", "ledger_client");
    ledger(dir.path());
    assert_eq!(
        check(&dir.path().join("ledger/Money.terli"), &[]),
        ExitCode::SUCCESS
    );
    assert_eq!(
        check(&dir.path().join("ledger/Book.terli"), &[]),
        ExitCode::SUCCESS
    );
    assert_eq!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
    assert_eq!(check(dir.path(), &[]), ExitCode::SUCCESS);
    write(dir.path(), "ledger/Client.terl", "module ledger.Client.\nimport ledger.Money.{Money, zero}.\npub example(): Money -> zero().\n");
    assert_ne!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_skips_paired_implementation_bodies_only_when_requested() {
    let dir = TestDirectory::new("interfaces", "skip_paired_bodies");
    ledger(dir.path());
    write(
        dir.path(),
        "ledger/Money.terl",
        "this is deliberately unfinished implementation source",
    );
    assert_eq!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
    assert_ne!(check(dir.path(), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_validates_unknown_types_arities_and_imports() {
    let dir = TestDirectory::new("interfaces", "signature_errors");
    let path = dir.path().join("api.terli");
    for source in [
        "module api. pub value(): Missing.",
        "module api. pub opaque type Box[T]. pub value(): Box[Int, String].",
        "module api. pub value(): missing.Module.Type.",
        "module api. import missing.Module.{Type}. pub value(): Type.",
        "module api. import missing.Module.{unused}. pub value(): Int.",
    ] {
        fs::write(&path, source).unwrap();
        assert_ne!(check(&path, &[]), ExitCode::SUCCESS, "accepted {source}");
    }
    fs::write(
        &path,
        "module api. pub opaque type Box[T]. pub identity[T](value: Box[T]): Box[T].",
    )
    .unwrap();
    assert_eq!(check(&path, &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_does_not_allow_missing_implementation_bodies() {
    let dir = TestDirectory::new("interfaces", "no_body_fallback");
    let path = dir.path().join("api.terl");
    fs::write(&path, "module api. pub value(): Int.").unwrap();
    assert_ne!(check(&path, &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_enforces_opaque_types_and_all_public_function_signatures() {
    let dir = TestDirectory::new("interfaces", "conformance");
    write(dir.path(), "api.terli", "module api. pub opaque type Money. @pure pub make(value: Int): Money. pub add(left: Money, right: Money): Money.");
    let path = dir.path().join("api.terl");
    for source in [
        "module api. pub opaque type Money = Int. pub make(value: Int): String -> \"wrong\".",
        "module api. pub opaque type Money = Int. pub make(): Money -> Money(0).",
        "module api. pub type Money = Int. pub make(value: Int): Money -> value. pub add(left: Money, right: Money): Money -> left + right.",
        "module api. pub opaque type Money = Int. pub make(value: Int): Money -> Money(value).",
    ] {
        fs::write(&path, source).unwrap();
        assert_ne!(check(&path, &[]), ExitCode::SUCCESS, "accepted {source}");
    }
    fs::write(&path, "module api. pub opaque type Money = Int. @pure pub make(value: Int): Money -> Money(value). pub add(left: Money, right: Money): Money -> left. pub helper(): Int -> 1.").unwrap();
    assert_eq!(check(&path, &[]), ExitCode::SUCCESS);
    assert_eq!(check(dir.path(), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_keeps_representation_and_helpers_hidden_from_clients() {
    let dir = TestDirectory::new("interfaces", "hidden_representation");
    write(
        dir.path(),
        "api.terli",
        "module api. pub opaque type Money. pub make(value: Int): Money.",
    );
    write(dir.path(), "api.terl", "module api. pub opaque type Money = Int. pub make(value: Int): Money -> Money(value). pub helper(): Int -> 1.");
    for body in [
        "import api.{helper}. pub value(): Int -> helper().",
        "import api.{Money}. pub value(): Money -> Money(5).",
        "import api.{Money}. pub value(left: Money, right: Money): Money -> left - right.",
    ] {
        write(dir.path(), "client.terl", &format!("module client. {body}"));
        assert_ne!(check(dir.path(), &[]), ExitCode::SUCCESS, "accepted {body}");
    }
}

#[test]
fn interface_first_purity_is_checked_against_implementation_effects() {
    let dir = TestDirectory::new("interfaces", "pure_contract");
    write(
        dir.path(),
        "api.terli",
        "module api. @pure pub emit(): Unit.",
    );
    write(
        dir.path(),
        "api.terl",
        "module api. import std.io.Console.{println}. pub emit(): Unit -> println(\"effect\").",
    );
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_accepts_generic_renaming_and_checks_parameter_defaults() {
    let dir = TestDirectory::new("interfaces", "generic_contract");
    write(
        dir.path(),
        "api.terli",
        "module api. pub identity[T](value: T): T. pub count(value: Int = 2): Int.",
    );
    write(
        dir.path(),
        "api.terl",
        "module api. pub identity[U](item: U): U -> item. pub count(value: Int = 2): Int -> value.",
    );
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    write(
        dir.path(),
        "api.terl",
        "module api. pub identity[U](item: U): U -> item. pub count(value: Int = 3): Int -> value.",
    );
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_phase_manifest_has_no_executable_or_proof_evidence() {
    let dir = TestDirectory::new("interfaces", "honest_manifest");
    write(dir.path(), "api.terli", "module api. pub value(): Int.");
    let manifest = dir.path().join("phase.json");
    assert_eq!(
        check(
            &dir.path().join("api.terli"),
            &["--emit-phase-manifest", manifest.to_str().unwrap()]
        ),
        ExitCode::SUCCESS
    );
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
    let text = json.to_string();
    assert!(text.contains("\"core_ir_hash\":0"), "{text}");
    assert!(text.contains("\"status\":\"skipped\""), "{text}");
}

#[test]
fn interface_first_rejects_malformed_sibling_contract() {
    let dir = TestDirectory::new("interfaces", "malformed_contract");
    write(dir.path(), "api.terli", "invalid interface source");
    write(dir.path(), "api.terl", "module api. pub value(): Int -> 1.");
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_supports_mutually_referencing_interfaces() {
    let dir = TestDirectory::new("interfaces", "mutual_signatures");
    write(
        dir.path(),
        "left.terli",
        "module left. import right.{Right}. pub opaque type Left. pub convert(value: Left): Right.",
    );
    write(
        dir.path(),
        "right.terli",
        "module right. import left.{Left}. pub opaque type Right. pub convert(value: Right): Left.",
    );
    assert_eq!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_supports_generic_receiver_signatures_and_trait_types() {
    let dir = TestDirectory::new("interfaces", "receiver_and_trait");
    write(dir.path(), "api.terli", "module api. pub opaque type Box[T]. pub (box: Box[T]) get(): T. pub trait Show[T] { show(value: T): String. }.");
    assert_eq!(check(&dir.path().join("api.terli"), &[]), ExitCode::SUCCESS);
    write(
        dir.path(),
        "api.terli",
        "module api. pub trait Show[T] { show(value: T): Missing. }.",
    );
    assert_ne!(check(&dir.path().join("api.terli"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_enforces_transparent_types_and_public_fields() {
    let dir = TestDirectory::new("interfaces", "public_type_shapes");
    write(
        dir.path(),
        "api.terli",
        "module api. pub type Count = Int. pub struct Item { count: Count }.",
    );
    write(
        dir.path(),
        "api.terl",
        "module api. pub type Count = Int. pub struct Item { count: Count }. type Private = Bool.",
    );
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    for implementation in [
        "module api. pub type Count = String. pub struct Item { count: Count }.",
        "module api. pub type Count = Int. pub struct Item { count: String }.",
        "module api. pub type Count = Int. struct Item { count: Count }.",
    ] {
        write(dir.path(), "api.terl", implementation);
        assert_ne!(
            check(&dir.path().join("api.terl"), &[]),
            ExitCode::SUCCESS,
            "{implementation}"
        );
    }
}

#[test]
fn interface_first_expands_inherited_interface_fields_before_checking_clients() {
    let dir = TestDirectory::new("interfaces", "inherited_fields");
    write(
        dir.path(),
        "base.terli",
        "module base. pub struct Base { code: Int }.",
    );
    write(
        dir.path(),
        "child.terli",
        "module child. import base.{Base}. pub struct Child includes Base { message: String }.",
    );
    write(
        dir.path(),
        "client.terl",
        "module client. import child.{Child}. pub code(value: Child): Int -> value.code.",
    );
    assert_eq!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_projects_expand_adjacent_summary_fields_before_checking_clients() {
    let dir = TestDirectory::new("interfaces", "adjacent_inherited_fields");
    write(
        dir.path(),
        "dep.typi",
        "module dep. pub struct Base { code: Int }.",
    );
    write(
        dir.path(),
        "api.terli",
        "module api. import dep.{Base}. pub struct Child includes Base { message: String }.",
    );
    write(
        dir.path(),
        "client.terl",
        "module client. import api.{Child}. pub code(value: Child): Int -> value.code.",
    );
    for flags in [&[][..], &["--interfaces-only"][..]] {
        assert_eq!(check(dir.path(), flags), ExitCode::SUCCESS);
    }
    write(
        dir.path(),
        "dep.typi",
        "module dep. pub struct Base { code: String }.",
    );
    assert_ne!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_inherited_fields_preserve_external_opaque_type_identity() {
    let dir = TestDirectory::new("interfaces", "inherited_type_identity");
    write(
        dir.path(),
        "provider/dep.typi",
        "module dep. pub opaque type Token. pub struct Base { token: Token }.",
    );
    write(dir.path(), "provider/Api.terli", "module provider.Api. import dep.{Base}. pub opaque type Token. pub struct Child includes Base { code: Int }.");
    write(dir.path(), "client.terl", "module client. import provider.Api.{Child}. pub token(value: Child): dep.Token -> value.token.");
    assert_eq!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
    write(dir.path(), "client.terl", "module client. import provider.Api.{Child, Token}. pub token(value: Child): Token -> value.token.");
    assert_ne!(check(dir.path(), &["--interfaces-only"]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_declared_api_overrides_richer_generated_summaries() {
    let dir = TestDirectory::new("interfaces", "summary_precedence");
    write(dir.path(), "api.terli", "module api. pub value(): Int.");
    write(
        dir.path(),
        "api.typi",
        "module api. pub value(): Int. pub helper(): Int.",
    );
    write(
        dir.path(),
        "client.terl",
        "module client. import api.{helper}. pub value(): Int -> helper().",
    );
    assert_ne!(
        check(&dir.path().join("client.terl"), &[]),
        ExitCode::SUCCESS
    );
}

#[test]
fn interface_first_preserves_dependency_identity_across_import_aliases() {
    let dir = TestDirectory::new("interfaces", "qualified_identities");
    write(
        dir.path(),
        "one.terli",
        "module one. pub opaque type Value.",
    );
    write(
        dir.path(),
        "two.terli",
        "module two. pub opaque type Value.",
    );
    write(
        dir.path(),
        "api.terli",
        "module api. import one.{Value}. pub identity(value: Value): Value.",
    );
    write(
        dir.path(),
        "api.terl",
        "module api. import one.{Value as Item}. pub identity(value: Item): Item -> value.",
    );
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    write(
        dir.path(),
        "api.terl",
        "module api. import two.{Value}. pub identity(value: Value): Value -> value.",
    );
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_preserves_generic_trait_bounds() {
    let dir = TestDirectory::new("interfaces", "generic_bounds");
    write(dir.path(), "api.terli", "module api. pub trait Marker[T] { mark(value: T): T. }. pub identity[T](value: T)[Marker[T]]: T.");
    write(dir.path(), "api.terl", "module api. pub trait Marker[T] { mark(value: T): T. }. pub identity[U](value: U)[Marker[U]]: U -> value.");
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    write(dir.path(), "api.terl", "module api. pub trait Marker[T] { mark(value: T): T. }. pub identity[U](value: U): U -> value.");
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_trait_conformance_facts_require_implementation_methods_later() {
    let dir = TestDirectory::new("interfaces", "trait_conformance");
    write(dir.path(), "api.terli", "module api. pub opaque type Item. pub trait Show[T] { show(value: T): String. }. pub impl Show[Item] for Item {}.");
    assert_eq!(check(&dir.path().join("api.terli"), &[]), ExitCode::SUCCESS);
    write(dir.path(), "api.terl", "module api. pub opaque type Item = Int. pub trait Show[T] { show(value: T): String. }. pub impl Show[Item] for Item {}.");
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    write(dir.path(), "api.terl", "module api. pub opaque type Item = Int. pub trait Show[T] { show(value: T): String. }. pub impl Show[Item] for Item { show(value: Item): String -> \"item\". }.");
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_rejects_undeclared_overloads_of_a_declared_operation() {
    let dir = TestDirectory::new("interfaces", "overload_contract");
    write(
        dir.path(),
        "api.terl",
        "module api. pub value(item: Int): Int -> item. pub value(item: String): String -> item.",
    );
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    write(
        dir.path(),
        "api.terli",
        "module api. pub value(item: Int): Int.",
    );
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_accepts_private_transparent_aliases_without_exporting_them() {
    let dir = TestDirectory::new("interfaces", "private_alias_contract");
    write(dir.path(), "api.terli", "module api. pub value(): Int.");
    write(
        dir.path(),
        "api.terl",
        "module api. type Hidden = Int. type Indirect = Hidden. pub value(): Indirect -> 1.",
    );
    assert_eq!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
    let source = fs::read_to_string(dir.path().join("api.terl")).unwrap();
    let compiled =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            dir.path().join("api.terl").to_str().unwrap(),
            &source,
            crate::DiagnosticFormat::Text {
                color: crate::ColorChoice::Never,
            },
            None,
            CliState::default().native_policy,
            crate::validation::target_profile::TargetProfile::Vm,
        )
        .artifacts
        .unwrap();
    let summary = compiled.core.interface.to_terlan_interface_text();
    assert!(summary.contains("pub value(): Int."), "{summary}");
    let declarations =
        crate::commands::emit_js::emit_core_module_to_typescript_declarations(&compiled.core);
    assert!(declarations.contains("value(): number"), "{declarations}");
    assert!(
        !declarations.contains("Hidden") && !declarations.contains("Indirect"),
        "{declarations}"
    );
    write(
        dir.path(),
        "api.terl",
        "module api. opaque type Hidden = Int. pub value(): Hidden -> Hidden(1).",
    );
    assert_ne!(check(&dir.path().join("api.terl"), &[]), ExitCode::SUCCESS);
}

#[test]
fn interface_first_single_file_checks_validate_transitive_interface_dependencies() {
    let dir = TestDirectory::new("interfaces", "dependency_validation");
    write(dir.path(), "dep.terli", "module dep. pub value(): Missing.");
    write(
        dir.path(),
        "middle.terli",
        "module middle. import dep.{value}. pub result(): Int.",
    );
    write(
        dir.path(),
        "client.terl",
        "module client. import middle.{result}. pub value(): Int -> result().",
    );
    assert_ne!(
        check(&dir.path().join("client.terl"), &[]),
        ExitCode::SUCCESS
    );
    assert_ne!(
        check(&dir.path().join("middle.terli"), &[]),
        ExitCode::SUCCESS
    );
    write(dir.path(), "dep.terli", "module dep. pub value(): Int.");
    write(
        dir.path(),
        "unrelated.terli",
        "unfinished unrelated interface",
    );
    assert_eq!(
        check(&dir.path().join("client.terl"), &[]),
        ExitCode::SUCCESS
    );
    write(dir.path(), "dep.terli", "invalid dependency interface");
    assert_ne!(
        check(&dir.path().join("client.terl"), &[]),
        ExitCode::SUCCESS
    );
}

#[test]
fn interface_first_transitive_interfaces_load_embedded_dependencies() {
    let dir = TestDirectory::new("interfaces", "embedded_dependency_validation");
    write(
        dir.path(),
        "dep.terli",
        "module dep. import std.io.Console.{println}. pub value(): Int.",
    );
    write(
        dir.path(),
        "client.terl",
        "module client. import dep.{value}. pub result(): Int -> value().",
    );
    assert_eq!(
        check(&dir.path().join("client.terl"), &[]),
        ExitCode::SUCCESS
    );
}

#[test]
fn interface_first_validates_dependencies_referenced_only_by_qualified_names() {
    let dir = TestDirectory::new("interfaces", "qualified_dependency_validation");
    write(
        dir.path(),
        "Dep.terli",
        "module app.Dep. pub opaque type Box. pub value(): Int. pub invalid(): Missing.",
    );
    write(
        dir.path(),
        "client.terli",
        "module client. pub value(): app.Dep.Box.",
    );
    assert_ne!(
        check(&dir.path().join("client.terli"), &[]),
        ExitCode::SUCCESS
    );
    write(
        dir.path(),
        "caller.terl",
        "module caller. pub result(): Int -> app.Dep.value().",
    );
    assert_ne!(
        check(&dir.path().join("caller.terl"), &[]),
        ExitCode::SUCCESS
    );
    write(
        dir.path(),
        "Dep.terli",
        "module app.Dep. pub opaque type Box. pub value(): Int.",
    );
    assert_eq!(
        check(&dir.path().join("client.terli"), &[]),
        ExitCode::SUCCESS
    );
    assert_eq!(
        check(&dir.path().join("caller.terl"), &[]),
        ExitCode::SUCCESS
    );
}

#[test]
fn interface_first_adjacent_contracts_override_malformed_cached_interfaces() {
    let dir = TestDirectory::new("interfaces", "dependency_precedence");
    write(dir.path(), "dep.terli", "module dep. pub value(): Int.");
    write(dir.path(), "cache/dep.terli", "invalid cached interface");
    let source = "module client. import dep.{value}. pub result(): Int -> value().";
    let path = dir.path().join("client.terl");
    let cache = dir.path().join("cache");
    write(dir.path(), "client.terl", source);
    let compile = || {
        crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            path.to_str().unwrap(),
            source,
            crate::DiagnosticFormat::default(),
            Some(&cache),
            CliState::default().native_policy,
            crate::validation::target_profile::TargetProfile::Vm,
        )
    };
    assert_eq!(compile().exit_code, ExitCode::SUCCESS);
    write(
        dir.path(),
        "cache/dep.terli",
        "module dep. pub value(): Int.",
    );
    write(dir.path(), "dep.terli", "invalid adjacent interface");
    assert_ne!(compile().exit_code, ExitCode::SUCCESS);
}

#[test]
fn interface_first_incremental_project_checks_revalidate_external_contracts() {
    let dir = TestDirectory::new("interfaces", "project_external_contracts");
    write(
        dir.path(),
        "src/client.terl",
        "module client. import dep.{value}. pub result(): Int -> value().",
    );
    write(
        dir.path(),
        "cache/dep.terli",
        "module dep. pub value(): Int.",
    );
    let source = dir.path().join("src");
    let state = CliState {
        cache_dir: Some(dir.path().join("cache")),
        incremental: true,
        ..CliState::default()
    };
    let run_check = || {
        super::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![source.to_string_lossy().into_owned()],
            },
            state.clone(),
        )
    };
    assert_eq!(run_check(), ExitCode::SUCCESS);
    assert_eq!(run_check(), ExitCode::SUCCESS);
    write(
        dir.path(),
        "cache/dep.terli",
        "module dep. pub value(): Int. pub invalid(): Missing.",
    );
    assert_ne!(run_check(), ExitCode::SUCCESS);
}

#[test]
fn interface_first_project_uses_source_contracts_and_external_cached_dependencies() {
    let dir = TestDirectory::new("interfaces", "project_dependency_precedence");
    write(
        dir.path(),
        "src/api/Value.terli",
        "module api.Value. pub value(): Int.",
    );
    write(dir.path(), "src/client.terl", "module client. import api.Value.{value}. import dep.{extra}. pub result(): Int -> value() + extra().");
    write(
        dir.path(),
        "cache/api.Value.terli",
        "module api.Value. pub value(): String. pub invalid(): Missing.",
    );
    write(
        dir.path(),
        "cache/dep.typi",
        "module dep. pub extra(): Int.",
    );
    let source = dir.path().join("src");
    let state = CliState {
        cache_dir: Some(dir.path().join("cache")),
        ..CliState::default()
    };
    for interfaces_only in [false, true] {
        let mut args = vec![source.to_string_lossy().into_owned()];
        if interfaces_only {
            args.push("--interfaces-only".into());
        }
        assert_eq!(
            super::run(
                CliCommand {
                    verb: Some("check".into()),
                    args
                },
                state.clone()
            ),
            ExitCode::SUCCESS
        );
    }
}

#[test]
fn interface_first_public_type_representations_hide_private_aliases() {
    let dir = TestDirectory::new("interfaces", "public_representation_aliases");
    write(
        dir.path(),
        "api.terli",
        "module api. pub type Value = Int. pub struct Record { value: Int }. pub value(): Value.",
    );
    write(dir.path(), "api.terl", "module api. type Hidden = Int. pub type Value = Hidden. pub struct Record { value: Hidden }. pub value(): Value -> 1.");
    let path = dir.path().join("api.terl");
    let source = fs::read_to_string(&path).unwrap();
    let compiled =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            path.to_str().unwrap(),
            &source,
            crate::DiagnosticFormat::default(),
            None,
            CliState::default().native_policy,
            crate::validation::target_profile::TargetProfile::Vm,
        )
        .artifacts
        .unwrap();
    let summary = compiled.core.interface.to_terlan_interface_text();
    assert_eq!(
        compiled.core.interface.type_bodies["Value"],
        ["Int"],
        "{summary}"
    );
    assert_eq!(
        compiled.core.interface.struct_fields["Record"][0].annotation, "Int",
        "{summary}"
    );
    let declarations =
        crate::commands::emit_js::emit_core_module_to_typescript_declarations(&compiled.core);
    assert!(
        declarations.contains("export type Value = number"),
        "{declarations}"
    );
    assert!(declarations.contains("value: number"), "{declarations}");
    assert!(!declarations.contains("Hidden"), "{declarations}");
}

#[test]
fn directory_check_reads_generated_package_bindings() {
    const CHILD_ROOT: &str = "TERLAN_GENERATED_BINDING_CHECK_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        assert_eq!(check(Path::new(&root), &[]), ExitCode::SUCCESS);
        return;
    }
    let dir = TestDirectory::new("check", "generated_package_bindings");
    write(
        dir.path(),
        "bindings/Host.typi",
        "module example.Host.\npub opaque type Host.\npub (receiver: Host) value(): Int.\n",
    );
    write(
        dir.path(),
        "bindings/manifest.json",
        r#"{"outputs":[{"module":"example.Host","summary":"Host.typi","members":[{"name":"value","js_name":"value","kind":"property","arity":1,"return_type":"Int"}]}]}"#,
    );
    write(dir.path(), "source/client/Main.terl", "module client.Main.\nimport example.Host.\nimport type example.Host.{Host}.\npub read(host: Host): Int -> Host.value(host).\n");
    // The host registry is process-local, so load this package in a fresh process.
    let thread = std::thread::current();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", thread.name().unwrap(), "--nocapture"])
        .env(CHILD_ROOT, dir.path().join("source"))
        .env(
            "TERLAN_JS_BINDING_MANIFESTS",
            dir.path().join("bindings/manifest.json"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
