use super::*;
use crate::support::test_fs::TestDirectory;
use crate::terlan_syntax::parse_module_as_syntax_output;

fn generate(paths: &[&Path], out: &Path) -> ExitCode {
    run(
        &paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        &CliState {
            out_dir: out.to_owned(),
            ..CliState::default()
        },
    )
}

#[test]
fn interface_first_generation_flattens_local_ancestors_and_rejects_cycles() {
    let dir = TestDirectory::new("interfaces", "generation_local_include_chains");
    let source = dir.path().join("api.terli");
    let out = dir.path().join("out");
    fs::write(&source, "module api. pub struct Leaf includes Middle { leaf: String }. pub struct Middle includes Base { middle: Bool }. pub struct Base { code: Int }.").unwrap();
    assert_eq!(generate(&[&source], &out), ExitCode::SUCCESS);
    let summary = fs::read_to_string(out.join("api.typi")).unwrap();
    let syntax = parse_module_as_syntax_output(&summary).unwrap();
    let interface = crate::terlan_hir::syntax_module_output_to_interface(&syntax);
    assert_eq!(
        interface.struct_fields["Leaf"]
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["code", "middle", "leaf"]
    );
    fs::write(
        &source,
        "module api. pub struct A includes B { a: Int }. pub struct B includes A { b: Int }.",
    )
    .unwrap();
    assert_ne!(generate(&[&source], &out), ExitCode::SUCCESS);
    assert_eq!(fs::read_to_string(out.join("api.typi")).unwrap(), summary);
}

#[test]
fn interface_first_generation_obeys_sibling_contracts() {
    let dir = TestDirectory::new("interfaces", "generation_contract");
    let source = dir.path().join("api.terl");
    let contract = source.with_extension("terli");
    let out = dir.path().join("out");
    fs::write(
        &source,
        "module api. type Hidden = Int. pub value(): Hidden -> helper(). pub helper(): Int -> 7.",
    )
    .unwrap();
    fs::write(&contract, "module api. pub value(): Int.").unwrap();
    assert_eq!(generate(&[&source], &out), ExitCode::SUCCESS);
    let summary = fs::read_to_string(out.join("api.typi")).unwrap();
    assert!(summary.contains("pub value(): Int."), "{summary}");
    assert!(!summary.contains("pub helper"), "{summary}");
    let admitted = fs::read(out.join("api.typi")).unwrap();
    let manifest = fs::read(out.join("api.typi.deps")).unwrap();
    fs::write(&contract, "module api. pub value(): String.").unwrap();
    assert_ne!(generate(&[&source], &out), ExitCode::SUCCESS);
    assert_eq!(fs::read(out.join("api.typi")).unwrap(), admitted);
    assert_eq!(fs::read(out.join("api.typi.deps")).unwrap(), manifest);
    fs::write(&contract, "unfinished interface").unwrap();
    assert_ne!(generate(&[&source], &out), ExitCode::SUCCESS);
    fs::write(&contract, "module different. pub value(): Int.").unwrap();
    assert_ne!(generate(&[&source], &out), ExitCode::SUCCESS);
}

#[test]
fn interface_first_generation_validates_all_declarations_before_publishing_batch() {
    let dir = TestDirectory::new("interfaces", "generation_validation");
    let good = dir.path().join("good.terli");
    let bad = dir.path().join("bad.terli");
    let out = dir.path().join("out");
    fs::write(&good, "module good. pub value(): Int.").unwrap();
    fs::write(&bad, "module bad. pub value(): Missing.").unwrap();
    assert_ne!(generate(&[&good, &bad], &out), ExitCode::SUCCESS);
    assert!(!out.join("good.typi").exists() && !out.join("bad.typi").exists());
    fs::write(&bad, "module bad. pub value(): Int.").unwrap();
    assert_eq!(generate(&[&good, &bad], &out), ExitCode::SUCCESS);
}

#[test]
fn interface_first_generation_rejects_invalid_external_dependencies() {
    let dir = TestDirectory::new("interfaces", "generation_dependencies");
    let client = dir.path().join("client.terli");
    let dependency = dir.path().join("dep.terli");
    let out = dir.path().join("out");
    fs::write(
        &client,
        "module client. import dep.{value}. pub result(): Int.",
    )
    .unwrap();
    fs::write(
        &dependency,
        "module dep. pub value(): Int. pub invalid(): Missing.",
    )
    .unwrap();
    assert_ne!(generate(&[&client], &out), ExitCode::SUCCESS);
    assert!(!out.join("client.typi").exists());
}

#[test]
fn interface_first_generation_validates_uncontracted_sources_in_checked_batches() {
    let dir = TestDirectory::new("interfaces", "generation_checked_source_dependency");
    let client = dir.path().join("client.terli");
    let dependency = dir.path().join("dep.terl");
    let out = dir.path().join("out");
    fs::write(
        &client,
        "module client. import dep.{value}. pub result(): Int.",
    )
    .unwrap();
    fs::write(&dependency, "module dep. pub value(): Int -> \"wrong\".").unwrap();
    assert_ne!(generate(&[&client, &dependency], &out), ExitCode::SUCCESS);
    assert!(!out.join("client.typi").exists() && !out.join("dep.typi").exists());
    fs::write(&dependency, "module dep. pub value(): Int -> 7.").unwrap();
    assert_eq!(generate(&[&client, &dependency], &out), ExitCode::SUCCESS);
}

#[test]
fn interface_first_generation_preserves_cross_directory_contract_boundaries() {
    let dir = TestDirectory::new("interfaces", "generation_batch_boundaries");
    fs::create_dir_all(dir.path().join("provider")).unwrap();
    fs::create_dir_all(dir.path().join("client")).unwrap();
    let provider = dir.path().join("provider/api.terl");
    let client = dir.path().join("client/client.terli");
    let out = dir.path().join("out");
    fs::write(
        &provider,
        "module api. pub value(): Int -> 7. pub helper(): Int -> 8.",
    )
    .unwrap();
    fs::write(
        provider.with_extension("terli"),
        "module api. pub value(): Int.",
    )
    .unwrap();
    fs::write(
        &client,
        "module client. import api.{helper}. pub result(): Int.",
    )
    .unwrap();
    assert_ne!(generate(&[&client, &provider], &out), ExitCode::SUCCESS);
    assert!(!out.join("api.typi").exists());
    fs::write(
        &client,
        "module client. import api.{value}. pub result(): Int.",
    )
    .unwrap();
    assert_eq!(generate(&[&client, &provider], &out), ExitCode::SUCCESS);
    let api = fs::read_to_string(out.join("api.typi")).unwrap();
    let manifest =
        crate::commands::artifacts::read_manifest(&out.join("client.typi.deps")).unwrap();
    let (_, interface) = crate::terlan_hir::parse_interface_text(&api).unwrap();
    assert_eq!(
        manifest
            .dependencies
            .iter()
            .find(|(name, _)| name == "api")
            .unwrap()
            .1,
        crate::commands::artifacts::fingerprint(
            interface.to_terlan_interface_type_text().as_bytes()
        )
    );
}

#[test]
fn interface_first_generation_loads_every_source_directory_independently_of_argument_order() {
    let dir = TestDirectory::new("interfaces", "generation_adjacent_inheritance");
    fs::create_dir_all(dir.path().join("provider")).unwrap();
    fs::create_dir_all(dir.path().join("client")).unwrap();
    let api = dir.path().join("provider/api.terli");
    let client = dir.path().join("client/client.terl");
    fs::write(
        dir.path().join("provider/dep.typi"),
        "module dep. pub struct Base { code: Int }.",
    )
    .unwrap();
    fs::write(
        &api,
        "module api. import dep.{Base}. pub struct Child includes Base { message: String }.",
    )
    .unwrap();
    fs::write(
        &client,
        "module client. import api.{Child}. pub code(value: Child): Int -> value.code.",
    )
    .unwrap();
    let first = dir.path().join("client-first");
    let second = dir.path().join("api-first");
    assert_eq!(generate(&[&client, &api], &first), ExitCode::SUCCESS);
    assert_eq!(generate(&[&api, &client], &second), ExitCode::SUCCESS);
    for file in [
        "api.typi",
        "api.typi.deps",
        "client.typi",
        "client.typi.deps",
    ] {
        assert_eq!(
            fs::read(first.join(file)).unwrap(),
            fs::read(second.join(file)).unwrap()
        );
    }
}

#[test]
fn interface_first_generation_expands_mixed_source_and_contract_inheritance_to_a_fixed_point() {
    let dir = TestDirectory::new("interfaces", "generation_mixed_inheritance");
    let base = dir.path().join("base.terl");
    let middle = dir.path().join("middle.terl");
    let child = dir.path().join("child.terli");
    let client = dir.path().join("client.terl");
    fs::write(&base, "module base. pub struct Base { code: Int }.").unwrap();
    fs::write(
        &middle,
        "module middle. import base.{Base}. pub struct Middle includes Base { message: String }.",
    )
    .unwrap();
    fs::write(
        &child,
        "module child. import middle.{Middle}. pub struct Child includes Middle { flag: Bool }.",
    )
    .unwrap();
    fs::write(
        &client,
        "module client. import child.{Child}. pub code(value: Child): Int -> value.code.",
    )
    .unwrap();
    let out = dir.path().join("out");
    assert_eq!(
        generate(&[&client, &child, &middle, &base], &out),
        ExitCode::SUCCESS
    );
    let (_, interface) = crate::terlan_hir::parse_interface_file(&out.join("child.typi")).unwrap();
    assert_eq!(
        interface.struct_fields["Child"]
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["code", "message", "flag"]
    );
    let reversed = dir.path().join("reversed");
    assert_eq!(
        generate(&[&base, &middle, &child, &client], &reversed),
        ExitCode::SUCCESS
    );
    assert_eq!(
        fs::read(out.join("child.typi")).unwrap(),
        fs::read(reversed.join("child.typi")).unwrap()
    );
}

#[test]
fn interface_first_generation_validates_external_contracts_from_other_source_directories() {
    let dir = TestDirectory::new("interfaces", "generation_external_inventory_validation");
    fs::create_dir_all(dir.path().join("provider")).unwrap();
    fs::create_dir_all(dir.path().join("client")).unwrap();
    let api = dir.path().join("provider/api.terli");
    let client = dir.path().join("client/client.terli");
    let dependency = dir.path().join("provider/dep.terli");
    fs::write(&api, "module api. pub value(): Int.").unwrap();
    fs::write(&client, "module client. pub token(): dep.Token.").unwrap();
    fs::write(&dependency, "module dep. pub opaque type Token.").unwrap();
    assert_eq!(
        generate(&[&client, &api], &dir.path().join("valid")),
        ExitCode::SUCCESS
    );
    fs::write(
        &dependency,
        "module dep. pub opaque type Token. pub invalid(): Missing.",
    )
    .unwrap();
    let invalid = dir.path().join("invalid");
    assert_ne!(generate(&[&client, &api], &invalid), ExitCode::SUCCESS);
    assert!(!invalid.join("client.typi").exists() && !invalid.join("api.typi").exists());
}

#[test]
fn interface_retains_included_public_fields_and_private_visibility() {
    let parent = parse_module_as_syntax_output(
        "module base.Errors. pub struct Error { code: Atom, message: String }.",
    )
    .unwrap();
    let interfaces = HashMap::from([(
        "base.Errors".into(),
        syntax_module_output_to_interface(&parent),
    )]);
    let child = parse_module_as_syntax_output(
        "module package.Parser. import type base.Errors.Error. pub struct ParseError includes Error { offset: Int, #source: String }.",
    ).unwrap();
    let interface = expanded_interface(&child, &interfaces).unwrap();
    let fields = &interface.struct_fields["ParseError"];
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["code", "message", "offset", "source"]
    );
    assert!(fields[3].is_private);
    let rendered = interface.to_terlan_interface_text();
    let reparsed = parse_module_as_syntax_output(&rendered).unwrap();
    assert_eq!(
        syntax_module_output_to_interface(&reparsed).struct_fields,
        interface.struct_fields
    );
}

#[test]
fn interface_rejects_conflicting_or_unknown_included_fields() {
    for source in [
        "module bad. pub struct Base { code: Atom }. pub struct Child includes Base { code: String }.",
        "module bad. pub struct Child includes Missing { offset: Int }.",
    ] {
        let syntax = parse_module_as_syntax_output(source).unwrap();
        let diagnostics = expanded_interface(&syntax, &HashMap::new()).unwrap_err();
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().all(|diagnostic| matches!(
            diagnostic.severity,
            crate::terlan_typeck::DiagSeverity::Error
        )));
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.message.is_empty())
        );
    }
}

/// Extraction must reject duplicate atoms before publishing any batch member.
#[test]
fn interface_extraction_rejects_duplicate_public_atom_aliases() {
    let dir = TestDirectory::new("interfaces", "duplicate_atom_aliases");
    let first = dir.path().join("first.terl");
    let second = dir.path().join("second.terl");
    let out = dir.path().join("out");
    fs::write(
        &first,
        r#"module first. pub type FlexEnd = Atom["flex-end"]."#,
    )
    .unwrap();
    fs::write(
        &second,
        r#"module second. pub type FlexEnd = Atom["flex-end"]."#,
    )
    .unwrap();
    assert_ne!(generate(&[&first, &second], &out), ExitCode::SUCCESS);
    assert!(!out.join("first.typi").exists());
    assert!(!out.join("second.typi").exists());
}
