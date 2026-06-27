use super::{
    load_interfaces_from_file_set, resolve_syntax_module_output,
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    ModuleInterface, TraitConformanceSource,
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
}.\n",
    )
    .expect("parse conformance source fixture");

    let interface = syntax_module_output_to_interface(&module);
    assert_trait_conformance(
        &interface,
        "Show[User]",
        "User",
        TraitConformanceSource::Implements,
    );
    assert_trait_conformance(
        &interface,
        "Show[Int]",
        "Int",
        TraitConformanceSource::ExplicitImpl,
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

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered conformance interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert_trait_conformance(
        &reparsed_interface,
        "Show[User]",
        "User",
        TraitConformanceSource::ExplicitImpl,
    );
    assert_trait_conformance(
        &reparsed_interface,
        "Show[Int]",
        "Int",
        TraitConformanceSource::ExplicitImpl,
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

/// Verifies interface rendering preserves same-name same-arity overloads.
///
/// Inputs:
/// - A provider interface with two public `pick/1` signatures distinguished by
///   parameter and return type.
///
/// Output:
/// - Test passes when rendered `.typi` text contains both overloads and the
///   reparsed interface stores both candidates in `function_overloads`.
///
/// Transformation:
/// - Converts interface syntax to `ModuleInterface`, renders it to summary
///   text, reparses the summary, and inspects overload metadata without relying
///   on the compatibility single-signature map.
#[test]
fn interface_rendering_preserves_function_overloads() {
    let source = "\
module overload.Provider.\n\
\n\
pub pick(value: Int): Int.\n\
pub pick(value: String): String.\n\
";
    let parsed = parse_interface_module_as_syntax_output(source).expect("parse overload interface");
    let interface = syntax_module_output_to_interface(&parsed);
    let rendered = interface.to_terlan_interface_text();

    assert!(
        rendered.contains("pub pick(value: Int): Int."),
        "rendered interface should contain Int overload:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub pick(value: String): String."),
        "rendered interface should contain String overload:\n{}",
        rendered
    );

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered overload interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let overloads = reparsed_interface
        .function_overloads
        .get(&("pick".to_string(), 1))
        .expect("pick/1 overloads");

    assert_eq!(overloads.len(), 2);
    assert!(overloads
        .iter()
        .any(|signature| signature.return_type == "Int"));
    assert!(overloads
        .iter()
        .any(|signature| signature.return_type == "String"));
}

/// Verifies resolved source modules preserve implemented overloads in summaries.
///
/// Inputs:
/// - A source module with two implemented public `pick/1` overloads
///   distinguished by parameter type.
///
/// Output:
/// - Test passes when the resolved interface summary text contains both
///   overload signatures.
///
/// Transformation:
/// - Parses ordinary source syntax, resolves it through HIR, then renders the
///   module interface exactly as std summary generation does.
#[test]
fn resolved_interface_rendering_preserves_source_function_overloads() {
    let source = "\
module overload.SourceProvider.\n\
\n\
pub pick(value: Int): String ->\n\
    value.to_string().\n\
\n\
pub pick(value: String): String ->\n\
    value.\n\
";
    let parsed = parse_module_as_syntax_output(source).expect("parse overload source");
    let resolved = resolve_syntax_module_output(&parsed);
    let rendered = resolved.module.interface.to_terlan_interface_type_text();

    assert!(
        rendered.contains("pub pick(value: Int): String."),
        "rendered source interface should contain Int overload:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub pick(value: String): String."),
        "rendered source interface should contain String overload:\n{}",
        rendered
    );
}

/// Verifies public struct fields survive `.typi` rendering and parsing.
///
/// Inputs:
/// - A syntax-output module containing one public struct with two fields.
///
/// Output:
/// - Test passes when direct and reparsed interfaces both expose the public
///   struct field signatures.
///
/// Transformation:
/// - Converts source to interface metadata, renders that metadata as
///   Terlan interface text, reparses it, and compares the resulting
///   span-free field signatures.
#[test]
fn interface_rendering_preserves_public_struct_fields() {
    let module = parse_module_as_syntax_output(
        "\
module interface_struct_fields.\n\
\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String,\n\
    #internal_id: String\n\
}.\n",
    )
    .expect("parse struct field source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let fields = interface
        .struct_fields
        .get("Error")
        .expect("direct struct field metadata");
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name, "code");
    assert_eq!(fields[0].annotation, "Atom");
    assert!(!fields[0].is_private);
    assert_eq!(fields[1].name, "message");
    assert_eq!(fields[1].annotation, "String");
    assert!(!fields[1].is_private);
    assert_eq!(fields[2].name, "internal_id");
    assert_eq!(fields[2].annotation, "String");
    assert!(fields[2].is_private);

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("pub struct Error"),
        "rendered interface should preserve struct declaration:\n{}",
        rendered
    );
    assert!(
        rendered.contains("#internal_id: String"),
        "rendered interface should preserve private field syntax:\n{}",
        rendered
    );
    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered struct interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let reparsed_fields = reparsed_interface
        .struct_fields
        .get("Error")
        .expect("reparsed struct field metadata");
    assert_eq!(reparsed_fields, fields);
}

/// Verifies callable default parameters survive interface generation.
///
/// Inputs:
/// - A source module containing a public function, receiver method, trait
///   method, and constructor with defaulted parameters.
///
/// Output:
/// - Test passes when generated interface text renders the defaults and the
///   reparsed interface keeps the same default metadata.
///
/// Transformation:
/// - Converts source syntax output to HIR interface metadata, renders a `.typi`
///   compatible summary, reparses it as interface source, and checks that
///   default parameter text remains available to downstream phases.
#[test]
fn interface_rendering_preserves_callable_parameter_defaults() {
    let module = parse_module_as_syntax_output(
        "\
module interface_callable_defaults.\n\
\n\
pub type User = {name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (name: String, active: Bool = True): User ->\n\
        User(name, active)\n\
}.\n\
\n\
pub trait Label[T] {\n\
    label(value: T, separator: String = \":\"): String.\n\
}.\n\
\n\
pub greet(name: String, excited: Bool = False): String ->\n\
    name.\n\
\n\
pub (name: String) pad(width: Int = 2): String ->\n\
    name.\n",
    )
    .expect("parse callable defaults source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let rendered = interface.to_terlan_interface_type_text();
    assert!(
        rendered.contains("pub greet(name: String, excited: Bool = False): String."),
        "rendered interface should preserve function defaults:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub (name: String) pad(width: Int = 2): String."),
        "rendered interface should preserve receiver-method defaults:\n{}",
        rendered
    );
    assert!(
        rendered.contains("label(value: T, separator: String = \":\"): String."),
        "rendered interface should preserve trait-method defaults:\n{}",
        rendered
    );
    assert!(
        rendered.contains("(name: String, active: Bool = True): User"),
        "rendered interface should preserve constructor defaults:\n{}",
        rendered
    );

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered callable default interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let greet = reparsed_interface
        .functions
        .get(&("greet".to_string(), 2))
        .expect("reparsed greet signature");
    assert_eq!(greet.params[1].default_text.as_deref(), Some("False"));

    let pad = reparsed_interface
        .functions
        .get(&("pad".to_string(), 2))
        .expect("reparsed receiver method signature");
    assert_eq!(pad.params[1].default_text.as_deref(), Some("2"));

    let label = reparsed_interface
        .traits
        .get("Label")
        .and_then(|signature| signature.methods.get("label"))
        .expect("reparsed label trait method");
    assert_eq!(label.params[1].default_text.as_deref(), Some("\":\""));

    let constructor = reparsed_interface
        .constructors
        .get("User")
        .and_then(|signatures| signatures.first())
        .expect("reparsed User constructor");
    assert_eq!(constructor.params[1].default_text.as_deref(), Some("True"));
}

/// Verifies generated provider summaries with constructors and empty impls parse.
///
/// Inputs:
/// - Interface text matching a cached provider `.typi` summary for a module
///   with a public struct, constructor, trait, explicit impl, and function.
///
/// Output:
/// - Test passes when interface parsing and HIR extraction preserve the
///   provider module interface.
///
/// Transformation:
/// - Parses generated interface text and converts it back into a
///   `ModuleInterface`, catching cache summary shapes that would otherwise
///   be silently skipped by interface loading.
#[test]
fn generated_provider_interface_with_empty_impl_parses() {
    let source = "\
module people.Provider.\n\
\n\
pub type ExternalUser.\n\
\n\
pub new_user(name: String): ExternalUser.\n\
\n\
pub trait Named[T] {\n\
    name(value: T): String.\n\
}.\n\
\n\
pub impl Named[ExternalUser] for ExternalUser {\n\
}.\n\
\n\
pub constructor ExternalUser {\n\
    (name: String): ExternalUser ->\n\
        terlan_interface_constructor\n\
}.\n";

    let parsed = parse_interface_module_as_syntax_output(source)
        .expect("parse generated provider interface summary");
    let interface = syntax_module_output_to_interface(&parsed);

    assert_eq!(interface.module, "people.Provider");
    assert!(interface.public_types.contains("ExternalUser"));
    assert!(interface.traits.contains_key("Named"));
    assert_eq!(interface.trait_conformances.len(), 1);
    assert!(interface
        .functions
        .contains_key(&("new_user".to_string(), 1)));
}

/// Asserts one trait conformance fact exists in an interface snapshot.
///
/// Inputs:
/// - `interface`: module interface to inspect.
/// - `trait_ref`: expected normalized trait reference.
/// - `for_type`: expected normalized implementation type.
/// - `source`: expected conformance source category.
///
/// Output:
/// - Panics when the conformance fact is missing.
///
/// Transformation:
/// - Performs an exact metadata lookup without inspecting source text.
fn assert_trait_conformance(
    interface: &ModuleInterface,
    trait_ref: &str,
    for_type: &str,
    source: TraitConformanceSource,
) {
    assert!(
        interface.trait_conformances.iter().any(|conformance| {
            conformance.trait_ref == trait_ref
                && conformance.for_type == for_type
                && conformance.source == source
                && conformance.public
        }),
        "missing conformance {trait_ref} for {for_type} via {:?}: {:?}",
        source,
        interface.trait_conformances
    );
}

/// Verifies exact duplicate function shapes remain invalid.
///
/// Inputs:
/// - A source module declaring two `pick/1` functions with the same parameter
///   and return annotations.
///
/// Output:
/// - Test passes when HIR reports a duplicate function definition diagnostic.
///
/// Transformation:
/// - Parses canonical source, resolves syntax output to HIR, and checks that
///   overload relaxation only applies to distinct type shapes.
#[test]
fn hir_rejects_duplicate_function_shape() {
    let module = parse_module_as_syntax_output(
        "\
module duplicate_function_shape.\n\
\n\
pub pick(value: Int): Int ->\n\
    value.\n\
\n\
pub pick(other: Int): Int ->\n\
    other.\n\
",
    )
    .expect("parse duplicate function shape fixture");
    let resolved = resolve_syntax_module_output(&module).module;

    assert!(
        resolved.diagnostics.iter().any(|diag| diag
            .message
            .contains("duplicate function definition: pick / 1")),
        "expected duplicate diagnostic, got {:?}",
        resolved.diagnostics
    );
}

#[test]
fn hir_accepts_canonical_syntax_contract() {
    let contract =
        cached_canonical_terlan_syntax_contract().expect("cached canonical syntax contract");

    let diagnostics = validate_syntax_contract(contract);
    assert!(
        diagnostics.is_empty(),
        "unexpected syntax contract diagnostics: {diagnostics:?}"
    );
}

#[test]
fn hir_rejects_broken_syntax_contract() {
    let mut contract =
        canonical_terlan_syntax_contract().expect("compile canonical syntax contract");
    contract.entry_rule = Some("Program".to_string());
    let expr_rule = contract.rule("Expr").expect("Expr rule").clone();
    let expr_rule_index = contract
        .rules
        .iter()
        .position(|rule| rule.name == expr_rule.name)
        .expect("Expr rule index");
    contract.rules[expr_rule_index].expr.kind = EbnfGrammarExprKind::Terminal {
        value: "broken".to_string(),
    };

    let diagnostics = validate_syntax_contract(&contract);
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("entry rule")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "syntax rule Expr must reference AssignExpr"));
}

#[test]
fn resolve_syntax_output_records_function_symbols() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
module syntax_resolve.

pub add(Value: Int): Int ->
    Value + 1.
"#,
    )
    .expect("parse syntax output");

    let resolved = resolve_syntax_module_output(&syntax_module);
    let symbol = resolved
        .module
        .function_symbols
        .get(&("add".to_string(), 1))
        .expect("add symbol");
    assert_eq!(symbol.return_type, "Int");
    assert!(symbol.exported);
    assert!(resolved.module.diagnostics.is_empty());
}

#[test]
fn resolve_syntax_output_rejects_source_export_payloads() {
    let mut syntax_module = crate::terlan_syntax::parse_interface_module_as_syntax_output(
        r#"
module syntax_resolve_source_export_payload.

export add/1.
"#,
    )
    .expect("parse interface syntax output");
    syntax_module.source_kind = SyntaxSourceKind::Module;

    let resolved = resolve_syntax_module_output(&syntax_module);
    assert!(resolved
        .module
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic
            .message
            .contains("source export declarations are not part of canonical Terlan")));
    assert!(!resolved
        .module
        .function_symbols
        .contains_key(&("add".to_string(), 1)));
}

/// Verifies source-tree resolution rejects source-mode export
/// payloads.
///
/// Inputs:
/// - An interface-parsed AST module containing an `Export` payload.
///
/// Output:
/// - Test passes when AST resolution reports the canonical source-export
///   diagnostic and does not create a function symbol from the interface
///   export summary.
///
/// Transformation:
/// - Feeds an interface export-summary AST payload through the source-oriented
///   compatibility resolver to prove it no longer treats `export` as a
///   normal source visibility mechanism.
#[test]
fn formal_hir_syntax_output_resolves_interface_surface() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
//! Public docs.
module formal_syntax_iface.

/// Item collection.
pub type Items[T] =
    List[T].

/// Builds item collections.
pub constructor Items[T] {
    (Values: List[T]): Items[T] ->
        Values
}.

/// Shows values.
	pub trait Show[A] {
	  /// Converts to text.
	  show(Value: A): Text.
	}.
	
	/// Adds one.
	pub add(Value: Int): Int ->
	    Value + 1.
"#,
    )
    .expect("parse syntax output");

    let interface = syntax_module_output_to_interface(&syntax_module);
    assert_eq!(interface.module, "formal_syntax_iface");
    assert_eq!(interface.docs, vec!["Public docs."]);
    assert!(interface.public_types.contains("Items"));
    assert_eq!(
        interface.type_params.get("Items"),
        Some(&vec!["T".to_string()])
    );
    assert_eq!(interface.constructors.get("Items").map(Vec::len), Some(1));
    assert_eq!(
        interface.traits["Show"].methods["show"].docs,
        vec!["Converts to text."]
    );
    assert_eq!(
        interface.functions[&("add".to_string(), 1)].return_type,
        "Int"
    );

    let resolved = resolve_syntax_module_output(&syntax_module);
    let symbol = resolved
        .module
        .function_symbols
        .get(&("add".to_string(), 1))
        .expect("add symbol");
    assert!(symbol.exported);
    assert!(resolved.module.diagnostics.is_empty());
}

/// Verifies higher-kinded type parameters survive HIR interface extraction.
///
/// Inputs:
/// - Public trait syntax with a unary higher-kinded parameter.
///
/// Output:
/// - Interface metadata and rendered interface text preserving `F[_]`.
///
/// Transformation:
/// - Parses source through syntax output, lowers to HIR interface metadata, and
///   renders the importable `.typi` text without erasing kind arity.
#[test]
fn formal_hir_preserves_higher_kinded_trait_params() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
module formal_hkt_iface.

pub trait Functor[F[_]] {
    map(value: F[T], fn: (T) -> U): F[U].
}.
"#,
    )
    .expect("parse hkt syntax output");

    let interface = syntax_module_output_to_interface(&syntax_module);
    assert_eq!(
        interface.traits["Functor"].type_params,
        vec!["F[_]".to_string()]
    );
    assert!(interface
        .to_terlan_interface_text()
        .contains("pub trait Functor[F[_]]"));
}

/// Verifies higher-kinded callable params survive HIR interface extraction.
///
/// Inputs:
/// - Public generic function syntax with a unary HKT parameter and a generic
///   bound.
///
/// Output:
/// - Interface metadata and rendered interface text preserving `F[_]`, `A`,
///   and the generic bound text.
///
/// Transformation:
/// - Parses source through syntax output, lowers to HIR interface metadata, and
///   renders the importable `.typi` text without erasing callable generic
///   parameters.
#[test]
fn formal_hir_preserves_higher_kinded_function_params() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
module formal_hkt_function_iface.

pub trait Show[T] {
    show(value: T): String.
}.

pub identity[F[_], A]<A: Show>(value: F[A]): F[A] ->
    value.

pub trait Functor[F[_]] {
    map[A, B](value: F[A], f: (A) -> B): F[B].
}.
"#,
    )
    .expect("parse hkt function syntax output");

    let interface = syntax_module_output_to_interface(&syntax_module);
    let signature = interface
        .functions
        .get(&("identity".to_string(), 1))
        .expect("identity signature");

    assert_eq!(signature.generic_params, vec!["F[_]", "A"]);
    assert_eq!(signature.generic_bounds, vec!["A : Show"]);
    assert!(interface
        .to_terlan_interface_text()
        .contains("pub identity[F[_], A]<A : Show>(value: F[A]): F[A]."));
    assert!(interface
        .to_terlan_interface_text()
        .contains("map[A, B](value: F[A], f: (A ) -> B): F[B]."));
}

/// Verifies release core collection contracts produce stable interfaces.
///
/// Inputs:
/// - Release source contracts for `std.collections.Map`, `std.collections.List`, and
///   `std.collections.Set`.
/// - Matching release `.typi` summaries using bodyless receiver method
///   signatures.
///
/// Output:
/// - Test passes when source-contract extraction and summary parsing expose
///   the same key function arities, return types, and receiver mutability.
///
/// Transformation:
/// - Converts source and summary receiver methods into HIR's callable
///   `method(receiver, args...)` convention while preserving `mut`.
#[test]
fn hir_extracts_release_core_collection_contracts_as_receiver_first_interfaces() {
    let contracts = [
        (
            "std.collections.Map",
            include_str!("../../../../../std/collections/map.terl"),
            include_str!("../../../../../std/summaries/std.collections.Map.typi"),
            vec![
                ("put", 3, "Unit", "map", "Map[K, V]", true),
                ("remove", 2, "Unit", "map", "Map[K, V]", true),
                ("clear", 1, "Unit", "map", "Map[K, V]", true),
            ],
        ),
        (
            "std.collections.List",
            include_str!("../../../../../std/collections/list.terl"),
            include_str!("../../../../../std/summaries/std.collections.List.typi"),
            vec![
                ("push", 2, "Unit", "list", "List[T]", true),
                ("clear", 1, "Unit", "list", "List[T]", true),
            ],
        ),
        (
            "std.collections.Set",
            include_str!("../../../../../std/collections/set.terl"),
            include_str!("../../../../../std/summaries/std.collections.Set.typi"),
            vec![
                ("add", 2, "Unit", "set", "Set[T]", true),
                ("remove", 2, "Unit", "set", "Set[T]", true),
                ("clear", 1, "Unit", "set", "Set[T]", true),
            ],
        ),
    ];

    for (module_name, source, summary, expected_functions) in contracts {
        let source_module =
            parse_module_as_syntax_output(source).expect("parse release collection source");
        let summary_module = parse_interface_module_as_syntax_output(summary)
            .expect("parse release collection summary");
        let source_interface = syntax_module_output_to_interface(&source_module);
        let summary_interface = syntax_module_output_to_interface(&summary_module);

        assert_eq!(source_interface.module, module_name);
        assert_eq!(summary_interface.module, module_name);

        for (function_name, arity, return_type, receiver_name, receiver_type, mutable) in
            expected_functions
        {
            let key = (function_name.to_string(), arity);
            let source_signature = source_interface
                .functions
                .get(&key)
                .unwrap_or_else(|| panic!("missing source signature {module_name}.{key:?}"));
            let summary_signature = summary_interface
                .functions
                .get(&key)
                .unwrap_or_else(|| panic!("missing summary signature {module_name}.{key:?}"));

            assert_eq!(source_signature.return_type, return_type);
            assert_eq!(summary_signature.return_type, return_type);
            assert_eq!(source_signature.params[0].name, receiver_name);
            assert_eq!(summary_signature.params[0].name, receiver_name);
            assert_eq!(source_signature.params[0].annotation, receiver_type);
            assert_eq!(summary_signature.params[0].annotation, receiver_type);
            assert!(source_signature.receiver_method);
            assert!(summary_signature.receiver_method);
            assert_eq!(source_signature.receiver_mutable, mutable);
            assert_eq!(summary_signature.receiver_mutable, mutable);
        }
    }
}

/// Verifies release iterator/iterable contracts produce stable interfaces.
///
/// Inputs:
/// - Release interface contracts for `std.collections.Iterator` and
///   `std.collections.Iterable`.
/// - Matching release `.typi` summaries.
///
/// Output:
/// - Test passes when source-contract extraction and summary parsing expose
///   the same key function and trait method signatures.
///
/// Transformation:
/// - Converts release interface syntax into HIR module interfaces and
///   compares those interfaces with the bodyless summaries planned for
///   later compiler phases.
#[test]
fn hir_extracts_release_traversal_contracts_as_interfaces() {
    let iterator_source =
        parse_module_as_syntax_output(include_str!("../../../../../std/collections/iterator.terl"))
            .expect("parse iterator source contract");
    let iterator_summary = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../std/summaries/std.collections.Iterator.typi"
    ))
    .expect("parse iterator summary");
    let iterator_source_interface = syntax_module_output_to_interface(&iterator_source);
    let iterator_summary_interface = syntax_module_output_to_interface(&iterator_summary);

    assert_eq!(iterator_source_interface.module, "std.collections.Iterator");
    assert_eq!(
        iterator_summary_interface.module,
        "std.collections.Iterator"
    );
    assert_eq!(
        iterator_source_interface.functions[&("next".to_string(), 1)].return_type,
        "Option[Step[T]]"
    );
    assert_eq!(
        iterator_summary_interface.functions[&("next".to_string(), 1)].return_type,
        "Option[Step[T]]"
    );

    let iterable_source =
        parse_module_as_syntax_output(include_str!("../../../../../std/collections/iterable.terl"))
            .expect("parse iterable source contract");
    let iterable_summary = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../std/summaries/std.collections.Iterable.typi"
    ))
    .expect("parse iterable summary");
    let iterable_source_interface = syntax_module_output_to_interface(&iterable_source);
    let iterable_summary_interface = syntax_module_output_to_interface(&iterable_summary);

    assert_eq!(iterable_source_interface.module, "std.collections.Iterable");
    assert_eq!(
        iterable_summary_interface.module,
        "std.collections.Iterable"
    );
    assert_eq!(
        iterable_source_interface.traits["Iterable"].methods["iterator"].return_type,
        "std.collections.Iterator.Iterator[T]"
    );
    assert_eq!(
        iterable_summary_interface.traits["Iterable"].methods["iterator"].return_type,
        "std.collections.Iterator.Iterator[T]"
    );
}

/// Converts shared identifier names to lower snake case.
///
/// Inputs:
/// - Component/type names that exercise lowercase-to-uppercase,
///   digit-to-uppercase, and hyphen boundaries.
///
/// Output:
/// - Test assertions over normalized snake-case names.
///
/// Transformation:
/// - Covers the HIR naming helper reused by SafeNative naming, hover
///   rendering, and HTML component typechecking.
#[test]
fn identifier_to_snake_handles_shared_component_names() {
    assert_eq!(identifier_to_snake("UserCard"), "user_card");
    assert_eq!(identifier_to_snake("Field2Value"), "field2_value");
    assert_eq!(identifier_to_snake("user-card"), "user_card");
    assert_eq!(identifier_to_snake("HTMLElement"), "html_element");
    assert_eq!(identifier_to_snake("URLValue"), "url_value");
    assert_eq!(identifier_to_snake("url"), "url");
}

/// Converts external source names into valid Terlan identifiers.
///
/// Inputs:
/// - JavaScript-like member names with camelCase, acronyms, symbols, numeric
///   starts, and keyword collisions.
///
/// Output:
/// - Test assertions over valid generated Terlan identifiers.
///
/// Transformation:
/// - Pins the shared naming helper used by generated JS/TypeScript bindings.
#[test]
fn source_name_to_terlan_identifier_sanitizes_external_names() {
    assert_eq!(
        source_name_to_terlan_identifier("getElementById"),
        "get_element_by_id"
    );
    assert_eq!(source_name_to_terlan_identifier("URLValue"), "url_value");
    assert_eq!(source_name_to_terlan_identifier("type"), "type_");
    assert_eq!(
        source_name_to_terlan_identifier("2dContext"),
        "value_2d_context"
    );
    assert_eq!(source_name_to_terlan_identifier("$value"), "value");
}
