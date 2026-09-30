//! Bounded, nonrecursive decoding of compound managed graphs.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    managed_binary_semantic_id, managed_bytes_semantic_id, managed_string_semantic_id, ActorHeap,
    ManagedAggregateDescriptor, ManagedClosureDescriptor, ManagedCollectionKind, ManagedFieldType,
    ManagedFieldValue, SemanticTypeId, TvmRef,
};
use crate::runtime::vm::{NativeClosureValue, ReplValue, VmRuntimeResult};

use super::{
    closures, consume_budget, managed_read_error, materialize_field,
    public_shapes::public_aggregate, ActiveReference, ActiveReferences, PublicManagedMetadata,
};

pub(super) enum Shape<'a> {
    Aggregate(&'a ManagedAggregateDescriptor),
    Closure(Arc<ManagedClosureDescriptor>),
    List,
    Map,
    Set,
}

pub(super) struct Children<'a> {
    pub(super) shape: Shape<'a>,
    pub(super) fields: Vec<(ManagedFieldType, ManagedFieldValue)>,
}

enum Task<'a> {
    Reference {
        semantic: SemanticTypeId,
        reference: TvmRef<()>,
        depth: usize,
    },
    Field {
        ty: ManagedFieldType,
        value: ManagedFieldValue,
        depth: usize,
    },
    Finish {
        shape: Shape<'a>,
        count: usize,
        identity: ActiveReference,
    },
}

pub(super) fn materialize(
    heap: &ActorHeap,
    metadata: &PublicManagedMetadata<'_>,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> VmRuntimeResult<ReplValue> {
    let original_depth = active.len();
    let result = traverse(
        heap,
        metadata,
        Task::Reference {
            semantic,
            reference,
            depth,
        },
        budget,
        active,
    );
    active.truncate(original_depth);
    result
}

fn traverse<'a>(
    heap: &ActorHeap,
    metadata: &'a PublicManagedMetadata<'_>,
    root: Task<'a>,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> VmRuntimeResult<ReplValue> {
    let mut tasks = vec![root];
    let mut values = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Reference {
                semantic,
                reference,
                depth,
            } => {
                consume_budget(depth, budget)?;
                let identity = ActiveReference {
                    semantic,
                    identity: reference.encoded_abi_word(),
                };
                if active.contains(&identity) {
                    return Err(
                        "error[execution_shard.managed_cycle]: cyclic public managed value".into(),
                    );
                }
                active.push(identity);
                let children = read_children(heap, metadata, semantic, reference)?;
                if children.fields.len() > *budget {
                    return Err("error[execution_shard.managed_budget]: public managed value exceeds conversion limits".into());
                }
                tasks.push(Task::Finish {
                    shape: children.shape,
                    count: children.fields.len(),
                    identity,
                });
                tasks.extend(
                    children
                        .fields
                        .into_iter()
                        .rev()
                        .map(|(ty, value)| Task::Field {
                            ty,
                            value,
                            depth: depth + 1,
                        }),
                );
            }
            Task::Field { ty, value, depth } => {
                if let (
                    ManagedFieldType::Reference(semantic),
                    ManagedFieldValue::Reference(reference),
                ) = (ty, value)
                {
                    if semantic != managed_string_semantic_id()
                        && semantic != managed_bytes_semantic_id()
                        && semantic != managed_binary_semantic_id()
                    {
                        consume_budget(depth, budget)?;
                        tasks.push(Task::Reference {
                            semantic,
                            reference,
                            depth,
                        });
                        continue;
                    }
                }
                values.push(materialize_field(
                    heap, metadata, ty, value, depth, budget, active,
                )?);
            }
            Task::Finish {
                shape,
                count,
                identity,
            } => {
                let start = values
                    .len()
                    .checked_sub(count)
                    .ok_or("error[execution_shard.managed_graph]: incomplete fields")?;
                let fields = values.split_off(start);
                values.push(finish(shape, fields)?);
                let completed = active.pop();
                debug_assert_eq!(completed, Some(identity));
            }
        }
    }
    if values.len() != 1 {
        return Err("error[execution_shard.managed_graph]: invalid result count".into());
    }
    Ok(values.pop().expect("checked result count"))
}

fn finish(shape: Shape<'_>, fields: Vec<ReplValue>) -> VmRuntimeResult<ReplValue> {
    Ok(match shape {
        Shape::Aggregate(descriptor) => public_aggregate(descriptor, fields),
        Shape::Closure(descriptor) => ReplValue::Closure(Arc::new(NativeClosureValue {
            descriptor,
            captures: fields.into_boxed_slice(),
        })),
        Shape::List => ReplValue::List(fields),
        Shape::Set => ReplValue::Set(fields),
        Shape::Map => {
            let mut entries = Vec::with_capacity(fields.len() / 2);
            let mut fields = fields.into_iter();
            while let Some(key) = fields.next() {
                let value = fields
                    .next()
                    .ok_or("error[execution_shard.managed_graph]: missing map value")?;
                entries.push((key, value));
            }
            ReplValue::Map(entries)
        }
    })
}

fn read_children<'a>(
    heap: &ActorHeap,
    metadata: &'a PublicManagedMetadata<'_>,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
) -> VmRuntimeResult<Children<'a>> {
    if metadata.is_closure(semantic) {
        return closures::read(heap, metadata, semantic, reference);
    }
    if let Some(descriptor) = metadata.collection(semantic) {
        return Ok(match descriptor.kind() {
            ManagedCollectionKind::List => {
                let list = descriptor.list_descriptor().expect("checked list schema");
                Children {
                    shape: Shape::List,
                    fields: heap
                        .list_elements(list, reference.cast())
                        .map_err(managed_read_error)?
                        .into_iter()
                        .map(|value| (list.element_type(), value))
                        .collect(),
                }
            }
            ManagedCollectionKind::Map => {
                let map = descriptor.map_descriptor().expect("checked map schema");
                Children {
                    shape: Shape::Map,
                    fields: heap
                        .map_entries(map, reference.cast())
                        .map_err(managed_read_error)?
                        .into_iter()
                        .flat_map(|(key, value)| [(map.key_type(), key), (map.value_type(), value)])
                        .collect(),
                }
            }
            ManagedCollectionKind::Set => {
                let set = descriptor.set_descriptor().expect("checked set schema");
                Children {
                    shape: Shape::Set,
                    fields: heap
                        .set_elements(set, reference.cast())
                        .map_err(managed_read_error)?
                        .into_iter()
                        .map(|value| (set.element_type(), value))
                        .collect(),
                }
            }
        });
    }
    let descriptor = metadata.layout_for_reference(heap, semantic, reference)?;
    let view = heap
        .read_aggregate(reference.cast(), descriptor)
        .map_err(managed_read_error)?;
    let fields = descriptor
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            view.field(index)
                .map(|value| (field.field_type(), value))
                .map_err(managed_read_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Children {
        shape: Shape::Aggregate(descriptor),
        fields,
    })
}
