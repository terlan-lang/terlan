//! Checked CoreIR to fixed NativeIR value-kind mapping.

use crate::runtime::native_image::managed::{ManagedClosureDescriptor, SemanticTypeId};
use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

use super::{scalar_types, NativeType};

pub(crate) fn native_type(core: Option<&CoreType>, text: &str) -> Option<NativeType> {
    match core {
        // Compiler-owned existential carrier, not a source-level Dynamic cast.
        // '$' cannot begin a user type name.
        Some(CoreType::Named(name)) if name == super::super::effect_values::ERASED_VALUE_TYPE => {
            crate::runtime::native_image::managed::managed_erased_value_semantic_id()
                .ok()
                .map(NativeType::ManagedRef)
        }
        Some(CoreType::Named(name)) if name == "Unit" => Some(NativeType::Unit),
        Some(CoreType::AtomLiteral(name)) if matches!(name.as_str(), "Unit" | "unit") => {
            Some(NativeType::Unit)
        }
        Some(CoreType::Int) => Some(NativeType::Int),
        Some(CoreType::Float) => Some(NativeType::Float),
        Some(CoreType::Bool) => Some(NativeType::Bool),
        Some(CoreType::Atom | CoreType::AtomLiteral(_)) => Some(NativeType::Atom),
        Some(CoreType::Union(variants))
            if variants
                .iter()
                .all(|variant| matches!(variant, CoreType::Atom | CoreType::AtomLiteral(_))) =>
        {
            Some(NativeType::Atom)
        }
        Some(CoreType::String) => Some(NativeType::StringRef),
        Some(core @ CoreType::Union(_)) if is_structural_string_option(core) => {
            managed_reference_type(&CoreType::Apply {
                constructor: "Option".to_string(),
                args: vec![CoreType::String],
            })
        }
        Some(CoreType::Binary) => Some(NativeType::BinaryRef),
        // Never has no value constructor or aggregate layout. Its reference
        // carrier lets empty collections retain a distinct checked schema;
        // it does not turn an uninhabited element into an ordinary Unit value.
        Some(core @ CoreType::Never) => managed_reference_type(core),
        Some(CoreType::Arrow {
            params,
            return_type,
        }) => {
            let parameters = params
                .iter()
                .map(|ty| native_type(Some(ty), &ty.contract_text()).map(NativeType::boundary_type))
                .collect::<Option<Vec<_>>>()?;
            let result =
                native_type(Some(return_type), &return_type.contract_text())?.boundary_type();
            ManagedClosureDescriptor::semantic_id_for_signature(&parameters, &[result])
                .ok()
                .map(NativeType::ManagedRef)
        }
        Some(CoreType::Named(name))
            if matches!(
                name.as_str(),
                "Bytes" | "std.binary.Bytes" | "std.vm.Bytes.Bytes"
            ) =>
        {
            Some(NativeType::BytesRef)
        }
        Some(CoreType::Named(name))
            if matches!(name.rsplit('.').next(), Some("Binary" | "BitString")) =>
        {
            Some(NativeType::BinaryRef)
        }
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Process") && args.len() == 1 =>
        {
            Some(NativeType::Int)
        }
        Some(CoreType::Apply { constructor, args })
            if matches!(
                constructor.rsplit('.').next(),
                Some("Entry" | "Monitor" | "ResourceKind" | "Resource")
            ) && args.len() == 1 =>
        {
            Some(NativeType::Int)
        }
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 =>
        {
            native_type(Some(&args[0]), &args[0].contract_text())
        }
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Iterator") && args.len() == 1 =>
        {
            let physical = CoreType::List(Box::new(args[0].clone()));
            managed_reference_type(&physical)
        }
        Some(CoreType::Named(name))
            if matches!(
                name.rsplit('.').next(),
                Some("Timer" | "ExitReason" | "SchedulingClass")
            ) =>
        {
            Some(NativeType::Int)
        }
        Some(core @ CoreType::Named(_)) => managed_reference_type(core),
        Some(
            core @ (CoreType::Tuple(_)
            | CoreType::Struct { .. }
            | CoreType::Map(_)
            | CoreType::Union(_)
            | CoreType::List(_)),
        ) => managed_reference_type(core),
        Some(core @ CoreType::Apply { constructor, .. })
            if scalar_types::managed_aggregate_constructor(constructor) =>
        {
            managed_reference_type(core)
        }
        None if text == "Unit" => Some(NativeType::Unit),
        None if text == "Int" => Some(NativeType::Int),
        None if text == "Float" => Some(NativeType::Float),
        None if text == "Bool" => Some(NativeType::Bool),
        None if text == "Atom" => Some(NativeType::Atom),
        None if text == "String" => Some(NativeType::StringRef),
        None if matches!(text, "Bytes" | "std.binary.Bytes" | "std.vm.Bytes.Bytes") => {
            Some(NativeType::BytesRef)
        }
        None if matches!(text, "Binary" | "BitString" | "std.binary.Binary") => {
            Some(NativeType::BinaryRef)
        }
        _ => None,
    }
}

/// Recognizes the exact transparent representation of `Option[String]` so
/// imported aliases and compiler-owned HTTP projections share one ABI identity.
fn is_structural_string_option(ty: &CoreType) -> bool {
    let CoreType::Union(variants) = ty else {
        return false;
    };
    variants.len() == 2
        && variants
            .iter()
            .any(|variant| matches!(variant, CoreType::AtomLiteral(tag) if tag == "none"))
        && variants.iter().any(|variant| {
            let CoreType::Tuple(elements) = variant else {
                return false;
            };
            let [tag, value] = elements.as_slice() else {
                return false;
            };
            matches!(tuple_element_type(tag), CoreType::AtomLiteral(tag) if tag == "some")
                && matches!(tuple_element_type(value), CoreType::String)
        })
}

fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

pub(in crate::compiler::native_ir) fn core_string_runtime_value(
    value: &str,
) -> Result<String, String> {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str(value)
            .map_err(|error| format!("error[native_ir.string_literal]: {error}"))
    } else {
        Ok(value.to_string())
    }
}

fn managed_reference_type(core: &CoreType) -> Option<NativeType> {
    let canonical = managed_semantic_contract(core);
    SemanticTypeId::from_canonical(&canonical)
        .ok()
        .map(NativeType::ManagedRef)
}

/// Returns the canonical managed ABI identity after applying compiler-owned
/// facade normalization. Constructor descriptors and public signatures must
/// use this same spelling or a value can have the right native kind but the
/// wrong heap semantic identity.
pub(in crate::compiler::native_ir) fn managed_semantic_contract(core: &CoreType) -> String {
    match core {
        // Concrete generic declarations retained by struct_instances already
        // carry their complete application contract as the internal name.
        CoreType::Struct { name, .. } if name.starts_with("Apply(") => name.clone(),
        CoreType::Struct { name, .. } => CoreType::Named(name.clone()).contract_text(),
        core @ CoreType::Union(_) if is_structural_string_option(core) => {
            "Apply(Option;String)".to_string()
        }
        _ => normalize_recursive_managed_type(core).contract_text(),
    }
}

/// Collapses repeated structural unrolling of the same recursive list union.
/// Type checking may expose one additional recursive layer at independent use
/// sites; managed identity must remain stable across those equivalent views.
pub(in crate::compiler::native_ir) fn normalize_recursive_managed_type(
    core: &CoreType,
) -> CoreType {
    match core {
        CoreType::Struct { name, .. } => CoreType::Named(name.clone()),
        CoreType::List(element) => {
            CoreType::List(Box::new(normalize_recursive_managed_type(element)))
        }
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args.iter().map(normalize_recursive_managed_type).collect(),
        },
        CoreType::Union(variants) => {
            let normalized = variants
                .iter()
                .map(normalize_recursive_managed_type)
                .collect::<Vec<_>>();
            for variant in &normalized {
                let Some(CoreType::Union(inner)) = list_type_element(variant) else {
                    continue;
                };
                if recursive_list_alias(inner).is_some()
                    && non_list_variants(&normalized) == non_list_variants(inner)
                {
                    return CoreType::Union(inner.clone());
                }
            }
            CoreType::Union(normalized)
        }
        _ => core.clone(),
    }
}

/// Returns one list element from either checked list representation.
fn list_type_element(core: &CoreType) -> Option<&CoreType> {
    match core {
        CoreType::List(element) => Some(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

/// Finds the nominal recursion anchor in a structural list union.
fn recursive_list_alias(variants: &[CoreType]) -> Option<&str> {
    let mut names = variants.iter().filter_map(|variant| {
        let CoreType::Named(name) = list_type_element(variant)? else {
            return None;
        };
        Some(name.as_str())
    });
    let name = names.next()?;
    names.next().is_none().then_some(name)
}

/// Returns ordered non-list variants for structural recursion comparison.
fn non_list_variants(variants: &[CoreType]) -> Vec<&CoreType> {
    variants
        .iter()
        .filter(|variant| list_type_element(variant).is_none())
        .collect()
}

/// Recovers the semantic type of a concrete homogeneous collection literal.
pub(crate) fn literal_collection_type(expr: &CoreExpr) -> Option<CoreType> {
    let CoreExpr::List(items) = expr else {
        return None;
    };
    super::collection_literal_types::homogeneous_list_type(items, literal_value_type)
}

/// Recovers a checked homogeneous list type when at least one item has a
/// concrete literal type and the remaining items are typed expressions.
///
/// This is intentionally used for native-image metadata inventory only. The
/// typechecker has already established list homogeneity; inventory merely
/// needs one concrete witness so lists such as `["prefix", value]` retain
/// their runtime collection descriptor.
pub(crate) fn witnessed_collection_type(expr: &CoreExpr) -> Option<CoreType> {
    let CoreExpr::List(items) = expr else {
        return None;
    };
    let mut known = items.iter().filter_map(literal_value_type);
    let element = known.next()?;
    known
        .all(|item| item == element)
        .then_some(CoreType::List(Box::new(element)))
}

fn literal_value_type(expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::UnaryOp { operator, operand } if operator == "-" => {
            match literal_value_type(operand) {
                Some(ty @ (CoreType::Int | CoreType::Float)) => Some(ty),
                _ => None,
            }
        }
        CoreExpr::List(_) => literal_collection_type(expr),
        CoreExpr::Tuple(items) => items
            .iter()
            .map(literal_value_type)
            .map(|item| item.map(crate::terlan_typeck::CoreTupleTypeElem::Type))
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        _ => None,
    }
}

pub(super) fn is_empty_list(expr: &CoreExpr) -> bool {
    matches!(expr, CoreExpr::List(items) if items.is_empty())
}
