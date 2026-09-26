mod bridge_rendering;
mod collection_adapter;
mod consumer_output;
mod contract_validation;
mod enum_adapter;
mod exception_adapter;
mod function_family;
mod mutation_adapter;
mod native_helper;
mod owned_value_adapter;
mod string_adapter;
mod type_mapping_validation;

use bridge_rendering::*;
use collection_adapter::{
    collection_adapter_name, render_collection_adapter_header, render_collection_adapter_source,
    resource_list_adapter_name, resource_list_parts, resource_list_result_name,
    validate_resource_list_projection,
};
use consumer_output::*;
use contract_validation::*;
use type_mapping_validation::*;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use enum_adapter::{enum_adapter_name, render_enum_adapter_header, render_enum_adapter_source};
use exception_adapter::{
    exception_adapter_name, render_exception_adapter_header, render_exception_adapter_source,
    EXCEPTION_ENVELOPE,
};
use function_family::{expand_function_families, CppBindingFunctionFamily, CppSymbolPolicyFamily};
use mutation_adapter::{
    function_uses_mutation_adapter, has_mutation_adapters, mutation_adapter_name,
    render_mutation_adapter_header, render_mutation_adapter_source,
};
use native_helper::render_native_helper;
use owned_value_adapter::{
    cpp_call_argument_with_presence, cpp_parameters, cpp_std_tuple_elements,
    function_owned_value_resource, function_owned_value_resource_tuple, has_owned_value_adapters,
    owned_tuple_adapter_name, owned_tuple_carrier_name, owned_tuple_take_name,
    owned_value_adapter_name, owned_value_bridge_parameter_type, render_owned_value_adapter_header,
    render_owned_value_adapter_source, resource_input_copy_name, resource_list_input_name,
    resource_list_input_new_name, resource_list_input_push_name, resource_tuple_return_resources,
    selected_overload_invocation, used_mutable_secondary_resources, used_resource_list_inputs,
};
use string_adapter::{
    render_string_adapter_header, render_string_adapter_source, string_adapter_name,
};

#[cfg(test)]
mod generator_test;

const NATIVE_BINDING_SCHEMA: &str = "terlan.cpp.binding.v1";
const CPP_METADATA_SCHEMA: &str = "terlan.cpp.metadata.v1";
const CPP_MAPPING_SCHEMA: &str = "terlan.cpp.mapping.v1";
const SKIPPED_SYMBOLS_SCHEMA: &str = "terlan.cpp.binding.skipped-symbols.v1";
const CXX_VERSION: &str = "1.0.197";
const GETRANDOM_VERSION: &str = "0.3.4";
const ALLOWED_CPP_SKIP_FAMILIES: &[&str] = &[
    "cpp.annotation.unsupported",
    "cpp.callback.unsupported",
    "cpp.exception.crossing",
    "cpp.inheritance.unsupported",
    "cpp.lifetime.borrowed",
    "cpp.overload.ambiguous",
    "cpp.ownership.unknown",
    "cpp.pointer.unsupported",
    "cpp.template.unspecialized",
    "cpp.type.unmapped",
    "cpp.variadic.unsupported",
];

#[derive(Debug, Serialize, Deserialize)]
struct NativeBindingManifest {
    schema: String,
    package: NativeBindingPackage,
    build: CppBuildPlan,
    #[serde(default)]
    /// Optional package-owned classification for null native results.
    null_failure: Option<CppNullFailurePolicy>,
    cpp_metadata: CppMetadata,
    mapping: CppMappingPolicy,
    modules: Vec<NativeBindingModule>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Maps a hidden native status probe to finite package error responses.
struct CppNullFailurePolicy {
    /// Extracted C++ symbol for a no-argument `noexcept` integer probe.
    probe_symbol: String,
    /// Reviewed status values and their public errors.
    cases: Vec<CppNullFailureCase>,
    /// Error used when the probe returns an unknown or empty status.
    fallback: CppStableFailure,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Associates one native status value with one stable package error.
struct CppNullFailureCase {
    /// Integer returned by the package status probe.
    value: i64,
    /// Stable response for this status.
    #[serde(flatten)]
    failure: CppStableFailure,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Public package error without native exception payloads.
struct CppStableFailure {
    /// Stable machine-readable package error code.
    code: String,
    /// Stable human-readable package error message.
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppMappingPolicy {
    schema: String,
    symbols: Vec<CppSymbolPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Homogeneous policies expanded into one classification per symbol.
    symbol_families: Vec<CppSymbolPolicyFamily>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppSymbolPolicy {
    symbol: String,
    disposition: CppSymbolDisposition,
    #[serde(default)]
    ownership: Option<CppOwnershipPolicy>,
    #[serde(default)]
    thread_safety: Option<CppThreadSafetyPolicy>,
    #[serde(default)]
    rejection: Option<CppRejectionPolicy>,
    #[serde(default)]
    exception: Option<CppExceptionPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Stable package-owned error returned when a selected C++ callable throws.
struct CppExceptionPolicy {
    /// Finite machine-readable error atom.
    error_code: String,
    /// Stable message that does not include the upstream exception payload.
    message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CppSymbolDisposition {
    Bind,
    Reject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CppOwnershipPolicy {
    Unique,
    Shared,
    Copied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CppThreadSafetyPolicy {
    ThreadConfined,
    Send,
    Sync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppRejectionPolicy {
    shape: UnsupportedCppShape,
    detail: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeBindingPackage {
    namespace: String,
    adapter: String,
    crate_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Declares every input used to compile and link the generated adapter.
struct CppBuildPlan {
    #[serde(default)]
    /// Package-relative support headers copied beside the generated bridge header.
    adapter_headers: Vec<String>,
    /// Adapter-relative roots searched for C++ headers.
    include_roots: Vec<String>,
    /// Preprocessor definitions applied to every target.
    defines: BTreeMap<String, Option<String>>,
    /// Adapter-relative roots searched for native libraries.
    library_search_paths: Vec<String>,
    /// Native libraries linked on every target.
    linked_libraries: Vec<CppLinkedLibrary>,
    /// Additional build settings selected from Cargo target properties.
    platform_conditions: Vec<CppPlatformCondition>,
    #[serde(default)]
    /// Binary SDKs resolved from explicit environment roots at build time.
    external_roots: Vec<CppExternalRoot>,
    /// Adapter-relative files or directories that trigger a rebuild.
    rebuild_inputs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Declares one relocatable external C++ SDK used by the generated adapter.
struct CppExternalRoot {
    /// Environment variable containing the SDK root directory.
    env: String,
    /// SDK-root-relative C++ include directories.
    include_roots: Vec<String>,
    /// SDK-root-relative native library directories.
    library_search_paths: Vec<String>,
    /// Libraries linked from this SDK.
    linked_libraries: Vec<CppLinkedLibrary>,
    #[serde(default)]
    /// SDK-root-relative inputs that trigger Cargo rebuilds.
    rebuild_inputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Names one native library and its explicit Cargo link mode.
struct CppLinkedLibrary {
    /// Linker-visible library name without a filename suffix.
    name: String,
    /// Cargo link mode for the library.
    kind: CppLinkKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Supported Cargo native-library link modes.
enum CppLinkKind {
    /// Link a static archive.
    Static,
    /// Link a dynamic library.
    Dynamic,
    /// Link an Apple framework.
    Framework,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Adds build settings when all declared target selectors match.
struct CppPlatformCondition {
    #[serde(default)]
    /// Cargo target operating-system selector.
    target_os: Option<String>,
    #[serde(default)]
    /// Cargo target architecture selector.
    target_arch: Option<String>,
    #[serde(default)]
    /// Cargo target environment selector.
    target_env: Option<String>,
    #[serde(default)]
    /// Additional adapter-relative include roots.
    include_roots: Vec<String>,
    #[serde(default)]
    /// Additional preprocessor definitions.
    defines: BTreeMap<String, Option<String>>,
    #[serde(default)]
    /// Additional adapter-relative library roots.
    library_search_paths: Vec<String>,
    #[serde(default)]
    /// Additional native libraries.
    linked_libraries: Vec<CppLinkedLibrary>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CppMetadata {
    schema: String,
    producer: CppMetadataProducer,
    compile: CppCompileConfiguration,
    namespace: String,
    header: String,
    sources: Vec<String>,
    symbols: Vec<CppSymbol>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppCompileConfiguration {
    target_triple: String,
    language_standard: String,
    include_roots: Vec<String>,
    defines: BTreeMap<String, Option<String>>,
    arguments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CppMetadataProducer {
    name: String,
    version: String,
    format: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppSymbol {
    id: String,
    cpp_name: String,
    source: CppSourceLocation,
    kind: CppSymbolKind,
    documentation: String,
    annotations: Vec<String>,
    overload_set: String,
    #[serde(default)]
    receiver: Option<String>,
    #[serde(default)]
    receiver_mutable: bool,
    #[serde(default)]
    returns: Option<CppTypeMetadata>,
    #[serde(default)]
    parameters: Vec<CppParameter>,
    #[serde(default)]
    noexcept: bool,
    #[serde(default)]
    template_parameters: Vec<String>,
    #[serde(default)]
    /// Concrete arguments for one extracted function-template specialization.
    template_arguments: Vec<String>,
    #[serde(default)]
    overload_candidates: usize,
    #[serde(default)]
    variadic: bool,
    #[serde(default)]
    inheritance: Vec<String>,
    #[serde(default)]
    /// Base records reachable through a Clang-proven public conversion.
    public_inheritance: Vec<String>,
    #[serde(default)]
    fields: Vec<CppRecordField>,
    #[serde(default)]
    enum_values: Vec<CppEnumValue>,
    #[serde(default)]
    direct_calls: Vec<CppDirectCall>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One extractor-owned direct call from a callable definition.
struct CppDirectCall {
    /// Canonical identity of the directly invoked declaration.
    id: String,
    /// Qualified overload-set identity of the invoked declaration.
    overload_set: String,
    /// Declaration family of the invoked callable.
    kind: CppSymbolKind,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One extractor-owned C++ enumerator and its upstream discriminant.
struct CppEnumValue {
    /// Declared enumerator name.
    name: String,
    /// Exact signed or unsigned integer spelling produced by Clang.
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One extractor-owned non-static C++ record field.
struct CppRecordField {
    /// C++ access level controlling whether bindings may expose this field.
    #[serde(default)]
    access: CppMemberAccess,
    /// Declared C++ field name.
    name: String,
    /// Structured declared and canonical type facts.
    ty: CppTypeMetadata,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Extractor-owned C++ record-field access level.
enum CppMemberAccess {
    /// Publicly accessible field eligible for copied value mapping.
    #[default]
    Public,
    /// Derived-class-only field excluded from copied value mapping.
    Protected,
    /// Class-private field excluded from copied value mapping.
    Private,
    /// Access was not specified by the declaration model.
    None,
}

#[derive(Debug, Serialize, Deserialize)]
struct CppSourceLocation {
    path: String,
    line: usize,
    column: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CppSymbolKind {
    Record,
    Enum,
    Function,
    Method,
    Macro,
}

#[derive(Debug, Serialize, Deserialize)]
struct CppParameter {
    name: String,
    ty: CppTypeMetadata,
    direction: CppParameterDirection,
    #[serde(default)]
    default: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppTypeMetadata {
    spelling: String,
    canonical: String,
    is_const: bool,
    pointer_depth: usize,
    reference: CppReferenceKind,
    function_pointer: bool,
    template_dependent: bool,
    #[serde(default)]
    enum_type: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CppReferenceKind {
    None,
    Lvalue,
    Rvalue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CppParameterDirection {
    Input,
    Output,
    InOut,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum UnsupportedCppShape {
    RawPointerOwnership,
    ReferenceLifetimeAmbiguity,
    UnsupportedTemplate,
    ExceptionBoundary,
    OverloadAmbiguity,
    UnsupportedMacro,
    UnsupportedVariadicFunction,
    UnsupportedInheritance,
    UnsupportedCallbackShape,
    UnknownOwnership,
    UnmappedType,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeBindingModule {
    module: String,
    documentation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Fully qualified generated types imported from sibling modules.
    type_imports: Vec<String>,
    #[serde(default)]
    types: Vec<NativeBindingType>,
    #[serde(default)]
    functions: Vec<NativeBindingFunction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Homogeneous declarations expanded from exact extracted C++ symbols.
    function_families: Vec<CppBindingFunctionFamily>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeBindingType {
    name: String,
    cpp_symbol: String,
    /// Generated representation selected by package policy.
    kind: NativeBindingTypeKind,
    #[serde(default)]
    /// Reviewed copied fields for value records.
    fields: Vec<NativeBindingField>,
    #[serde(default)]
    /// Reviewed symbolic variants for generated finite enums.
    variants: Vec<NativeBindingVariant>,
    #[serde(default)]
    /// Extracted zero-argument method that copies this value to `std::string`.
    stringifier: Option<String>,
    documentation: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Generated representation of one selected C++ record.
enum NativeBindingTypeKind {
    /// Uniquely owned process-local C++ resource.
    OpaqueResource,
    /// Immutable ordinary Terlan record copied from primitive getters.
    ValueRecord,
    /// Finite symbolic value converted without exposing C++ discriminants.
    Enum,
    /// Transparent String whose C++ value is constructed inside an adapter.
    StringValue,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Package-owned mapping from one C++ enumerator to one Terlan atom type.
struct NativeBindingVariant {
    /// Public Terlan variant type and constructor name.
    name: String,
    /// Extractor-owned C++ enumerator selected by package policy.
    cpp_name: String,
    /// Stable lowercase atom transported by the native helper.
    atom: String,
    /// Public documentation for the generated variant.
    documentation: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Package-owned mapping for one copied record field.
struct NativeBindingField {
    /// Public Terlan field name.
    name: String,
    /// Extractor-owned C++ field represented by this field.
    cpp_field: String,
    /// Public Terlan field type.
    ty: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeBindingFunction {
    name: String,
    operation: String,
    #[serde(default)]
    cpp_symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Pure Terlan implementation used to compose generated native leaves.
    terlan_body: Option<String>,
    #[serde(default, skip_serializing_if = "NativeVisibility::is_public")]
    /// Controls whether a generated declaration is exported from its module.
    visibility: NativeVisibility,
    role: NativeFunctionRole,
    #[serde(default)]
    args: Vec<NativeBindingArg>,
    #[serde(default)]
    /// Getter symbols used to assemble a copied value record.
    projections: Vec<NativeBindingProjection>,
    #[serde(default)]
    /// Typed success/error mapping for a contained throwing callable.
    fallible: Option<NativeFallibleResult>,
    returns: String,
    blocking: NativeBlockingPolicy,
    resource: NativeResourcePolicy,
    documentation: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NativeVisibility {
    #[default]
    Public,
    Private,
}

impl NativeVisibility {
    fn is_public(&self) -> bool {
        *self == Self::Public
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Public Terlan result types for one exception-contained operation.
struct NativeFallibleResult {
    /// Success value copied from the C++ call.
    ok: String,
    /// Stable public error type.
    error: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Maps one copied Terlan field to a reviewed C++ getter.
struct NativeBindingProjection {
    /// Generated Terlan record field.
    field: String,
    /// Bindable zero-argument C++ getter symbol.
    cpp_symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Declares one public Terlan argument and its optional copied-field lowering.
struct NativeBindingArg {
    /// Public argument name.
    name: String,
    /// Public Terlan argument type.
    ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Exact extracted C++ parameter receiving this public argument or grouped value.
    cpp_parameter: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    /// Retains a compatibility-only public argument without forwarding it to C++.
    cpp_ignore: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Reviewed Terlan literal used when the public caller omits this argument.
    default: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    /// Additional retained resource mutated by a generated free-function adapter.
    mutable: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    /// Copies the immediately preceding resource before this resource list.
    prepend_resource: bool,
    #[serde(default)]
    /// Explicit projections from one copied record to scalar C++ parameters.
    fields: Vec<NativeBindingArgField>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Maps one public copied-record field to one extracted C++ parameter.
struct NativeBindingArgField {
    /// Public field selected from the argument's generated value record.
    field: String,
    /// Extracted scalar C++ parameter receiving the copied field value.
    cpp_parameter: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NativeFunctionRole {
    Constructor,
    ImmutableMethod,
    MutableMethod,
    /// Mutates one owned resource through a generated, exception-contained C++ free function.
    MutableFreeFunction,
    FreeFunction,
    /// Copies reviewed getter results into an ordinary Terlan record.
    ValueProjection,
    /// Copies reviewed getters from a temporary uniquely owned C++ value.
    OwnedValueProjection,
    /// Converts one C++ enum result into a reviewed finite Terlan atom.
    EnumProjection,
    /// Converts one copied C++ value through its extracted stringifier.
    StringProjection,
    /// Copies one potentially throwing C++ primitive getter through containment.
    ScalarProjection,
    /// Copies one borrowed C++ integer collection into an owned Terlan list.
    IntListProjection,
    /// Copies a returned C++ vector into independently owned resource handles.
    ResourceListProjection,
    /// Executes a potentially throwing method through a generated `noexcept` envelope.
    ExceptionMethod,
    Dispose,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NativeBlockingPolicy {
    Fast,
    Blocking,
    Async,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NativeResourcePolicy {
    Value,
    /// Legacy spelling for a package-owned opaque resource.
    OpaqueHandle,
    /// Non-null package-owned handle returned by a reviewed producer.
    OwnedHandle,
    /// C++ nullable handle whose null branch is mapped through `null_failure`.
    NullableHandle,
    BorrowedHandle,
    MutableHandle,
    DisposeHandle,
    TransferableHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Counts the modules, functions, and rejected symbols emitted for one package.
pub(in crate::commands::bind) struct CppBindingGenerationSummary {
    /// Number of generated Terlan modules.
    pub(in crate::commands::bind) module_count: usize,
    /// Number of generated public binding functions.
    pub(in crate::commands::bind) function_count: usize,
    /// Number of declarations retained in the stable rejection snapshot.
    pub(in crate::commands::bind) skipped_symbol_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct SkippedSymbol {
    id: String,
    symbol: String,
    source: String,
    reason: String,
    message: String,
}

#[derive(Serialize)]
struct SkippedSymbolsManifest<'a> {
    schema: &'static str,
    metadata_producer: &'a CppMetadataProducer,
    skipped: &'a [SkippedSymbol],
}

struct ValidatedCppSymbols<'a> {
    declarations: BTreeMap<&'a str, &'a CppSymbol>,
    policies: BTreeMap<&'a str, &'a CppSymbolPolicy>,
}

impl ValidatedCppSymbols<'_> {
    fn is_bindable(&self, symbol: &str) -> bool {
        self.policies
            .get(symbol)
            .is_some_and(|policy| policy.disposition == CppSymbolDisposition::Bind)
    }
}

/// Generates an executable C++ package from normalized metadata emitted by a
/// maintained C++ frontend. The generator copies the declared package sources;
/// it never parses C++ headers itself.
pub(in crate::commands::bind) fn generate_cpp_bindings(
    manifest_path: &Path,
    out_dir: &Path,
) -> Result<CppBindingGenerationSummary, String> {
    refuse_non_empty_output(out_dir)?;
    let manifest_text = fs::read_to_string(manifest_path).map_err(|err| {
        format!(
            "failed to read native binding manifest `{}`: {err}",
            manifest_path.display()
        )
    })?;
    let mut manifest: NativeBindingManifest =
        serde_json::from_str(&manifest_text).map_err(|err| {
            format!(
                "failed to parse structured C++ metadata `{}`: {err}",
                manifest_path.display()
            )
        })?;
    expand_function_families(&mut manifest)?;
    let input_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let symbols = validate_manifest(&manifest, input_dir)?;
    let skipped = collect_skipped_symbols(&symbols)?;

    fs::create_dir_all(out_dir).map_err(|err| {
        format!(
            "failed to create output directory `{}`: {err}",
            out_dir.display()
        )
    })?;

    for module in &manifest.modules {
        write_file(
            &out_dir.join(module_source_path(&module.module)),
            &render_module_source(module),
        )?;
        write_file(
            &out_dir.join(module_docs_path(&module.module)),
            &render_module_docs(module, &symbols.declarations),
        )?;
        if let Some(test) = render_consumer_test(module, &manifest.modules)? {
            write_file(&out_dir.join(consumer_test_path(&module.module)), &test)?;
        }
    }

    copy_cpp_inputs(&manifest.cpp_metadata, &manifest.build, input_dir, out_dir)?;
    write_file(
        &out_dir.join("terlan.toml"),
        &render_terlan_manifest(&manifest),
    )?;
    write_file(
        &out_dir.join("native/terlan-native.toml"),
        &render_native_boundary_metadata(&manifest)?,
    )?;
    write_file(
        &out_dir.join("native/rust/Cargo.toml"),
        &render_rust_adapter_cargo(&manifest),
    )?;
    write_file(
        &out_dir.join("native/rust/build.rs"),
        &render_cxx_build(
            &manifest.cpp_metadata,
            &manifest.build,
            has_enum_adapters(&manifest),
            has_string_adapters(&manifest),
            has_collection_adapters(&manifest),
            has_exception_adapters(&manifest),
            has_owned_value_adapters(&manifest, &symbols.declarations),
            has_mutation_adapters(&manifest),
        ),
    )?;
    write_file(
        &out_dir.join("native/rust/include/terlan_enum_adapters.hpp"),
        &render_enum_adapter_header(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/cpp/terlan_enum_adapters.cc"),
        &render_enum_adapter_source(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/include/terlan_string_adapters.hpp"),
        &render_string_adapter_header(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/cpp/terlan_string_adapters.cc"),
        &render_string_adapter_source(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/include/terlan_collection_adapters.hpp"),
        &render_collection_adapter_header(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/cpp/terlan_collection_adapters.cc"),
        &render_collection_adapter_source(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/include/terlan_exception_adapters.hpp"),
        &render_exception_adapter_header(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"),
        &render_exception_adapter_source(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"),
        &render_owned_value_adapter_header(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"),
        &render_owned_value_adapter_source(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/include/terlan_mutation_adapters.hpp"),
        &render_mutation_adapter_header(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/cpp/terlan_mutation_adapters.cc"),
        &render_mutation_adapter_source(&manifest, &symbols.declarations)?,
    )?;
    write_file(
        &out_dir.join("native/rust/src/lib.rs"),
        &render_cxx_bridge(&manifest, &symbols)?,
    )?;
    write_file(
        &out_dir.join("native/rust/src/bin/native_boundary_helper.rs"),
        &render_native_helper(&manifest, &symbols.declarations)?,
    )?;
    let normalized_manifest = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to normalize native binding manifest: {err}"))?;
    write_file(
        &out_dir.join("bindings/native-binding-manifest.json"),
        &(normalized_manifest + "\n"),
    )?;
    write_file(
        &out_dir.join("bindings/skipped-symbols.json"),
        &render_skipped_symbols(&manifest.cpp_metadata.producer, &skipped)?,
    )?;

    Ok(CppBindingGenerationSummary {
        module_count: manifest.modules.len(),
        function_count: manifest
            .modules
            .iter()
            .map(|module| module.functions.len())
            .sum(),
        skipped_symbol_count: skipped.len(),
    })
}
