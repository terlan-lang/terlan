//! Checked CoreIR to fixed NativeIR value-kind mapping.

use crate::runtime::native_image::managed::{ManagedClosureDescriptor, SemanticTypeId};
use crate::terlan_typeck::{CoreExpr, CoreType};

use super::{scalar_types, NativeType};

pub(crate) fn native_type(core: Option<&CoreType>, text: &str) -> Option<NativeType> {
    match core {
        Some(CoreType::Named(name)) if name == "Unit" => Some(NativeType::Unit),
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
        Some(CoreType::Named(name))
            if super::super::template_values::is_template_html_type(name) =>
        {
            Some(NativeType::StringRef)
        }
        Some(CoreType::Binary) => Some(NativeType::BinaryRef),
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

pub(super) fn core_string_runtime_value(value: &str) -> Result<String, String> {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str(value)
            .map_err(|error| format!("error[native_ir.string_literal]: {error}"))
    } else {
        Ok(value.to_string())
    }
}

fn managed_reference_type(core: &CoreType) -> Option<NativeType> {
    let canonical = match core {
        CoreType::Struct { name, .. } => name.clone(),
        _ => core.contract_text(),
    };
    SemanticTypeId::from_canonical(&canonical)
        .ok()
        .map(NativeType::ManagedRef)
}

/// Recovers the semantic type of a concrete homogeneous collection literal.
pub(crate) fn literal_collection_type(expr: &CoreExpr) -> Option<CoreType> {
    let CoreExpr::List(items) = expr else {
        return None;
    };
    let mut item_types = items.iter().map(literal_value_type);
    let element = item_types.next()??;
    item_types
        .all(|item| item.as_ref() == Some(&element))
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
        _ => None,
    }
}

pub(super) fn is_empty_list(expr: &CoreExpr) -> bool {
    matches!(expr, CoreExpr::List(items) if items.is_empty())
}
