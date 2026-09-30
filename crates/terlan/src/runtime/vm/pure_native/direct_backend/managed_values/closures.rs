//! Owned function captures at the in-process, generation-checked value boundary.

use std::ops::Deref;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    decode_field_word, field_word, managed_binary_semantic_id, managed_bytes_semantic_id,
    managed_string_semantic_id, ActorHeap, ManagedClosureDispatchTable, ManagedFieldType,
    ManagedLayoutRegistry, ManagedMemoryError, SemanticTypeId, TvmRef,
};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::{ReplValue, VmRuntimeError, VmRuntimeResult};

use super::materialization::{Children, Shape};
use super::{allocate_field, consume_budget, reference_mismatch, AllocationMemo};

/// Admitted schemas and optional executable membership used by one graph traversal.
pub(in super::super) struct PublicManagedMetadata<'a> {
    pub(in super::super) layouts: &'a ManagedLayoutRegistry,
    pub(in super::super) closures: Option<&'a ManagedClosureDispatchTable>,
}

impl Deref for PublicManagedMetadata<'_> {
    type Target = ManagedLayoutRegistry;

    fn deref(&self) -> &Self::Target {
        self.layouts
    }
}

impl PublicManagedMetadata<'_> {
    pub(super) fn is_closure(&self, semantic: SemanticTypeId) -> bool {
        self.closures
            .is_some_and(|table| table.contains_signature(semantic))
    }
}

pub(super) fn allocate(
    heap: &mut ActorHeap,
    metadata: &PublicManagedMetadata<'_>,
    semantic: SemanticTypeId,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> VmRuntimeResult<TvmRef<()>> {
    consume_budget(depth, budget)?;
    let ReplValue::Closure(value) = value else {
        return reference_mismatch("Function", value).map_err(Into::into);
    };
    let table = metadata.closures.ok_or_else(missing_generation)?;
    table
        .validate_descriptor(&value.descriptor, value.captures.len())
        .map_err(closure_error)?;
    let descriptor = &value.descriptor;
    if descriptor.semantic_id() != semantic {
        return Err(closure_error(ManagedMemoryError::ClosureSignatureMismatch));
    }
    let mut captures = Vec::with_capacity(value.captures.len());
    for (ty, value) in descriptor.captures().iter().zip(value.captures.iter()) {
        let field = allocate_field(
            heap,
            metadata,
            field_type(ty)?,
            value,
            depth + 1,
            budget,
            memo,
        )?;
        captures.push(i64::from_ne_bytes(field_word(field).to_ne_bytes()));
    }
    heap.allocate_closure(descriptor, &captures)
        .map(TvmRef::erase)
        .map_err(closure_error)
}

pub(super) fn read(
    heap: &ActorHeap,
    metadata: &PublicManagedMetadata<'_>,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
) -> VmRuntimeResult<Children<'static>> {
    let table = metadata.closures.ok_or_else(missing_generation)?;
    let view = heap.closure_view(reference.cast()).map_err(closure_error)?;
    table
        .resolve(&view, table.generation(), &view.parameters, &view.results)
        .map_err(closure_error)?;
    let descriptor = table
        .closure_descriptor(view.callable_id)
        .map_err(closure_error)?;
    if descriptor.semantic_id() != semantic {
        return Err(closure_error(ManagedMemoryError::ClosureSignatureMismatch));
    }
    let mut captures = Vec::with_capacity(view.capture_words.len());
    for (ty, word) in view.capture_types.iter().zip(&view.capture_words) {
        let ty = field_type(ty)?;
        let field = decode_field_word(ty, *word).map_err(closure_error)?;
        captures.push((ty, field));
    }
    Ok(Children {
        shape: Shape::Closure(Arc::new(descriptor)),
        fields: captures,
    })
}

fn field_type(ty: &TvmBoundaryType) -> VmRuntimeResult<ManagedFieldType> {
    Ok(match ty {
        TvmBoundaryType::Unit => ManagedFieldType::Unit,
        TvmBoundaryType::Bool => ManagedFieldType::Bool,
        TvmBoundaryType::Int => ManagedFieldType::Int,
        TvmBoundaryType::Float => ManagedFieldType::Float,
        TvmBoundaryType::Atom => ManagedFieldType::Atom,
        TvmBoundaryType::String => ManagedFieldType::Reference(managed_string_semantic_id()),
        TvmBoundaryType::Bytes => ManagedFieldType::Reference(managed_bytes_semantic_id()),
        TvmBoundaryType::Binary => ManagedFieldType::Reference(managed_binary_semantic_id()),
        TvmBoundaryType::Managed(id) => {
            ManagedFieldType::Reference(SemanticTypeId::from_bytes(*id))
        }
        TvmBoundaryType::Json | TvmBoundaryType::NativeResource(_) => {
            return Err(closure_error(ManagedMemoryError::InvalidClosure));
        }
    })
}

fn missing_generation() -> VmRuntimeError {
    "error[execution_shard.closure]: no admitted executable generation".into()
}

fn closure_error(error: ManagedMemoryError) -> VmRuntimeError {
    format!("error[execution_shard.closure]: {error}").into()
}
