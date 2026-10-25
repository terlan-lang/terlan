use super::*;

pub(super) fn validate_function(
    module: &IrModule,
    function: &IrFunction,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if function.capture_count > function.arity || function.capture_count > function.parameters.len()
    {
        diagnostics.push(diagnostic(
            "capture-count-exceeds-arity",
            "function capture count must not exceed its arity or parameter count",
            Some(&function.name),
            None,
        ));
    }
    if abi_minor < 4 && function.capture_count != 0 {
        diagnostics.push(diagnostic(
            "capture-count-requires-abi-1.4",
            "function capture metadata requires ABI 1.4",
            Some(&function.name),
            None,
        ));
    }
    if function.module_name != module.name {
        diagnostics.push(diagnostic(
            "function-module-mismatch",
            "function module_name must equal its containing module name",
            Some(&function.name),
            None,
        ));
    }
    if function.name.is_empty() {
        diagnostics.push(diagnostic(
            "missing-function-name",
            "function name must not be empty",
            Some(&function.name),
            None,
        ));
    }
    if function.arity != function.parameters.len() {
        diagnostics.push(diagnostic(
            "arity-parameter-mismatch",
            "function arity must equal its parameter count",
            Some(&function.name),
            None,
        ));
    }
    if function.return_type.is_empty() {
        diagnostics.push(diagnostic(
            "missing-return-type",
            "function return type must not be empty",
            Some(&function.name),
            None,
        ));
    }
    if function.receiver_command && !function.receiver_mutable {
        diagnostics.push(diagnostic(
            "receiver-command-without-mutable-receiver",
            "receiver command must declare a mutable receiver",
            Some(&function.name),
            None,
        ));
    }
    if function.receiver_mutable && function.parameters.is_empty() {
        diagnostics.push(diagnostic(
            "mutable-receiver-missing-parameter",
            "mutable receiver function must have a receiver parameter",
            Some(&function.name),
            None,
        ));
    }
    if function.receiver_command
        && function
            .parameters
            .first()
            .map(|parameter| parameter.type_name.as_str())
            != Some(function.return_type.as_str())
    {
        diagnostics.push(diagnostic(
            "receiver-command-return-mismatch",
            "receiver command internal return type must equal its receiver type",
            Some(&function.name),
            None,
        ));
    }

    let Some(entry) = function.blocks.first() else {
        diagnostics.push(diagnostic(
            "missing-entry-block",
            "function must contain an entry block",
            Some(&function.name),
            None,
        ));
        return;
    };
    if entry.label != "entry" {
        diagnostics.push(diagnostic(
            "invalid-entry-block",
            "first function block must be named entry",
            Some(&function.name),
            Some(&entry.label),
        ));
    }

    let mut labels = HashSet::new();
    for block in &function.blocks {
        if block.label.is_empty() {
            diagnostics.push(diagnostic(
                "missing-block-label",
                "block label must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        } else if !labels.insert(block.label.as_str()) {
            diagnostics.push(diagnostic(
                "duplicate-block-label",
                "block labels must be unique inside a function",
                Some(&function.name),
                Some(&block.label),
            ));
        }
    }

    for block in &function.blocks {
        validate_terminator(function, block, &labels, diagnostics);
        validate_instructions(function, block, &labels, abi_minor, diagnostics);
    }
}

pub(super) fn validate_terminator(
    function: &IrFunction,
    block: &IrBlock,
    labels: &HashSet<&str>,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    let expected_targets = match block.terminator.opcode.as_str() {
        "return" | "tail-call" | "unreachable" => Some(0),
        "jump" => Some(1),
        "branch" => Some(2),
        _ => None,
    };
    match expected_targets {
        None => diagnostics.push(diagnostic(
            "unsupported-terminator",
            "terminator is not part of self-host IR ABI 1",
            Some(&function.name),
            Some(&block.label),
        )),
        Some(expected) if block.terminator.targets.len() != expected => {
            diagnostics.push(diagnostic(
                "invalid-terminator-target-count",
                format!(
                    "{} terminator requires {expected} target(s)",
                    block.terminator.opcode
                ),
                Some(&function.name),
                Some(&block.label),
            ))
        }
        Some(_) => {}
    }
    let valid_operand_count = match block.terminator.opcode.as_str() {
        "return" | "branch" => block.terminator.operands.len() == 1,
        "jump" | "unreachable" => block.terminator.operands.is_empty(),
        "tail-call" => !block.terminator.operands.is_empty(),
        _ => true,
    };
    if !valid_operand_count {
        diagnostics.push(diagnostic(
            "invalid-terminator-operand-count",
            format!(
                "{} terminator has an invalid operand count",
                block.terminator.opcode
            ),
            Some(&function.name),
            Some(&block.label),
        ));
    }
    for target in &block.terminator.targets {
        if !labels.contains(target.as_str()) {
            diagnostics.push(diagnostic(
                "missing-block-target",
                format!("terminator target {target:?} does not exist"),
                Some(&function.name),
                Some(&block.label),
            ));
        }
    }
}

pub(super) fn validate_instructions(
    function: &IrFunction,
    block: &IrBlock,
    labels: &HashSet<&str>,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    let mut results = HashSet::new();
    for instruction in &block.instructions {
        if instruction.opcode.is_empty() {
            diagnostics.push(diagnostic(
                "missing-instruction-opcode",
                "instruction opcode must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if !instruction_opcode_supported(&instruction.opcode) {
            diagnostics.push(diagnostic(
                "unsupported-instruction",
                format!(
                    "instruction opcode {:?} is not part of self-host IR ABI 1",
                    instruction.opcode
                ),
                Some(&function.name),
                Some(&block.label),
            ));
        } else if !instruction_operands_valid(instruction, labels) {
            diagnostics.push(diagnostic(
                "invalid-instruction-operands",
                format!("{} instruction has invalid operands", instruction.opcode),
                Some(&function.name),
                Some(&block.label),
            ));
        }
        validate_instruction_try_region(function, block, instruction, abi_minor, diagnostics);
        if instruction.type_name.is_empty() {
            diagnostics.push(diagnostic(
                "missing-instruction-type",
                "instruction type must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if abi_minor >= 3 && instruction.operand_types.len() != instruction.operands.len() {
            diagnostics.push(diagnostic(
                "operand-type-count-mismatch",
                format!(
                    "{} instruction has {} operand(s) but {} operand type slot(s)",
                    instruction.opcode,
                    instruction.operands.len(),
                    instruction.operand_types.len()
                ),
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if instruction
            .operand_types
            .iter()
            .any(|type_name| matches!(type_name, Some(type_name) if type_name.is_empty()))
        {
            diagnostics.push(diagnostic(
                "empty-operand-type",
                "instruction operand type slots must be absent or non-empty",
                Some(&function.name),
                Some(&block.label),
            ));
        }
        let Some(result) = &instruction.result else {
            diagnostics.push(diagnostic(
                "missing-instruction-result",
                "ABI 1 instructions must produce a typed result",
                Some(&function.name),
                Some(&block.label),
            ));
            continue;
        };
        if result.is_empty() || !results.insert(result.as_str()) {
            diagnostics.push(diagnostic(
                "invalid-instruction-result",
                "instruction results must be non-empty and unique inside a block",
                Some(&function.name),
                Some(&block.label),
            ));
        }
    }
}

pub(super) fn validate_instruction_try_region(
    function: &IrFunction,
    block: &IrBlock,
    instruction: &IrInstruction,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if (instruction.opcode == "try-region" || instruction.try_region.is_some()) && abi_minor < 6 {
        diagnostics.push(diagnostic(
            "try-region-requires-abi-1.6",
            "structured try regions require ABI 1.6",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    match (instruction.opcode.as_str(), instruction.try_region.as_ref()) {
        ("try-region", None) => diagnostics.push(diagnostic(
            "missing-try-region",
            "try-region instruction must carry a structured region payload",
            Some(&function.name),
            Some(&block.label),
        )),
        ("try-region", Some(region)) => {
            validate_try_region(function, block, instruction, region, abi_minor, diagnostics)
        }
        (_, Some(_)) => diagnostics.push(diagnostic(
            "unexpected-try-region",
            "only try-region instructions may carry a structured region payload",
            Some(&function.name),
            Some(&block.label),
        )),
        (_, None) => {}
    }
}

pub(super) fn validate_try_region(
    function: &IrFunction,
    block: &IrBlock,
    instruction: &IrInstruction,
    region: &IrTryRegion,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    let mut labels = HashSet::new();
    validate_region(
        function,
        block,
        &region.protected,
        abi_minor,
        &mut labels,
        diagnostics,
    );
    validate_region(
        function,
        block,
        &region.success,
        abi_minor,
        &mut labels,
        diagnostics,
    );
    validate_region(
        function,
        block,
        &region.failure,
        abi_minor,
        &mut labels,
        diagnostics,
    );
    for cleanup in &region.cleanup {
        validate_region(
            function,
            block,
            cleanup,
            abi_minor,
            &mut labels,
            diagnostics,
        );
    }

    if !region.protected.parameters.is_empty() {
        diagnostics.push(diagnostic(
            "invalid-try-protected-parameters",
            "protected try region must not declare parameters",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region.success.parameters.len() != 1
        || region.success.parameters[0].type_name != region.protected.type_name
    {
        diagnostics.push(diagnostic(
            "invalid-try-success-parameters",
            "success region must accept exactly the protected result type",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region.failure.parameters.len() != 1 || region.failure.parameters[0].type_name != "Int" {
        diagnostics.push(diagnostic(
            "invalid-try-failure-parameters",
            "failure region must accept exactly one Int status parameter",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region
        .cleanup
        .iter()
        .any(|cleanup| !cleanup.parameters.is_empty() || cleanup.type_name != "Unit")
    {
        diagnostics.push(diagnostic(
            "invalid-try-cleanup-contract",
            "cleanup regions must have no parameters and Unit result type",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region.success.type_name != instruction.type_name
        || region.failure.type_name != instruction.type_name
    {
        diagnostics.push(diagnostic(
            "try-region-result-type-mismatch",
            "success and failure region types must equal the try instruction type",
            Some(&function.name),
            Some(&block.label),
        ));
    }
}

pub(super) fn validate_region<'a>(
    function: &IrFunction,
    parent: &IrBlock,
    region: &'a IrRegion,
    abi_minor: u32,
    region_labels: &mut HashSet<&'a str>,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if region.label.is_empty() {
        diagnostics.push(diagnostic(
            "invalid-try-region-label",
            "try region label must not be empty",
            Some(&function.name),
            Some(&parent.label),
        ));
    } else if !region_labels.insert(region.label.as_str()) {
        diagnostics.push(diagnostic(
            "duplicate-try-region-label",
            "try region labels must be unique inside one try instruction",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    if region.type_name.is_empty() {
        diagnostics.push(diagnostic(
            "missing-region-type",
            "try region result type must not be empty",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    if region.result.is_empty() {
        diagnostics.push(diagnostic(
            "missing-region-result",
            "try region result identity must not be empty",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    let Some(entry) = region.blocks.first() else {
        diagnostics.push(diagnostic(
            "missing-region-block",
            "try region must contain an entry block",
            Some(&function.name),
            Some(&parent.label),
        ));
        return;
    };
    if entry.label != "entry" {
        diagnostics.push(diagnostic(
            "invalid-region-entry",
            "first try region block must be named entry",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    let mut labels = HashSet::new();
    for nested in &region.blocks {
        if nested.label.is_empty() || !labels.insert(nested.label.as_str()) {
            diagnostics.push(diagnostic(
                "invalid-region-block-label",
                "try region block labels must be non-empty and unique",
                Some(&function.name),
                Some(&parent.label),
            ));
        }
    }
    for nested in &region.blocks {
        validate_terminator(function, nested, &labels, diagnostics);
        validate_instructions(function, nested, &labels, abi_minor, diagnostics);
    }
    let bound = region
        .parameters
        .iter()
        .any(|parameter| parameter.name == region.result)
        || region.blocks.iter().any(|nested| {
            nested
                .instructions
                .iter()
                .any(|instruction| instruction.result.as_deref() == Some(region.result.as_str()))
        });
    if !bound {
        diagnostics.push(diagnostic(
            "unbound-region-result",
            "try region result must name a parameter or instruction result",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
}
