use std::collections::{HashMap, HashSet};

use terlan_syntax::span::Span;

#[derive(Debug, Clone)]
/// Public module interface used by downstream resolver/typecheck phases.
///
/// Inputs: syntax-output module or `.terli`/`.typi` summary. Output: importable
/// module surface. Transformation: strips implementation bodies while
/// preserving exported types, constructors, functions, traits, conformances,
/// docs, and overload metadata.
pub struct ModuleInterface {
    pub module: String,
    pub docs: Vec<String>,
    pub public_types: HashSet<String>,
    pub private_types: HashSet<String>,
    pub opaque_types: HashSet<String>,
    pub type_params: HashMap<String, Vec<String>>,
    pub type_bodies: HashMap<String, Vec<String>>,
    pub struct_fields: HashMap<String, Vec<StructFieldSignature>>,
    pub type_docs: HashMap<String, Vec<String>>,
    pub traits: HashMap<String, TraitSignature>,
    pub trait_conformances: Vec<TraitConformanceSignature>,
    pub constructors: HashMap<String, Vec<ConstructorSignature>>,
    pub functions: HashMap<(String, usize), FunctionSignature>,
    pub function_overloads: HashMap<(String, usize), Vec<FunctionSignature>>,
}

/// Public field signature for a struct exported through a module interface.
///
/// Inputs:
/// - One source struct field from syntax output.
///
/// Output:
/// - Stable interface metadata containing the field name and normalized type.
///
/// Transformation:
/// - Drops source spans and default expressions so imported modules can type
///   check and expand included struct shape without depending on implementation
///   source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructFieldSignature {
    pub name: String,
    pub annotation: String,
    pub is_private: bool,
}

#[derive(Debug, Clone)]
/// Constructor signature exported through a module interface.
///
/// Inputs: syntax-output constructor clause. Output: callable constructor
/// signature. Transformation: records fixed parameters, optional vararg,
/// return type, body summary, arity policy, visibility, and docs.
pub struct ConstructorSignature {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<ParamSignature>,
    pub vararg: Option<ParamSignature>,
    pub return_type: String,
    pub body: String,
    pub min_arity: usize,
    pub varargs: bool,
    pub public: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
/// Function or receiver-method signature exported through an interface.
///
/// Inputs: syntax-output function, method, or native config signature. Output:
/// callable signature metadata. Transformation: records normalized parameter
/// and return annotations plus receiver/visibility/doc metadata.
pub struct FunctionSignature {
    pub name: String,
    pub generic_params: Vec<String>,
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub generic_bounds: Vec<String>,
    pub receiver_method: bool,
    pub receiver_mutable: bool,
    pub public: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
/// Trait signature exported through an interface.
///
/// Inputs: public syntax-output trait declaration. Output: trait methods and
/// inheritance metadata. Transformation: keeps type params, super traits,
/// method signatures, and docs without implementation bodies.
pub struct TraitSignature {
    pub name: String,
    pub type_params: Vec<String>,
    pub super_traits: Vec<String>,
    pub methods: HashMap<String, TraitMethodSignature>,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
/// Trait method signature exported through an interface.
///
/// Inputs: syntax-output trait method declaration. Output: method signature.
/// Transformation: records params, return type, generic bounds, default-body
/// availability, and docs.
pub struct TraitMethodSignature {
    pub generic_params: Vec<String>,
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub generic_bounds: Vec<String>,
    pub has_default: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Trait conformance fact exported through an interface.
///
/// Inputs: declaration-site `implements` or explicit impl declaration. Output:
/// normalized conformance metadata. Transformation: records trait, owner type,
/// source category, and visibility for imported conformance checks.
pub struct TraitConformanceSignature {
    pub trait_ref: String,
    pub for_type: String,
    pub source: TraitConformanceSource,
    pub public: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Source form that introduced a trait conformance.
///
/// Inputs: syntax declaration kind. Output: implements or explicit-impl tag.
/// Transformation: classifies conformance provenance without affecting backend
/// lowering.
pub enum TraitConformanceSource {
    Implements,
    ExplicitImpl,
}

#[derive(Debug, Clone)]
/// Parameter signature used by HIR interfaces.
///
/// Inputs: syntax-output parameter. Output: name, normalized annotation, and
/// mutability/default metadata. Transformation: removes spans while preserving
/// callable type shape and optional source-like default text for generated
/// interface summaries.
pub struct ParamSignature {
    pub name: String,
    pub annotation: String,
    pub is_mutable: bool,
    pub default_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Visibility of a type name in HIR.
///
/// Inputs: declaration visibility. Output: public or private tag.
/// Transformation: normalizes source visibility for import and export checks.
pub enum TypeVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone)]
/// Function symbol resolved in the current module.
///
/// Inputs: syntax-output function or method declaration. Output: local symbol
/// table entry. Transformation: records callable shape, export/public flags,
/// docs, and source span for duplicate/export diagnostics.
pub struct FunctionSymbol {
    pub name: String,
    pub arity: usize,
    pub generic_params: Vec<String>,
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub generic_bounds: Vec<String>,
    pub receiver_method: bool,
    pub receiver_mutable: bool,
    pub public: bool,
    pub exported: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
/// Imported type or trait item resolved from an interface.
///
/// Inputs: import item and provider interface. Output: local import binding.
/// Transformation: records local alias, provider module/name, visibility, and
/// source span for duplicate diagnostics.
pub struct ImportedItem {
    pub local_name: String,
    pub source_module: String,
    pub source_name: String,
    pub visibility: TypeVisibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
/// Fully resolved module summary.
///
/// Inputs: syntax-output module plus visible interfaces. Output: local symbols,
/// imports, generated interface, and diagnostics. Transformation: resolves
/// imports/exports/types/functions while preserving loaded interface metadata.
pub struct ResolvedModule {
    pub name: String,
    pub function_symbols: HashMap<(String, usize), FunctionSymbol>,
    pub local_type_names: HashMap<String, TypeVisibility>,
    pub imported_types: HashMap<String, ImportedItem>,
    pub imported_traits: HashMap<String, ImportedItem>,
    pub interface_map: HashMap<String, ModuleInterface>,
    pub interface: ModuleInterface,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
/// HIR diagnostic.
///
/// Inputs: resolver error condition. Output: span/message diagnostic.
/// Transformation: attaches HIR messages to source spans for later display.
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

#[derive(Debug)]
/// Resolver result wrapper.
///
/// Inputs: syntax-output module resolution. Output: resolved module.
/// Transformation: packages the resolved module for callers while leaving room
/// for future resolver metadata.
pub struct ResolveResult {
    pub module: ResolvedModule,
}
