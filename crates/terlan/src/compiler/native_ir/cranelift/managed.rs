//! Managed-allocation lowering for direct-AOT native functions.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use cranelift_codegen::ir::{
    types, AbiParam, Block, BlockArg, InstBuilder, Signature, StackSlotData, StackSlotKind, Value,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;
use cranelift_object::ObjectModule;

use super::super::{status, NativeExpr, NativeModule};

/// Aggregate descriptors admitted for one native object.
pub(super) struct ManagedLayouts {
    encoded: HashSet<Arc<[u8]>>,
    atom_words: HashMap<String, i64>,
}

impl ManagedLayouts {
    /// Inventories every constructor descriptor admitted to generated functions.
    pub(super) fn declare(
        _module: &mut ObjectModule,
        natives: &[NativeModule],
    ) -> Result<Self, String> {
        let mut layouts = Vec::<Arc<[u8]>>::new();
        for native in natives {
            for function in &native.functions {
                collect_layouts(&function.body, &mut layouts);
            }
            for continuation in &native.continuations {
                collect_layouts(&continuation.body, &mut layouts);
            }
        }
        let admitted = natives
            .iter()
            .flat_map(|native| native.managed_layouts.iter().cloned())
            .collect::<HashSet<_>>();
        for layout in &layouts {
            if admitted.contains(layout) {
                continue;
            }
            let Ok(descriptor) =
                crate::runtime::native_image::managed::decode_aggregate_layout(layout)
            else {
                continue;
            };
            return Err(format!(
                "error[cranelift.managed_layout_metadata]: body layout `{}` variant {:?} with fields {:?} is absent from image metadata",
                descriptor.canonical_type(),
                descriptor.variant_name(),
                descriptor
                    .fields()
                    .iter()
                    .map(|field| (field.name(), field.field_type()))
                    .collect::<Vec<_>>()
            ));
        }
        let atom_words = natives
            .iter()
            .flat_map(|native| native.atoms.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(index, identity)| {
                i64::try_from(index)
                    .map(|index| (identity, index))
                    .map_err(|_| {
                        "error[cranelift.atom_table]: atom index exceeds native word".to_string()
                    })
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(Self {
            encoded: layouts.into_iter().collect(),
            atom_words,
        })
    }

    /// Confirms that one encoded aggregate layout belongs to the admitted object.
    fn admit(&self, layout: &Arc<[u8]>) -> Result<(), String> {
        self.encoded.contains(layout).then_some(()).ok_or_else(|| {
            "error[cranelift.managed_layout]: constructor layout was not inventoried".to_string()
        })
    }

    /// Resolves one semantic atom to the compact index of the emitted image.
    pub(super) fn atom_word(&self, identity: &str) -> Result<i64, String> {
        self.atom_words.get(identity).copied().ok_or_else(|| {
            format!("error[cranelift.atom_literal]: atom `{identity}` was not inventoried")
        })
    }
}

/// One admitted aggregate allocation emitted into generated code.
pub(super) struct ManagedAllocation<'a> {
    pub(super) encoded_layout: &'a Arc<[u8]>,
    pub(super) fields: &'a [Value],
}

/// VM-owned state required by the generated managed allocator callback.
pub(super) struct ManagedAllocationRuntime {
    pub(super) context: Value,
    pub(super) allocator: Value,
}

/// Emits one checked call to the VM-owned managed aggregate allocator.
pub(super) fn emit_managed_allocation(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    layouts: &ManagedLayouts,
    allocation: ManagedAllocation<'_>,
    runtime: ManagedAllocationRuntime,
    error_block: Block,
) -> Result<Value, String> {
    let ManagedAllocation {
        encoded_layout,
        fields,
    } = allocation;
    let ManagedAllocationRuntime {
        context: runtime_context,
        allocator,
    } = runtime;
    let pointer = module.target_config().pointer_type();
    let allocator_missing =
        builder
            .ins()
            .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::Equal, allocator, 0);
    branch_on_error(
        builder,
        allocator_missing,
        status::MANAGED_RUNTIME_UNAVAILABLE,
        error_block,
    );

    let field_bytes = fields.len().checked_mul(8).ok_or_else(|| {
        "error[cranelift.managed_fields]: aggregate field storage overflows usize".to_string()
    })?;
    let stack_bytes = u32::try_from(field_bytes.max(8)).map_err(|_| {
        "error[cranelift.managed_fields]: aggregate field storage exceeds u32".to_string()
    })?;
    let field_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        stack_bytes,
        3,
    ));
    for (index, field) in fields.iter().enumerate() {
        let offset = i32::try_from(index.saturating_mul(8)).map_err(|_| {
            "error[cranelift.managed_fields]: aggregate field offset exceeds i32".to_string()
        })?;
        builder.ins().stack_store(*field, field_slot, offset);
    }
    let result_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().stack_store(zero, result_slot, 0);

    layouts.admit(encoded_layout)?;
    let layout_bytes = u32::try_from(encoded_layout.len().max(1)).map_err(|_| {
        "error[cranelift.managed_layout]: descriptor storage exceeds u32".to_string()
    })?;
    let layout_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        layout_bytes,
        0,
    ));
    for (index, byte) in encoded_layout.iter().copied().enumerate() {
        let offset = i32::try_from(index).map_err(|_| {
            "error[cranelift.managed_layout]: descriptor offset exceeds i32".to_string()
        })?;
        let value = builder.ins().iconst(types::I8, i64::from(byte));
        builder.ins().stack_store(value, layout_slot, offset);
    }
    let layout_pointer = builder.ins().stack_addr(pointer, layout_slot, 0);
    let layout_length = i64::try_from(encoded_layout.len()).map_err(|_| {
        "error[cranelift.managed_layout]: descriptor length exceeds i64".to_string()
    })?;
    let layout_length = builder.ins().iconst(types::I64, layout_length);
    let fields_pointer = builder.ins().stack_addr(pointer, field_slot, 0);
    let field_count = i64::try_from(fields.len()).map_err(|_| {
        "error[cranelift.managed_fields]: aggregate field count exceeds i64".to_string()
    })?;
    let field_count = builder.ins().iconst(types::I64, field_count);
    let result_pointer = builder.ins().stack_addr(pointer, result_slot, 0);
    let signature = Signature {
        params: vec![
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv: module.target_config().default_call_conv,
    };
    let signature = builder.import_signature(signature);
    let call = builder.ins().call_indirect(
        signature,
        allocator,
        &[
            runtime_context,
            layout_pointer,
            layout_length,
            fields_pointer,
            field_count,
            result_pointer,
        ],
    );
    let callback_status = builder.inst_results(call)[0];
    let failed = builder.ins().icmp_imm(
        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
        callback_status,
        i64::from(status::OK),
    );
    let next = builder.create_block();
    let error = [BlockArg::Value(callback_status)];
    builder.ins().brif(failed, error_block, &error, next, &[]);
    builder.switch_to_block(next);
    let result = builder.ins().stack_load(types::I64, result_slot, 0);
    if crate::runtime::native_image::managed::managed_abi_result_is_reference(encoded_layout) {
        builder.declare_value_needs_stack_map(result);
        let invalid_reference =
            builder
                .ins()
                .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::Equal, result, 0);
        branch_on_error(
            builder,
            invalid_reference,
            status::INVALID_MANAGED_REFERENCE,
            error_block,
        );
    }
    Ok(result)
}

/// Recursively inventories every managed constructor descriptor in one body.
fn collect_layouts(expr: &NativeExpr, layouts: &mut Vec<Arc<[u8]>>) {
    match expr {
        NativeExpr::ManagedLiteral { encoded } => layouts.push(encoded.clone()),
        NativeExpr::ManagedOperation { encoded, args } => {
            layouts.push(encoded.clone());
            args.iter()
                .for_each(|argument| collect_layouts(argument, layouts));
        }
        NativeExpr::MakeClosure { encoded, captures } => {
            layouts.push(encoded.clone());
            captures
                .iter()
                .for_each(|capture| collect_layouts(capture, layouts));
        }
        NativeExpr::Construct {
            encoded_layout,
            fields,
            ..
        } => {
            layouts.push(encoded_layout.clone());
            fields
                .iter()
                .for_each(|field| collect_layouts(field, layouts));
        }
        NativeExpr::Call { args, .. }
        | NativeExpr::TailCall { args, .. }
        | NativeExpr::ContinuationTailCall { args, .. } => args
            .iter()
            .for_each(|argument| collect_layouts(argument, layouts)),
        NativeExpr::InvokeClosure { callee, args, .. } => {
            collect_layouts(callee, layouts);
            args.iter()
                .for_each(|argument| collect_layouts(argument, layouts));
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            values,
            ..
        } => {
            collect_layouts(callee, layouts);
            args.iter()
                .chain(values)
                .for_each(|argument| collect_layouts(argument, layouts));
        }
        NativeExpr::CallThen { args, values, .. } => {
            args.iter()
                .chain(values)
                .for_each(|value| collect_layouts(value, layouts));
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => collect_layouts(value, layouts),
        NativeExpr::Binary { left, right, .. } => {
            collect_layouts(left, layouts);
            collect_layouts(right, layouts);
        }
        NativeExpr::Let { bindings, body } => {
            bindings
                .iter()
                .for_each(|binding| collect_layouts(binding, layouts));
            collect_layouts(body, layouts);
        }
        NativeExpr::If { clauses } => clauses.iter().for_each(|(condition, body)| {
            collect_layouts(condition, layouts);
            collect_layouts(body, layouts);
        }),
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            collect_layouts(protected, layouts);
            collect_layouts(success, layouts);
            collect_layouts(failure, layouts);
            cleanup
                .iter()
                .for_each(|expression| collect_layouts(expression, layouts));
        }
        NativeExpr::Suspend {
            arguments, values, ..
        } => arguments
            .iter()
            .chain(values)
            .for_each(|value| collect_layouts(value, layouts)),
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::Param(_) => {}
    }
}

/// Routes a failed precondition to the native function's shared error block.
fn branch_on_error(
    builder: &mut FunctionBuilder<'_>,
    failed: Value,
    status: i32,
    error_block: Block,
) {
    let next = builder.create_block();
    let status = builder.ins().iconst(types::I32, i64::from(status));
    let error = [BlockArg::Value(status)];
    builder.ins().brif(failed, error_block, &error, next, &[]);
    builder.switch_to_block(next);
}
