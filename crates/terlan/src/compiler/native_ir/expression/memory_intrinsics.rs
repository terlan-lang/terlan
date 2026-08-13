//! Type-directed memory introspection lowering.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_scalar_field_operation, encode_memory_retained_size_operation,
    encode_memory_shallow_size_operation,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType};

use super::super::aggregate_types::memory_layout_descriptor;
use super::{
    lower_expr_with_constructors, native_type, NativeConstructorLayouts, NativeExpr, NativeType,
};

/// Returns the native result type for one memory intrinsic.
pub(super) fn infer_memory_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match &call.id {
        CoreIntrinsicId::MemoryLayoutOf(_) => {
            native_type(Some(&call.return_type), &call.return_type.contract_text())
        }
        CoreIntrinsicId::MemoryShallowSize(_) | CoreIntrinsicId::MemoryRetainedSize(_) => {
            Some(NativeType::Int)
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MemoryLayoutSize
            | CorePrimitiveIntrinsic::MemoryLayoutAlignment,
        ) => Some(NativeType::Int),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MemoryLayoutStorage) => {
            Some(NativeType::Atom)
        }
        _ => None,
    }
}

/// Lowers one type-directed memory intrinsic into fixed NativeIR operations.
pub(super) fn lower_memory_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Option<Result<NativeExpr, String>> {
    let (value_type, operation) = match &call.id {
        CoreIntrinsicId::MemoryLayoutOf(value_type) => {
            return Some(lower_layout_of(value_type, call, constructors));
        }
        CoreIntrinsicId::MemoryShallowSize(value_type) => {
            (value_type, encode_memory_shallow_size_operation())
        }
        CoreIntrinsicId::MemoryRetainedSize(value_type) => {
            (value_type, encode_memory_retained_size_operation())
        }
        CoreIntrinsicId::Primitive(
            operation @ (CorePrimitiveIntrinsic::MemoryLayoutSize
            | CorePrimitiveIntrinsic::MemoryLayoutAlignment
            | CorePrimitiveIntrinsic::MemoryLayoutStorage),
        ) => {
            return Some(lower_layout_projection(
                operation,
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            ));
        }
        _ => return None,
    };
    Some((|| {
        let [value] = call.args.as_slice() else {
            return Err(
                "error[native_ir.memory_arity]: memory size requires one value".to_string(),
            );
        };
        let representation = representation_layout(value_type)?;
        if !representation.storage.is_managed() {
            return Ok(NativeExpr::Int(representation.size));
        }
        let value = lower_expr_with_constructors(
            value,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?;
        Ok(NativeExpr::ManagedOperation {
            encoded: Arc::from(operation),
            args: vec![value],
        })
    })())
}

/// Lowers one immutable `Memory.Layout` field projection.
fn lower_layout_projection(
    operation: &CorePrimitiveIntrinsic,
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let [layout_value] = call.args.as_slice() else {
        return Err(
            "error[native_ir.memory_layout_arity]: layout projection requires one value"
                .to_string(),
        );
    };
    let field_name = match operation {
        CorePrimitiveIntrinsic::MemoryLayoutSize => "size",
        CorePrimitiveIntrinsic::MemoryLayoutAlignment => "alignment",
        CorePrimitiveIntrinsic::MemoryLayoutStorage => "storage",
        _ => {
            return Err(
                "error[native_ir.memory_layout_operation]: unknown layout projection".to_string(),
            );
        }
    };
    let (descriptor, _) = memory_layout_descriptor()?;
    let field = descriptor
        .fields()
        .iter()
        .position(|field| field.name() == Some(field_name))
        .ok_or_else(|| {
            format!(
                "error[native_ir.memory_layout_field]: Memory.Layout has no `{field_name}` field"
            )
        })?;
    let encoded =
        encode_aggregate_scalar_field_operation(descriptor.managed().semantic_id(), field)
            .map_err(|error| format!("error[native_ir.memory_layout_projection]: {error}"))?;
    let value = lower_expr_with_constructors(
        layout_value,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    Ok(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: vec![value],
    })
}

/// Materializes the target layout descriptor as a managed `Memory.Layout`.
fn lower_layout_of(
    value_type: &CoreType,
    call: &CoreIntrinsicCall,
    _constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    if !call.args.is_empty() {
        return Err("error[native_ir.memory_arity]: layout_of accepts no values".to_string());
    }
    let representation =
        representation_layout(value_type).unwrap_or_else(|_| RepresentationLayout::opaque());
    let (descriptor, encoded_layout) = memory_layout_descriptor()?;
    let fields = descriptor
        .fields()
        .iter()
        .map(|field| match field.name() {
            Some("size") => Ok(NativeExpr::Int(representation.size)),
            Some("alignment") => Ok(NativeExpr::Int(representation.alignment)),
            Some("storage") => Ok(NativeExpr::AtomLiteral(Arc::from(
                representation.storage.atom(),
            ))),
            Some(name) => Err(format!(
                "error[native_ir.memory_layout_field]: unknown Memory.Layout field `{name}`"
            )),
            None => Err(
                "error[native_ir.memory_layout_field]: Memory.Layout fields must be named"
                    .to_string(),
            ),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields,
    })
}

/// Computes the selected target's direct value representation.
fn representation_layout(value_type: &CoreType) -> Result<RepresentationLayout, String> {
    let native = native_type(Some(value_type), &value_type.contract_text()).ok_or_else(|| {
        format!(
            "error[native_ir.memory_type]: `{}` has no admitted physical representation",
            value_type.contract_text()
        )
    })?;
    Ok(match native {
        NativeType::Unit => RepresentationLayout::inline(0, 1),
        NativeType::Bool => RepresentationLayout::inline(1, 1),
        NativeType::Atom => RepresentationLayout::inline(4, 4),
        NativeType::Int | NativeType::Float => RepresentationLayout::inline(8, 8),
        NativeType::StringRef
        | NativeType::BytesRef
        | NativeType::BinaryRef
        | NativeType::ManagedRef(_) => RepresentationLayout::managed_reference(),
    })
}

/// Closed physical storage category exposed by `std.core.Memory`.
#[derive(Clone, Copy)]
enum RepresentationStorage {
    Inline,
    Managed,
    Opaque,
}

impl RepresentationStorage {
    /// Returns whether runtime graph accounting is required.
    fn is_managed(self) -> bool {
        matches!(self, Self::Managed)
    }

    /// Returns the corresponding Terlan atom constructor.
    fn atom(self) -> &'static str {
        match self {
            Self::Inline => "Inline",
            Self::Managed => "Managed",
            Self::Opaque => "Opaque",
        }
    }
}

/// Fixed size, alignment, and storage selected for one native value slot.
#[derive(Clone, Copy)]
struct RepresentationLayout {
    size: i64,
    alignment: i64,
    storage: RepresentationStorage,
}

impl RepresentationLayout {
    /// Builds one directly embedded value representation.
    fn inline(size: i64, alignment: i64) -> Self {
        Self {
            size,
            alignment,
            storage: RepresentationStorage::Inline,
        }
    }

    /// Builds the pointer-width actor-local managed-reference representation.
    fn managed_reference() -> Self {
        Self {
            size: 8,
            alignment: 8,
            storage: RepresentationStorage::Managed,
        }
    }

    /// Builds a layout for a type whose target representation is not inspectable.
    fn opaque() -> Self {
        Self {
            size: 0,
            alignment: 1,
            storage: RepresentationStorage::Opaque,
        }
    }
}
