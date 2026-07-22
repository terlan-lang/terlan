use super::imported_type_refs::collect_syntax_selected_type_refs;
use super::{
    load_interfaces_from_file_set, parse_interface_dependency_entries,
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface, ModuleInterface, TraitConformanceSource,
};
use crate::terlan_hir::{identifier_to_snake, source_name_to_terlan_identifier};
use crate::terlan_syntax::cached_canonical_terlan_syntax_contract;
use crate::terlan_syntax::canonical_terlan_syntax_contract;
use crate::terlan_syntax::ebnf::EbnfGrammarExprKind;
use crate::terlan_syntax::parse_interface_module_as_syntax_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_syntax::validate_syntax_contract;
use crate::terlan_syntax::SyntaxSourceKind;
use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn rendered_interface_docs_have_no_trailing_whitespace() {
    let module = parse_module_as_syntax_output(
        r#"/** Public module docs. */
module interface_doc_whitespace.

/** Public item docs. */
pub opaque type Item.
"#,
    )
    .expect("parse documented module");
    let rendered = syntax_module_output_to_interface(&module).to_terlan_interface_text();

    assert!(
        rendered.lines().all(|line| line == line.trim_end()),
        "rendered interface contains trailing whitespace:\n{rendered}"
    );
}

/// Verifies type-only imports support module-default type exports.
///
/// Inputs:
/// - A provider interface named `std.core.Task` that exports public opaque
///   type `Task`.
/// - A consumer module using `import std.core.Task.` and an aliased form
///   `import std.core.Task as AsyncTask.`.
///
/// Output:
/// - Test passes when both local type names resolve to provider type
///   `std.core.Task.Task`.
///
/// Transformation:
/// - Parses the consumer through syntax output, resolves it against the
///   provider interface map, and checks that the resolver collapses the
///   repeated module/type name only when the provider module exports the
///   matching default type.
#[test]
fn type_import_resolves_module_default_type_export() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module std.core.Task.\n\
\n\
pub opaque type Task[T].\n",
    )
    .expect("parse task provider interface");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "std.core.Task".to_string(),
        syntax_module_output_to_interface(&provider),
    );
    let consumer = parse_module_as_syntax_output(
        "\
module default_type_import_consumer.\n\
\n\
import std.core.Task.\n\
import std.core.Task as AsyncTask.\n\
\n\
pub identity(task: Task[Int]): AsyncTask[Int] ->\n\
    task.\n",
    )
    .expect("parse default type import consumer");

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;

    let task = resolved
        .imported_types
        .get("Task")
        .expect("default Task import");
    assert_eq!(task.source_module, "std.core.Task");
    assert_eq!(task.source_name, "Task");
    let async_task = resolved
        .imported_types
        .get("AsyncTask")
        .expect("aliased default Task import");
    assert_eq!(async_task.source_module, "std.core.Task");
    assert_eq!(async_task.source_name, "Task");
    assert!(
        resolved.diagnostics.is_empty(),
        "unexpected default type import diagnostics: {:?}",
        resolved.diagnostics
    );
}

/// Verifies wildcard imports expand public type and trait symbols.
///
/// Inputs:
/// - Provider interface with public/private types, a public opaque type, and a
///   public trait.
/// - Consumer using `import provider.Surface.{*}.`.
///
/// Output:
/// - Resolver imports only the provider's public type-like and trait surface.
///
/// Transformation:
/// - Expands the wildcard against the loaded interface before applying normal
///   duplicate and visibility import rules.
#[test]
fn wildcard_import_resolves_public_type_and_trait_surface() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.Surface.\n\
\n\
pub type User = Int.\n\
type Internal = Int.\n\
pub opaque type Token.\n\
pub trait VisibleTrait {}.\n",
    )
    .expect("parse wildcard provider interface");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "provider.Surface".to_string(),
        syntax_module_output_to_interface(&provider),
    );
    let consumer = parse_module_as_syntax_output(
        "\
module wildcard_import_consumer.\n\
\n\
import provider.Surface.{*}.\n\
\n\
pub identity(user: User): User ->\n\
    user.\n",
    )
    .expect("parse wildcard import consumer");

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;

    assert!(resolved.imported_types.contains_key("User"));
    assert!(resolved.imported_types.contains_key("Token"));
    assert!(!resolved.imported_types.contains_key("Internal"));
    assert!(resolved.imported_traits.contains_key("VisibleTrait"));
    assert!(
        resolved.diagnostics.is_empty(),
        "unexpected wildcard import diagnostics: {:?}",
        resolved.diagnostics
    );
}

/// Verifies pathological duplicate type imports diagnose deterministically.
///
/// Inputs:
/// - Two provider interfaces exporting the same public type name.
/// - A consumer importing both surfaces through wildcard imports.
///
/// Output:
/// - Test passes when HIR records a duplicate imported type diagnostic.
///
/// Transformation:
/// - Exercises adversarial import expansion where multiple providers attempt
///   to bind the same local type name.
#[test]
fn adversarial_hir_rejects_pathological_duplicate_wildcard_imports() {
    let primary = parse_interface_module_as_syntax_output(
        "\
module provider.Primary.\n\
\n\
pub type User = Int.\n",
    )
    .expect("parse primary provider interface");
    let secondary = parse_interface_module_as_syntax_output(
        "\
module provider.Secondary.\n\
\n\
pub type User = String.\n",
    )
    .expect("parse secondary provider interface");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "provider.Primary".to_string(),
        syntax_module_output_to_interface(&primary),
    );
    interfaces.insert(
        "provider.Secondary".to_string(),
        syntax_module_output_to_interface(&secondary),
    );
    let consumer = parse_module_as_syntax_output(
        "\
module adversarial_duplicate_imports.\n\
\n\
import provider.Primary.{*}.\n\
import provider.Secondary.{*}.\n\
\n\
pub identity(user: User): User ->\n\
    user.\n",
    )
    .expect("parse duplicate import consumer");

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;

    assert!(
        resolved.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("duplicate imported type name 'User'")),
        "expected duplicate imported type diagnostic, got {:?}",
        resolved.diagnostics
    );
}

/// Verifies test-layout `std` directories do not shadow root std summaries.
///
/// Inputs:
/// - A temporary workspace containing adjacent std test sources without
///   summaries.
/// - A root `std/summaries` directory containing `std_core_result.typi`.
/// - A source path under `std/core`.
///
/// Output:
/// - Test passes when `load_interfaces_from_file_set` still loads the root
///   stdlib summary.
///
/// Transformation:
/// - Builds the workspace on disk, runs normal interface discovery from an
///   adjacent std test source path, and removes the workspace afterward.
#[test]
fn std_interface_loading_handles_adjacent_std_test_source() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_hir_std_shadow_{}_{}",
        std::process::id(),
        nanos
    ));
    let test_core = root.join("std/core");
    let summaries = root.join("std/summaries");
    fs::create_dir_all(&test_core).expect("create test std fixture");
    fs::create_dir_all(&summaries).expect("create std summaries fixture");
    let source_path = test_core.join("result_test.terl");
    fs::write(&source_path, "module result_test.\n").expect("write test source fixture");
    fs::write(
        summaries.join("std_core_result.typi"),
        "\
module std_core_result.\n\
pub type Ok[T] = {:ok, T}.\n\
pub constructor Ok[T] {\n\
    (value: T): Ok[T] -> {:ok, value}\n\
}.\n",
    )
    .expect("write std summary fixture");

    let interfaces = load_interfaces_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
    );
    let _ = fs::remove_dir_all(&root);

    assert!(
        interfaces.contains_key("std_core_result"),
        "interfaces: {:?}",
        interfaces.keys().collect::<Vec<_>>()
    );
}

/// Verifies release collection summaries load through std discovery.
///
/// Inputs:
/// - A temporary workspace containing a `std/summaries` directory populated
///   from the release Map/List/Set `.typi` summaries.
/// - A source file path under the same temporary workspace.
///
/// Output:
/// - Test passes when `load_interfaces_from_file_set` discovers all three
///   collection interfaces and preserves receiver-method mutability.
///
/// Transformation:
/// - Writes release summaries into a throwaway std tree, runs the normal
///   interface discovery algorithm, and checks the resulting module
///   interfaces through the same path external projects use.
#[test]
fn std_interface_loading_discovers_release_core_collection_contracts() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_hir_collection_summaries_{}_{}",
        std::process::id(),
        nanos
    ));
    let source_dir = root.join("src/app");
    let summaries = root.join("std/summaries");
    fs::create_dir_all(&source_dir).expect("create source fixture");
    fs::create_dir_all(&summaries).expect("create summaries fixture");
    let source_path = source_dir.join("Main.terl");
    fs::write(&source_path, "module app.Main.\n").expect("write source fixture");

    for (file_name, text) in [
        (
            "std.collections.Map.typi",
            include_str!("../../../../../std/summaries/std.collections.Map.typi"),
        ),
        (
            "std.collections.List.typi",
            include_str!("../../../../../std/summaries/std.collections.List.typi"),
        ),
        (
            "std.collections.Set.typi",
            include_str!("../../../../../std/summaries/std.collections.Set.typi"),
        ),
    ] {
        fs::write(summaries.join(file_name), text)
            .unwrap_or_else(|err| panic!("write {file_name}: {err}"));
    }

    let interfaces = load_interfaces_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
    );
    let _ = fs::remove_dir_all(&root);

    assert_collection_summary_signature(
        &interfaces,
        "std.collections.Map",
        "put",
        3,
        "Unit",
        "map",
        "Map[K, V]",
        true,
        true,
    );
    assert_collection_summary_signature(
        &interfaces,
        "std.collections.List",
        "clear",
        1,
        "Unit",
        "list",
        "List[T]",
        true,
        true,
    );
    assert_collection_summary_signature(
        &interfaces,
        "std.collections.Set",
        "add",
        2,
        "Unit",
        "set",
        "Set[T]",
        true,
        true,
    );
}

/// Verifies release iterator/iterable summaries load through std discovery.
///
/// Inputs:
/// - A temporary workspace containing a `std/summaries` directory populated
///   from the release Iterator/Iterable `.typi` summaries.
/// - A source file path under the same temporary workspace.
///
/// Output:
/// - Test passes when `load_interfaces_from_file_set` discovers both
///   interfaces and preserves `Iterator.next` plus `Iterable.iterator`.
///
/// Transformation:
/// - Writes release summaries into a throwaway std tree, runs normal
///   interface discovery, and checks the resulting module interfaces
///   from the checked-in release std summaries.
#[test]
fn std_interface_loading_discovers_release_traversal_contracts() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_hir_collection_trait_summaries_{}_{}",
        std::process::id(),
        nanos
    ));
    let source_dir = root.join("src/app");
    let summaries = root.join("std/summaries");
    fs::create_dir_all(&source_dir).expect("create source fixture");
    fs::create_dir_all(&summaries).expect("create summaries fixture");
    let source_path = source_dir.join("Main.terl");
    fs::write(&source_path, "module app.Main.\n").expect("write source fixture");

    for (file_name, text) in [
        (
            "std.collections.Iterator.typi",
            include_str!("../../../../../std/summaries/std.collections.Iterator.typi"),
        ),
        (
            "std.collections.Iterable.typi",
            include_str!("../../../../../std/summaries/std.collections.Iterable.typi"),
        ),
    ] {
        fs::write(summaries.join(file_name), text)
            .unwrap_or_else(|err| panic!("write {file_name}: {err}"));
    }

    let interfaces = load_interfaces_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
    );
    let _ = fs::remove_dir_all(&root);

    assert_collection_summary_signature(
        &interfaces,
        "std.collections.Iterator",
        "next",
        1,
        "Option[Step[T]]",
        "iterator",
        "Iterator[T]",
        false,
        false,
    );
    assert_trait_method_signature(
        &interfaces,
        "std.collections.Iterable",
        "Iterable",
        "iterator",
        "std.collections.Iterator.Iterator[T]",
        "collection",
        "C",
    );
}

/// Asserts one loaded collection summary function signature.
///
/// Inputs:
/// - `interfaces`: discovered module interfaces keyed by module name.
/// - `module_name`: expected collection module name.
/// - `function_name`: expected function/method name.
/// - `arity`: expected receiver-first callable arity.
/// - `return_type`: expected normalized return type text.
/// - `receiver_name`: expected receiver parameter name.
/// - `receiver_type`: expected normalized receiver annotation text.
/// - `receiver_method`: expected receiver-method syntax marker.
/// - `receiver_mutable`: expected receiver mutability marker.
///
/// Output:
/// - Panics when the interface, function, return type, receiver-first
///   parameter shape, or receiver mutability does not match.
///
/// Transformation:
/// - Reads a function signature from an already loaded interface and
///   compares the receiver-first shape plus mutability metadata used by
///   downstream compiler phases.
fn assert_collection_summary_signature(
    interfaces: &HashMap<String, ModuleInterface>,
    module_name: &str,
    function_name: &str,
    arity: usize,
    return_type: &str,
    receiver_name: &str,
    receiver_type: &str,
    receiver_method: bool,
    receiver_mutable: bool,
) {
    let interface = interfaces
        .get(module_name)
        .unwrap_or_else(|| panic!("missing interface {module_name}"));
    let signature = interface
        .functions
        .get(&(function_name.to_string(), arity))
        .unwrap_or_else(|| panic!("missing signature {module_name}.{function_name}/{arity}"));

    assert_eq!(signature.return_type, return_type);
    assert_eq!(signature.params[0].name, receiver_name);
    assert_eq!(signature.params[0].annotation, receiver_type);
    assert_eq!(signature.receiver_method, receiver_method);
    assert_eq!(signature.receiver_mutable, receiver_mutable);
}

/// Asserts one loaded trait method signature.
///
/// Inputs:
/// - `interfaces`: discovered module interfaces keyed by module name.
/// - `module_name`: expected module containing the trait.
/// - `trait_name`: expected trait name.
/// - `method_name`: expected trait method name.
/// - `return_type`: expected normalized method return type.
/// - `param_name`: expected first parameter name.
/// - `param_type`: expected first parameter annotation.
///
/// Output:
/// - Panics when the interface, trait, method, return type, or parameter
///   shape does not match.
///
/// Transformation:
/// - Reads a trait method signature from an already loaded interface and
///   compares the shape used by downstream conformance checks.
fn assert_trait_method_signature(
    interfaces: &HashMap<String, ModuleInterface>,
    module_name: &str,
    trait_name: &str,
    method_name: &str,
    return_type: &str,
    param_name: &str,
    param_type: &str,
) {
    let interface = interfaces
        .get(module_name)
        .unwrap_or_else(|| panic!("missing interface {module_name}"));
    let trait_signature = interface
        .traits
        .get(trait_name)
        .unwrap_or_else(|| panic!("missing trait {module_name}.{trait_name}"));
    let method = trait_signature
        .methods
        .get(method_name)
        .unwrap_or_else(|| panic!("missing trait method {trait_name}.{method_name}"));

    assert_eq!(method.return_type, return_type);
    assert_eq!(method.params[0].name, param_name);
    assert_eq!(method.params[0].annotation, param_type);
}

/// Verifies interface snapshots preserve public trait conformance facts.
///
/// Inputs:
/// - A source module containing one declaration-site `implements`
///   conformance and one explicit `impl Trait[...] for Type` conformance.
///
/// Output:
/// - Test passes when both conformance facts appear in the direct interface
///   and survive rendering/parsing as `.typi` interface text.
///
/// Transformation:
/// - Converts syntax output to `ModuleInterface`, renders it as interface
///   text, reparses that text through the interface parser, and converts it
///   back to `ModuleInterface` to prove the metadata is stable.
#[test]
fn interface_rendering_preserves_public_trait_conformances() {
    let module = parse_module_as_syntax_output(
        "\
module interface_trait_conformance.\n\
\n\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub type User implements Show[User] = {name: String}.\n\
\n\
pub impl Show[Int] for Int {\n\
    show(value: Int): String ->\n\
        \"int\".\n\
}.\n\
\n\
pub opaque type Secret.\n\
pub impl not Show[Secret].\n\
impl not Show[String].\n",
    )
    .expect("parse conformance source fixture");

    let interface = syntax_module_output_to_interface(&module);
    assert_trait_conformance(
        &interface,
        "Show[User]",
        "User",
        TraitConformanceSource::Implements,
        false,
    );
    assert_trait_conformance(
        &interface,
        "Show[Int]",
        "Int",
        TraitConformanceSource::ExplicitImpl,
        false,
    );
    assert_trait_conformance(
        &interface,
        "Show",
        "Secret",
        TraitConformanceSource::ExplicitImpl,
        true,
    );

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("pub impl Show[User] for User"),
        "rendered interface should preserve declaration-site conformance:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub impl Show[Int] for Int"),
        "rendered interface should preserve explicit impl conformance:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub impl not Show[Secret]."),
        "rendered interface should preserve public negative conformance:\n{}",
        rendered
    );
    assert!(
        !rendered.contains("impl not Show[String]"),
        "rendered interface should exclude private negative conformance:\n{}",
        rendered
    );

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered conformance interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert_trait_conformance(
        &reparsed_interface,
        "Show[User]",
        "User",
        TraitConformanceSource::ExplicitImpl,
        false,
    );
    assert_trait_conformance(
        &reparsed_interface,
        "Show[Int]",
        "Int",
        TraitConformanceSource::ExplicitImpl,
        false,
    );
    assert_trait_conformance(
        &reparsed_interface,
        "Show",
        "Secret",
        TraitConformanceSource::ExplicitImpl,
        true,
    );
}

/// Verifies interface rendering qualifies default-imported impl type heads.
///
/// Inputs:
/// - A module that imports `std.collections.List.` through default-export
///   syntax and implements a higher-kinded trait for `List`.
///
/// Output:
/// - Generated interface text uses `std.collections.List.List`, the exported
///   type constructor, rather than the module path `std.collections.List`.
///
/// Transformation:
/// - Exercises syntax-output import selection metadata during conformance
///   extraction so generated `.typi` summaries preserve HKT kind arity.
#[test]
fn interface_rendering_qualifies_default_imported_hkt_impl_type_heads() {
    let module = parse_module_as_syntax_output(
        "\
module default_imported_hkt_impl.\n\
\n\
import std.collections.List.\n\
import type std.collections.List.\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub impl Functor[List] for List {\n\
    map(value: List[A], f: (A) -> B): List[B] ->\n\
        value.\n\
}.\n",
    )
    .expect("parse default imported hkt impl");

    let interface = syntax_module_output_to_interface(&module);
    let rendered = interface.to_terlan_interface_text();

    assert!(
        rendered
            .contains("pub impl Functor[std.collections.List.List] for std.collections.List.List"),
        "rendered interface should qualify the default imported type constructor:\n{}",
        rendered
    );
}

#[test]
fn interface_rendering_qualifies_selected_types_in_function_signatures() {
    let module = parse_module_as_syntax_output(
        "module selected_type_signature.\n\
import type std.core.Ordering.{Comparison}.\n\
pub compare_with(callback: (Int, Int) -> Comparison): Comparison ->\n\
    callback(1, 2).\n",
    )
    .expect("parse selected type signature");

    let rendered = syntax_module_output_to_interface(&module).to_terlan_interface_text();

    assert!(
        rendered.contains(
            "pub compare_with(callback: (Int, Int) -> std.core.Ordering.Comparison): std.core.Ordering.Comparison."
        ),
        "rendered interface should qualify selected types in nested and return positions:\n{rendered}"
    );
}

#[test]
fn interface_rendering_preserves_collapsed_module_default_type_shorthand() {
    let module = parse_module_as_syntax_output(
        "module collapsed_type_signature.\n\
import type std.core.{Option}.\n\
import std.core.Result.{Err, Result}.\n\
pub keep(value: Option[Int]): Option[Int] ->\n\
    value.\n\
pub keep_result(value: Result[Int, String]): Result[Int, String] ->\n\
    value.\n",
    )
    .expect("parse collapsed module type signature");

    let rendered = syntax_module_output_to_interface(&module).to_terlan_interface_text();

    assert!(
        rendered.contains("pub keep(value: Option[Int]): Option[Int]."),
        "collapsed module-default shorthand must remain resolver-owned:\n{rendered}"
    );
    assert!(
        rendered.contains("pub keep_result(value: Result[Int, String]): Result[Int, String]."),
        "selected module-default shorthand must remain resolver-owned:\n{rendered}"
    );
}

#[test]
fn interface_summary_without_imports_has_no_selected_type_references() {
    let module = parse_interface_module_as_syntax_output(
        "module summary_without_imports.\npub value(): Unit.\n",
    )
    .expect("parse import-free summary");

    let imported = collect_syntax_selected_type_refs(&module);

    assert!(
        imported.is_empty(),
        "import-free interface summary synthesized selected type references: {imported:?}"
    );
    let interface = syntax_module_output_to_interface(&module);
    assert_eq!(
        interface.functions[&("value".to_string(), 0)].return_type,
        "Unit"
    );
}

/// Verifies interface rendering preserves trait default-method markers.
///
/// Inputs:
/// - A public trait with one required method and one default method.
///
/// Output:
/// - Test passes when direct and rendered/reparsed interfaces mark only the
///   default method as having a default implementation.
///
/// Transformation:
/// - Converts source syntax to an interface, renders the `.typi` summary
///   with a placeholder default body, reparses that summary, and verifies
///   downstream interface extraction still sees the default marker.
#[test]
fn interface_rendering_preserves_trait_default_method_markers() {
    let module = parse_module_as_syntax_output(
        "\
module interface_trait_defaults.\n\
\n\
pub trait Lifecycle[T] {\n\
    start(value: T): T.\n\
    stop(value: T): Unit -> Unit.\n\
}.\n",
    )
    .expect("parse default trait method source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let lifecycle = interface
        .traits
        .get("Lifecycle")
        .expect("direct lifecycle trait");
    assert!(!lifecycle.methods["start"].has_default);
    assert!(lifecycle.methods["stop"].has_default);

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("stop(value: T): Unit ->"),
        "rendered summary should contain a placeholder default body:\n{}",
        rendered
    );

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered default trait interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let reparsed_lifecycle = reparsed_interface
        .traits
        .get("Lifecycle")
        .expect("reparsed lifecycle trait");
    assert!(!reparsed_lifecycle.methods["start"].has_default);
    assert!(reparsed_lifecycle.methods["stop"].has_default);
}
