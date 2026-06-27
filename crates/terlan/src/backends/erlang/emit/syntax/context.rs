//! Syntax-output Erlang lowering context and lookup tables.
//!
//! This module owns module-wide metadata collection for the syntax bridge.

use super::*;

/// Context shared by syntax-output Erlang lowering.
///
/// Inputs:
/// - Syntax module output, imported interfaces, static assets, templates, and
///   markdown imports.
///
/// Output:
/// - Lookup tables used by expression, pattern, callable, constructor, trait,
///   template, and receiver-method lowering.
///
/// Transformation:
/// - Precomputes source-visible and backend-visible dispatch metadata so
///   individual lowering functions can stay mostly local and table-driven.
pub(in crate::backends::erlang::emit) struct SyntaxLowerCtx {
    pub(super) module_name: String,
    pub(super) constructors: BTreeMap<String, Vec<SyntaxConstructorTarget>>,
    pub(super) imported_constructor_targets: BTreeMap<String, Vec<SyntaxRemoteConstructorTarget>>,
    pub(super) remote_constructor_targets: BTreeMap<String, Vec<SyntaxRemoteConstructorTarget>>,
    pub(super) constructor_patterns: BTreeMap<String, Vec<SyntaxConstructorPatternTarget>>,
    pub(super) alias_constructor_targets: BTreeMap<String, SyntaxAliasConstructorTarget>,
    pub(super) remote_alias_constructor_targets: BTreeMap<String, SyntaxAliasConstructorTarget>,
    pub(super) local_functions: BTreeMap<String, Vec<SyntaxLocalFunctionTarget>>,
    pub(super) imported_functions: BTreeMap<String, Vec<SyntaxImportedFunctionTarget>>,
    pub(super) local_function_params: BTreeMap<(String, usize), Vec<String>>,
    pub(super) opaque_constructors: BTreeSet<String>,
    pub(super) trait_method_wrappers: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) typed_trait_method_wrappers:
        BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
    pub(super) generic_functions: BTreeMap<(String, usize), SyntaxGenericFunctionTarget>,
    pub(super) local_function_values: BTreeMap<String, usize>,
    pub(super) imported_trait_aliases: BTreeMap<String, (String, String)>,
    pub(super) imported_trait_conformances: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) imported_type_refs: BTreeMap<String, String>,
    pub(super) local_trait_methods: BTreeMap<String, BTreeSet<String>>,
    pub(super) receiver_methods:
        BTreeMap<(String, usize), BTreeMap<String, SyntaxReceiverMethodTarget>>,
    pub(super) module_aliases: BTreeMap<String, String>,
    pub(super) imported_module_member_functions:
        BTreeMap<(String, String), SyntaxImportedFunctionTarget>,
    pub(super) file_imports: BTreeMap<String, Vec<u8>>,
    pub(super) markdown_imports: BTreeMap<String, crate::terlan_html::MarkdownDocument>,
    pub(super) templates: BTreeMap<String, LowerTemplate>,
    pub(super) struct_field_types: BTreeMap<String, BTreeMap<String, String>>,
}

/// Receiver-method target metadata used by backend dispatch.
///
/// Inputs:
/// - Local method declarations and inherited methods from local struct includes.
///
/// Output:
/// - Mutable receiver marker and non-receiver parameter names for a receiver
///   type/method/arity target.
///
/// Transformation:
/// - Carries mutability and source parameter names into lowering so mutable
///   receiver calls can rebind and named receiver arguments can lower in
///   declaration order.
#[derive(Debug, Clone)]
pub(in crate::backends::erlang::emit) struct SyntaxReceiverMethodTarget {
    pub(in crate::backends::erlang::emit) mutable: bool,
    pub(super) fixed_arity: usize,
    pub(super) min_arity: usize,
    pub(super) param_names: Vec<String>,
    pub(super) defaults: Vec<Option<SyntaxExprOutput>>,
}

/// Local function target metadata used for default-aware call lowering.
///
/// Inputs:
/// - Local function declarations in the syntax-output module.
///
/// Output:
/// - Full arity, required arity, parameter names, default expressions, and the
///   declared return type.
///
/// Transformation:
/// - Captures enough declaration metadata for the backend to lower calls that
///   omit trailing/defaulted parameters by inserting the declaration defaults,
///   and to propagate return types into downstream pattern-bound receiver
///   dispatch.
#[derive(Debug, Clone)]
pub(super) struct SyntaxLocalFunctionTarget {
    pub(super) fixed_arity: usize,
    pub(super) min_arity: usize,
    pub(super) param_names: Vec<String>,
    pub(super) defaults: Vec<Option<SyntaxExprOutput>>,
    pub(super) return_type: String,
}

/// Selected imported function target metadata.
///
/// Inputs:
/// - Imported interface function signatures and selected import aliases.
///
/// Output:
/// - Provider module/function identity plus arity/default metadata.
///
/// Transformation:
/// - Stores enough interface information for selected imported calls to lower
///   named arguments and omitted defaulted parameters without provider source.
#[derive(Debug, Clone)]
pub(super) struct SyntaxImportedFunctionTarget {
    pub(super) module: String,
    pub(super) function: String,
    pub(super) fixed_arity: usize,
    pub(super) min_arity: usize,
    pub(super) param_names: Vec<String>,
    pub(super) defaults: Vec<Option<SyntaxExprOutput>>,
}

/// Local environment for lowering one syntax-output expression body.
///
/// Inputs:
/// - Function/constructor parameters, generic bounds, and temporary lowering
///   substitutions introduced by `let`, pipes, and mutable receivers.
///
/// Output:
/// - Local value/type/struct and replacement tables used during expression and
///   pattern lowering.
///
/// Transformation:
/// - Tracks only body-local state; module-wide lookup data stays in
///   `SyntaxLowerCtx`.
#[derive(Clone, Default)]
pub(super) struct SyntaxLowerEnv {
    pub(super) value_locals: BTreeSet<String>,
    pub(super) value_types: BTreeMap<String, String>,
    pub(super) trait_bound_dicts: BTreeMap<(String, String), String>,
    pub(super) value_replacements: BTreeMap<String, ErlExpr>,
    pub(super) current_constructor_target: Option<String>,
}

impl SyntaxLowerCtx {
    /// Creates an empty syntax lowering context.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Context with all lookup tables empty.
    ///
    /// Transformation:
    /// - Provides a deterministic fallback for tests and unsupported direct
    ///   lowering paths.
    pub(super) fn empty() -> Self {
        Self {
            module_name: String::new(),
            constructors: BTreeMap::new(),
            imported_constructor_targets: BTreeMap::new(),
            remote_constructor_targets: BTreeMap::new(),
            constructor_patterns: BTreeMap::new(),
            alias_constructor_targets: BTreeMap::new(),
            remote_alias_constructor_targets: BTreeMap::new(),
            local_functions: BTreeMap::new(),
            imported_functions: BTreeMap::new(),
            local_function_params: BTreeMap::new(),
            opaque_constructors: BTreeSet::new(),
            trait_method_wrappers: BTreeMap::new(),
            typed_trait_method_wrappers: BTreeMap::new(),
            generic_functions: BTreeMap::new(),
            local_function_values: BTreeMap::new(),
            imported_trait_aliases: BTreeMap::new(),
            imported_trait_conformances: BTreeMap::new(),
            imported_type_refs: BTreeMap::new(),
            local_trait_methods: BTreeMap::new(),
            receiver_methods: BTreeMap::new(),
            module_aliases: BTreeMap::new(),
            imported_module_member_functions: BTreeMap::new(),
            file_imports: BTreeMap::new(),
            markdown_imports: BTreeMap::new(),
            templates: BTreeMap::new(),
            struct_field_types: BTreeMap::new(),
        }
    }

    /// Resolves a source-visible remote module or module alias.
    ///
    /// Inputs:
    /// - `module`: module segment used in source remote-call syntax.
    ///
    /// Output:
    /// - Fully resolved module name when an alias exists, otherwise `module`.
    ///
    /// Transformation:
    /// - Applies selected import aliases without changing already-qualified
    ///   module names.
    pub(super) fn resolve_remote_module(&self, module: &str) -> String {
        self.module_aliases
            .get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string())
    }

    /// Resolves a selected imported function call target.
    ///
    /// Inputs:
    /// - `name`: local function name or import alias used at the call site.
    /// - `arity`: number of call arguments.
    ///
    /// Output:
    /// - Imported function target when the local call resolves to a selected
    ///   public function import that accepts the supplied arity.
    ///
    /// Transformation:
    /// - Looks up selected import metadata and applies required/full arity
    ///   checks so omitted defaulted parameters can still lower to full
    ///   backend calls.
    pub(super) fn imported_function_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxImportedFunctionTarget> {
        let mut matches = self
            .imported_functions
            .get(name)?
            .iter()
            .filter(|target| arity >= target.min_arity && arity <= target.fixed_arity);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// Resolves an imported module-member function value.
    ///
    /// Inputs:
    /// - `module_alias`: source-visible imported module alias such as `Users`.
    /// - `function`: member name such as `index`.
    ///
    /// Output:
    /// - Imported function target when the referenced module exposes exactly
    ///   one public function with that name.
    ///
    /// Transformation:
    /// - Reads the precomputed module-member function table populated from
    ///   imported interfaces, keeping field access lowering deterministic.
    pub(super) fn imported_module_member_function_target(
        &self,
        module_alias: &str,
        function: &str,
    ) -> Option<&SyntaxImportedFunctionTarget> {
        self.imported_module_member_functions
            .get(&(module_alias.to_string(), function.to_string()))
    }

    /// Resolves local function parameter names for named-argument emission.
    ///
    /// Inputs:
    /// - `name`: local function name used at the call site.
    /// - `arity`: number of supplied arguments.
    ///
    /// Output:
    /// - Parameter names in declaration order when the callable is a local
    ///   function in the current syntax-output module.
    ///
    /// Transformation:
    /// - Looks up the source declaration table captured during context
    ///   construction without changing overload or dispatch behavior.
    pub(super) fn local_function_param_names(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&Vec<String>> {
        self.local_function_params.get(&(name.to_string(), arity))
    }

    /// Resolves a local function target for supplied call arity.
    ///
    /// Inputs:
    /// - `name`: local function call head.
    /// - `arity`: number of source arguments supplied at the call site.
    ///
    /// Output:
    /// - Function target that accepts the supplied arity after considering
    ///   defaulted parameters.
    /// - `None` when no unique local function target accepts the arity.
    ///
    /// Transformation:
    /// - Applies required/full arity bounds captured from the declaration and
    ///   keeps overload ambiguity conservative by requiring a single match.
    pub(super) fn local_function_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxLocalFunctionTarget> {
        let mut matches = self
            .local_functions
            .get(name)?
            .iter()
            .filter(|target| arity >= target.min_arity && arity <= target.fixed_arity);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// Resolves metadata for a bounded generic local function.
    ///
    /// Inputs:
    /// - `name`: local function name used at the call site.
    /// - `arity`: source-visible argument count, excluding hidden dictionaries.
    ///
    /// Output:
    /// - Generic function metadata when the local function declares one or more
    ///   trait bounds.
    ///
    /// Transformation:
    /// - Looks up the source-visible function key captured from formal syntax
    ///   output without exposing hidden backend dictionary parameters to source
    ///   callers.
    pub(super) fn generic_function_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxGenericFunctionTarget> {
        self.generic_functions.get(&(name.to_string(), arity))
    }

    /// Resolves a local alias-constructor target by name and arity.
    ///
    /// Inputs:
    /// - `name`: source constructor-like alias name.
    /// - `arity`: number of associated values.
    ///
    /// Output:
    /// - Alias constructor target when the alias exists and arity matches.
    ///
    /// Transformation:
    /// - Looks up eligible single-shape type aliases collected from the current
    ///   module and selected imports.
    pub(super) fn alias_constructor_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_targets
            .get(name)
            .filter(|target| target.params.len() == arity)
    }

    /// Resolves a transparent singleton alias value by source name.
    ///
    /// Inputs:
    /// - `name`: local source name from a bare variable expression.
    ///
    /// Output:
    /// - The zero-payload alias target when `name` represents a singleton atom
    ///   value such as `None` or `Unit`.
    /// - `None` when the alias carries associated values or is not present.
    ///
    /// Transformation:
    /// - Looks up the existing alias-constructor target table but restricts
    ///   bare value lowering to targets with no parameters.
    pub(super) fn singleton_alias_value_target(
        &self,
        name: &str,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_targets
            .get(name)
            .filter(|target| target.params.is_empty())
    }

    /// Resolves an alias call target for constructor syntax.
    ///
    /// Inputs:
    /// - `name`: local call head.
    /// - `arity`: number of supplied call arguments.
    ///
    /// Output:
    /// - The alias target when constructor syntax carries associated values.
    /// - `None` for zero-payload aliases so `None()` and `Unit()` do not lower.
    ///
    /// Transformation:
    /// - Reuses alias target metadata while enforcing that call syntax is only
    ///   for associated values, not singleton atom aliases.
    pub(super) fn alias_constructor_call_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_target(name, arity)
            .filter(|target| !target.params.is_empty())
    }

    /// Resolves a remote alias-constructor target by module, name, and arity.
    ///
    /// Inputs:
    /// - `module`: source remote module or module alias.
    /// - `name`: remote alias constructor name.
    /// - `arity`: number of associated values.
    ///
    /// Output:
    /// - Remote alias target when the provider exposes a matching eligible
    ///   alias shape.
    ///
    /// Transformation:
    /// - Resolves module aliases and checks arity against imported interface
    ///   type-body metadata.
    pub(super) fn remote_alias_constructor_target(
        &self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        let key = format!("{}.{}", self.resolve_remote_module(module), name);
        self.remote_alias_constructor_targets
            .get(&key)
            .filter(|target| target.params.len() == arity)
    }

    /// Resolves a local explicit constructor target.
    ///
    /// Inputs:
    /// - `name`: constructor call name.
    /// - `arity`: number of supplied call arguments.
    ///
    /// Output:
    /// - Constructor target that accepts the arity.
    ///
    /// Transformation:
    /// - Applies min/fixed arity and varargs rules captured from constructor
    ///   clauses.
    pub(super) fn constructor_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxConstructorTarget> {
        self.constructors.get(name)?.iter().find(|target| {
            if target.varargs {
                arity >= target.fixed_arity
            } else {
                arity >= target.min_arity && arity <= target.fixed_arity
            }
        })
    }

    /// Resolves a selected imported constructor target.
    ///
    /// Inputs:
    /// - `name`: local import name or alias.
    /// - `arity`: number of supplied call arguments.
    ///
    /// Output:
    /// - Remote constructor target selected by import and arity.
    ///
    /// Transformation:
    /// - Searches public constructor signatures from imported interfaces.
    pub(super) fn imported_constructor_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxRemoteConstructorTarget> {
        self.imported_constructor_targets
            .get(name)?
            .iter()
            .find(|target| target.accepts_arity(arity))
    }

    /// Resolves an explicit remote constructor target.
    ///
    /// Inputs:
    /// - `module`: source remote module or alias.
    /// - `name`: remote constructor name.
    /// - `arity`: number of supplied call arguments.
    ///
    /// Output:
    /// - Remote constructor target when a public signature accepts the arity.
    ///
    /// Transformation:
    /// - Resolves module aliases and searches imported interface constructor
    ///   signatures.
    pub(super) fn remote_constructor_target(
        &self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxRemoteConstructorTarget> {
        let key = format!("{}.{}", self.resolve_remote_module(module), name);
        self.remote_constructor_targets
            .get(&key)?
            .iter()
            .find(|target| target.accepts_arity(arity))
    }

    /// Resolves a local constructor pattern target.
    ///
    /// Inputs:
    /// - `name`: constructor-pattern name.
    /// - `arity`: number of pattern arguments.
    ///
    /// Output:
    /// - Pattern lowering target when the constructor has a non-vararg clause
    ///   with matching arity.
    ///
    /// Transformation:
    /// - Searches precomputed local constructor pattern metadata.
    pub(super) fn constructor_pattern_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxConstructorPatternTarget> {
        self.constructor_patterns
            .get(name)?
            .iter()
            .find(|target| target.params.len() == arity)
    }

    /// Resolves an untyped trait-method wrapper.
    ///
    /// Inputs:
    /// - `trait_name`: source trait name or qualified trait reference.
    /// - `method`: trait method name.
    ///
    /// Output:
    /// - Wrapper function name when an untyped wrapper exists.
    ///
    /// Transformation:
    /// - Attempts exact trait lookup first, then final-segment lookup for
    ///   qualified source names.
    pub(super) fn trait_method_wrapper(&self, trait_name: &str, method: &str) -> Option<&String> {
        let key = trait_name.trim_matches('.').trim();
        if let Some(wrapper) = self
            .trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
        {
            return Some(wrapper);
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
    }

    /// Returns the generated wrapper for a typed trait-method implementation.
    ///
    /// Inputs:
    /// - `trait_name`: trait name from a local or imported trait-method call.
    /// - `method`: trait method name.
    /// - `type_arg`: normalized concrete implementation type.
    ///
    /// Output:
    /// - `Some(wrapper_name)` when the trait, method, and type argument resolve
    ///   to a typed implementation wrapper.
    /// - `None` when the call must fall back to the untyped dispatch path.
    ///
    /// Transformation:
    /// - Looks up the exact trait name first, then falls back to the final
    ///   qualified segment so source-qualified names can share local wrappers.
    pub(super) fn typed_trait_method_wrapper(
        &self,
        trait_name: &str,
        method: &str,
        type_arg: &str,
    ) -> Option<&String> {
        let key = trait_name.trim_matches('.').trim();
        if let Some(wrapper) = self
            .typed_trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
            .and_then(|types| types.get(type_arg))
        {
            return Some(wrapper);
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.typed_trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
            .and_then(|types| types.get(type_arg))
    }

    /// Returns whether a local trait declares a method name.
    ///
    /// Inputs:
    /// - `trait_name`: source-visible local trait name.
    /// - `method`: source-visible trait method name.
    ///
    /// Output:
    /// - `true` when the current module declares the trait and method.
    ///
    /// Transformation:
    /// - Performs a syntax-output inventory lookup so backend lowering only
    ///   rewrites calls that are actually trait-shaped in source.
    pub(super) fn has_local_trait_method(&self, trait_name: &str, method: &str) -> bool {
        self.local_trait_methods
            .get(trait_name)
            .is_some_and(|methods| methods.contains(method))
    }

    /// Returns provider metadata for an imported trait alias.
    ///
    /// Inputs:
    /// - `name`: local trait alias used as the remote call head, or the
    ///   qualified selected-import spelling produced by module alias
    ///   resolution.
    ///
    /// Output:
    /// - Provider module and provider-local trait name when the call head
    ///   identifies an imported trait.
    /// - `None` when the call head is not an imported trait.
    ///
    /// Transformation:
    /// - Looks up the call head directly first, then falls back to its final
    ///   dotted segment so imports such as `std.collections.Enumerable.{Enumerable}`
    ///   still dispatch as traits after uppercase selected-import alias
    ///   qualification.
    pub(super) fn imported_trait_alias(&self, name: &str) -> Option<(&str, &str)> {
        let key = name.trim_matches('.').trim();
        if let Some((module, source_name)) = self.imported_trait_aliases.get(key) {
            return Some((module.as_str(), source_name.as_str()));
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.imported_trait_aliases
            .get(key)
            .map(|(module, source_name)| (module.as_str(), source_name.as_str()))
    }

    /// Returns the provider-local wrapper type for an imported conformance.
    ///
    /// Inputs:
    /// - `trait_name`: local imported trait name or alias used at the call site.
    /// - `type_arg`: normalized concrete first-argument type.
    ///
    /// Output:
    /// - Provider-local type key used in the remote wrapper function name.
    /// - `None` when provider interface metadata exposes no public conformance.
    ///
    /// Transformation:
    /// - Looks up by the consumer-qualified type key while returning the
    ///   provider-local type key, because remote wrapper symbols are generated
    ///   in the provider module's local namespace.
    pub(super) fn imported_trait_conformance_wrapper_type(
        &self,
        trait_name: &str,
        type_arg: &str,
    ) -> Option<&str> {
        let key = trait_name.trim_matches('.').trim();
        let normalized_type_arg = normalize_syntax_trait_dispatch_type_key(type_arg);
        if let Some(wrapper) = self.imported_trait_conformances.get(key).and_then(|types| {
            types
                .get(type_arg)
                .or_else(|| types.get(normalized_type_arg.as_str()))
        }) {
            return Some(wrapper.as_str());
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.imported_trait_conformances
            .get(key)
            .and_then(|types| {
                types
                    .get(type_arg)
                    .or_else(|| types.get(normalized_type_arg.as_str()))
            })
            .map(String::as_str)
    }

    /// Returns metadata for a local receiver-method declaration.
    ///
    /// Inputs:
    /// - `receiver_type`: normalized type key inferred for the call receiver.
    /// - `method`: source method name.
    /// - `arity`: number of non-receiver call arguments.
    ///
    /// Output:
    /// - Receiver-method metadata when the current module declares the selected
    ///   receiver type and method name, and the supplied non-receiver arity is
    ///   accepted by required/defaulted parameter bounds.
    /// - `None` when no matching receiver method exists.
    ///
    /// Transformation:
    /// - Scans method buckets by source method name and applies min/full arity
    ///   checks so omitted defaulted parameters can still select the full
    ///   backend receiver target.
    pub(in crate::backends::erlang::emit) fn receiver_method_target(
        &self,
        receiver_type: &str,
        method: &str,
        arity: usize,
    ) -> Option<&SyntaxReceiverMethodTarget> {
        self.receiver_methods
            .iter()
            .filter(|((candidate_method, _), _)| candidate_method == method)
            .filter_map(|(_, receivers)| receivers.get(receiver_type))
            .find(|target| arity >= target.min_arity && arity <= target.fixed_arity)
    }
}
