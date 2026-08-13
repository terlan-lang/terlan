//! LLVM NVPTX implementation of the generic accelerator AOT backend.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use super::{
    artifact_kernels, sha256_bytes, AcceleratorAotArtifact, AcceleratorAotBackend,
    AcceleratorAotError, AcceleratorAotRequest, AcceleratorArtifactDescriptor,
    AcceleratorArtifactSource, ACCELERATOR_ARTIFACT_SCHEMA,
};
use crate::compiler::accelerator::{
    accelerator_toolchain_sha256, AcceleratorIrAccess, AcceleratorIrAddressSpace,
    AcceleratorIrBinaryOperation, AcceleratorIrComparison, AcceleratorIrKernel, AcceleratorIrNode,
    AcceleratorIrOperation, AcceleratorIrType, AcceleratorIrUnaryOperation, AcceleratorScalarType,
};

/// Maintained LLVM NVPTX backend invoked through an admitted `llc` executable.
#[derive(Debug, Default)]
pub struct LlvmNvptxBackend;

impl AcceleratorAotBackend for LlvmNvptxBackend {
    fn identity(&self) -> &'static str {
        "llvm-nvptx"
    }

    fn compile(
        &self,
        request: &AcceleratorAotRequest<'_>,
    ) -> Result<AcceleratorAotArtifact, AcceleratorAotError> {
        request
            .ir
            .verify()
            .map_err(|error| AcceleratorAotError::InvalidIr(error.to_string()))?;
        validate_request(request)?;
        fs::create_dir_all(request.output_directory).map_err(io_error)?;
        let ir_hash = request
            .ir
            .normalized_hash()
            .map_err(|error| AcceleratorAotError::InvalidIr(error.to_string()))?;
        let key = cache_key(request, &ir_hash);
        let base = sanitize(&request.ir.module);
        let artifact_name = format!("{base}-{key}.ptx");
        let descriptor_name = format!("{base}-{key}.accelerator.json");
        let artifact_path = request.output_directory.join(&artifact_name);
        let descriptor_path = request.output_directory.join(&descriptor_name);
        if artifact_path.is_file() && descriptor_path.is_file() {
            let bytes = fs::read(&artifact_path).map_err(io_error)?;
            validate_ptx(&bytes, request)?;
            let descriptor = decode_descriptor(&descriptor_path)?;
            if descriptor_matches(request, &descriptor, &bytes, &ir_hash) {
                return Ok(AcceleratorAotArtifact {
                    descriptor,
                    bytes,
                    descriptor_path,
                    artifact_path,
                    cache_hit: true,
                });
            }
        }

        let llvm = emit_module(request)?;
        let llvm_path = request.output_directory.join(format!("{base}-{key}.ll"));
        fs::write(&llvm_path, llvm.as_bytes()).map_err(io_error)?;
        invoke_llc(request, &llvm_path, &artifact_path)?;
        let bytes = fs::read(&artifact_path).map_err(io_error)?;
        validate_ptx(&bytes, request)?;
        let descriptor = AcceleratorArtifactDescriptor {
            schema: ACCELERATOR_ARTIFACT_SCHEMA.to_string(),
            backend: self.identity().to_string(),
            artifact_format: "ptx".to_string(),
            architecture: request.architecture.to_string(),
            ir_sha256: ir_hash,
            toolchain: request.toolchain.clone(),
            kernels: artifact_kernels(&request.ir.kernels),
            sources: request
                .ir
                .kernels
                .iter()
                .map(|kernel| AcceleratorArtifactSource {
                    entrypoint: kernel.name.clone(),
                    source: kernel.source.clone(),
                })
                .collect(),
            artifact: artifact_name,
            artifact_sha256: sha256_bytes(&bytes),
            build_options: request.build_options.clone(),
        };
        let encoded = serde_json::to_vec_pretty(&descriptor)
            .map_err(|error| AcceleratorAotError::Io(error.to_string()))?;
        fs::write(&descriptor_path, [encoded.as_slice(), b"\n"].concat()).map_err(io_error)?;
        Ok(AcceleratorAotArtifact {
            descriptor,
            bytes,
            descriptor_path,
            artifact_path,
            cache_hit: false,
        })
    }
}

/// Validates toolchain and architecture identities without ambient discovery.
fn validate_request(request: &AcceleratorAotRequest<'_>) -> Result<(), AcceleratorAotError> {
    if request.toolchain.name != "llvm-nvptx"
        || request.toolchain.license != "Apache-2.0 WITH LLVM-exception"
        || !request.toolchain.executable.starts_with('/')
    {
        return Err(AcceleratorAotError::Toolchain(
            "LLVM NVPTX requires an absolute admitted LLVM toolchain".to_string(),
        ));
    }
    let Some(compute) = request.architecture.strip_prefix("sm-") else {
        return Err(AcceleratorAotError::Toolchain(format!(
            "unsupported architecture `{}`",
            request.architecture
        )));
    };
    if compute.len() < 2 || !compute.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AcceleratorAotError::Toolchain(format!(
            "unsupported architecture `{}`",
            request.architecture
        )));
    }
    let actual_digest = accelerator_toolchain_sha256(Path::new(&request.toolchain.executable))
        .map_err(|error| AcceleratorAotError::Toolchain(error.to_string()))?;
    if actual_digest != request.toolchain.executable_sha256 {
        return Err(AcceleratorAotError::Toolchain(
            "admitted LLVM executable digest changed".to_string(),
        ));
    }
    let version = Command::new(&request.toolchain.executable)
        .arg("--version")
        .env_clear()
        .output()
        .map_err(|error| AcceleratorAotError::Toolchain(error.to_string()))?;
    let version_output = String::from_utf8_lossy(&version.stdout);
    if !version.status.success() || !version_output.contains(&request.toolchain.version) {
        return Err(AcceleratorAotError::Toolchain(
            "admitted LLVM version does not match the executable".to_string(),
        ));
    }
    for (name, value) in &request.build_options {
        if name != "optimization" || !matches!(value.as_str(), "0" | "1" | "2" | "3") {
            return Err(AcceleratorAotError::Toolchain(format!(
                "unsupported deterministic LLVM option `{name}={value}`"
            )));
        }
    }
    Ok(())
}

/// Validates every cache-owned descriptor field before reuse.
fn descriptor_matches(
    request: &AcceleratorAotRequest<'_>,
    descriptor: &AcceleratorArtifactDescriptor,
    bytes: &[u8],
    ir_hash: &str,
) -> bool {
    descriptor.backend == "llvm-nvptx"
        && descriptor.artifact_format == "ptx"
        && descriptor.architecture == request.architecture
        && descriptor.ir_sha256 == ir_hash
        && descriptor.toolchain == *request.toolchain
        && descriptor.kernels == artifact_kernels(&request.ir.kernels)
        && descriptor.artifact_sha256 == sha256_bytes(bytes)
        && descriptor.build_options == request.build_options
}

/// Computes the complete content-addressed backend cache key.
fn cache_key(request: &AcceleratorAotRequest<'_>, ir_hash: &str) -> String {
    let identity = serde_json::json!({
        "backend": "llvm-nvptx",
        "ir": ir_hash,
        "architecture": request.architecture,
        "toolchain": request.toolchain,
        "options": request.build_options,
    });
    sha256_bytes(identity.to_string().as_bytes())
}

/// Invokes the explicitly admitted maintained LLVM backend.
fn invoke_llc(
    request: &AcceleratorAotRequest<'_>,
    input: &Path,
    output: &Path,
) -> Result<(), AcceleratorAotError> {
    let mut command = Command::new(&request.toolchain.executable);
    command
        .arg("-march=nvptx64")
        .arg(format!("-mcpu={}", request.architecture.replace('-', "_")))
        .arg("-filetype=asm");
    if let Some(level) = request.build_options.get("optimization") {
        command.arg(format!("-O={level}"));
    }
    let execution = command
        .arg("-o")
        .arg(output)
        .arg(input)
        .env_clear()
        .output()
        .map_err(|error| AcceleratorAotError::ToolchainFailed(error.to_string()))?;
    if !execution.status.success() {
        return Err(AcceleratorAotError::ToolchainFailed(
            String::from_utf8_lossy(&execution.stderr)
                .trim()
                .to_string(),
        ));
    }
    Ok(())
}

/// Validates structural PTX metadata and every expected entrypoint.
fn validate_ptx(
    bytes: &[u8],
    request: &AcceleratorAotRequest<'_>,
) -> Result<(), AcceleratorAotError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AcceleratorAotError::InvalidArtifact(error.to_string()))?;
    for required in [".version", ".target", ".address_size 64"] {
        if !text.contains(required) {
            return Err(AcceleratorAotError::InvalidArtifact(format!(
                "PTX omits `{required}`"
            )));
        }
    }
    let expected_target = request.architecture.replace('-', "_");
    if !text.contains(&format!(".target {expected_target}")) {
        return Err(AcceleratorAotError::InvalidArtifact(format!(
            "PTX target does not match `{}`",
            request.architecture
        )));
    }
    for kernel in &request.ir.kernels {
        if !text.contains(&format!(".visible .entry {}(", kernel.name)) {
            return Err(AcceleratorAotError::InvalidArtifact(format!(
                "PTX omits entrypoint `{}`",
                kernel.name
            )));
        }
    }
    Ok(())
}

/// Decodes one cached descriptor with strict schema validation.
fn decode_descriptor(path: &Path) -> Result<AcceleratorArtifactDescriptor, AcceleratorAotError> {
    let bytes = fs::read(path).map_err(io_error)?;
    let descriptor: AcceleratorArtifactDescriptor = serde_json::from_slice(&bytes)
        .map_err(|error| AcceleratorAotError::InvalidArtifact(error.to_string()))?;
    if descriptor.schema != ACCELERATOR_ARTIFACT_SCHEMA {
        return Err(AcceleratorAotError::InvalidArtifact(
            "artifact descriptor schema mismatch".to_string(),
        ));
    }
    Ok(descriptor)
}

/// Converts filesystem failures to stable AOT I/O errors.
fn io_error(error: std::io::Error) -> AcceleratorAotError {
    AcceleratorAotError::Io(error.to_string())
}

/// Produces a filesystem-safe deterministic module component.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Emits one LLVM module from typed AcceleratorIR without source substitution.
fn emit_module(request: &AcceleratorAotRequest<'_>) -> Result<String, AcceleratorAotError> {
    let mut declarations = BTreeSet::new();
    let mut functions = Vec::new();
    for kernel in &request.ir.kernels {
        functions.push(FunctionEmitter::new(kernel).emit(&mut declarations)?);
    }
    let mut output = format!(
        "; generated from {}\ntarget triple = \"nvptx64-nvidia-cuda\"\n\n",
        request.ir.module
    );
    for declaration in declarations {
        output.push_str(&declaration);
        output.push('\n');
    }
    if !output.ends_with("\n\n") {
        output.push('\n');
    }
    for function in &functions {
        output.push_str(function);
        output.push('\n');
    }
    output.push_str("!nvvm.annotations = !{");
    for index in 0..request.ir.kernels.len() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&format!("!{index}"));
    }
    output.push_str("}\n");
    for (index, kernel) in request.ir.kernels.iter().enumerate() {
        let signature = function_type_signature(kernel)?;
        output.push_str(&format!(
            "!{index} = !{{void ({signature})* @{}, !\"kernel\", i32 1}}\n",
            kernel.name
        ));
    }
    Ok(output)
}

/// One lowered SSA value.
#[derive(Clone, Debug)]
struct LlvmValue {
    /// LLVM operand spelling.
    operand: String,
    /// AcceleratorIR source type.
    ty: AcceleratorIrType,
}

/// Stateful LLVM SSA emitter for one kernel.
struct FunctionEmitter<'a> {
    kernel: &'a AcceleratorIrKernel,
    next_value: u64,
    locals: BTreeMap<String, LlvmValue>,
    instructions: Vec<String>,
}

impl<'a> FunctionEmitter<'a> {
    /// Creates an emitter with parameter locals.
    fn new(kernel: &'a AcceleratorIrKernel) -> Self {
        let locals = kernel
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name.clone(),
                    LlvmValue {
                        operand: format!("%{}", parameter.name),
                        ty: parameter.ty.clone(),
                    },
                )
            })
            .collect();
        Self {
            kernel,
            next_value: 0,
            locals,
            instructions: Vec::new(),
        }
    }

    /// Emits the complete kernel entrypoint.
    fn emit(mut self, declarations: &mut BTreeSet<String>) -> Result<String, AcceleratorAotError> {
        let signature = function_signature(self.kernel)?;
        let result = self.emit_node(&self.kernel.body, declarations)?;
        if self.kernel.return_type != AcceleratorIrType::Unit {
            let llvm_type = llvm_scalar_type(&self.kernel.return_type)?;
            self.instructions.push(format!(
                "  store {llvm_type} {}, {llvm_type} addrspace(1)* %__terlan_result, align {}",
                result.operand,
                llvm_alignment(&self.kernel.return_type)?
            ));
        }
        let mut output = format!(
            "define void @{}({signature}) {{\nentry:\n",
            self.kernel.name
        );
        for instruction in self.instructions {
            output.push_str(&instruction);
            output.push('\n');
        }
        output.push_str("  ret void\n}\n");
        Ok(output)
    }

    /// Emits one typed expression into SSA instructions.
    fn emit_node(
        &mut self,
        node: &AcceleratorIrNode,
        declarations: &mut BTreeSet<String>,
    ) -> Result<LlvmValue, AcceleratorAotError> {
        match &node.operation {
            AcceleratorIrOperation::Int { value } => Ok(value_of(value.to_string(), &node.ty)),
            AcceleratorIrOperation::Float { value } => Ok(value_of(value.clone(), &node.ty)),
            AcceleratorIrOperation::Bool { value } => {
                Ok(value_of(if *value { "1" } else { "0" }, &node.ty))
            }
            AcceleratorIrOperation::Local { name } => {
                self.locals.get(name).cloned().ok_or_else(|| {
                    AcceleratorAotError::InvalidIr(format!("unknown local `{name}`"))
                })
            }
            AcceleratorIrOperation::Let { bindings, body } => {
                let original = self.locals.clone();
                for (name, value) in bindings {
                    let value = self.emit_node(value, declarations)?;
                    self.locals.insert(name.clone(), value);
                }
                let result = self.emit_node(body, declarations);
                self.locals = original;
                result
            }
            AcceleratorIrOperation::Unary { operation, operand } => {
                let operand = self.emit_node(operand, declarations)?;
                let ty = llvm_scalar_type(&operand.ty)?;
                let instruction = match operation {
                    AcceleratorIrUnaryOperation::Negate if is_float(&operand.ty) => {
                        format!("fsub {ty} -0.0, {}", operand.operand)
                    }
                    AcceleratorIrUnaryOperation::Negate => {
                        format!("sub {ty} 0, {}", operand.operand)
                    }
                    AcceleratorIrUnaryOperation::Not => {
                        format!("xor i1 {}, 1", operand.operand)
                    }
                };
                self.instruction(instruction, &node.ty)
            }
            AcceleratorIrOperation::Binary {
                operation,
                left,
                right,
            } => {
                let left = self.emit_node(left, declarations)?;
                let right = self.emit_node(right, declarations)?;
                let ty = llvm_scalar_type(&left.ty)?;
                let opcode = binary_opcode(*operation, &left.ty)?;
                self.instruction(
                    format!("{opcode} {ty} {}, {}", left.operand, right.operand),
                    &node.ty,
                )
            }
            AcceleratorIrOperation::Compare {
                comparison,
                left,
                right,
            } => {
                let left = self.emit_node(left, declarations)?;
                let right = self.emit_node(right, declarations)?;
                let ty = llvm_scalar_type(&left.ty)?;
                let instruction = if is_float(&left.ty) {
                    format!(
                        "fcmp {} {ty} {}, {}",
                        float_comparison(*comparison),
                        left.operand,
                        right.operand
                    )
                } else {
                    format!(
                        "icmp {} {ty} {}, {}",
                        integer_comparison(*comparison, &left.ty),
                        left.operand,
                        right.operand
                    )
                };
                self.instruction(instruction, &node.ty)
            }
            AcceleratorIrOperation::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.emit_node(condition, declarations)?;
                let then_label = self.next_label("if_then");
                let else_label = self.next_label("if_else");
                let merge_label = self.next_label("if_merge");
                self.instructions.push(format!(
                    "  br i1 {}, label %{then_label}, label %{else_label}",
                    condition.operand
                ));
                self.instructions.push(format!("{then_label}:"));
                let then_value = self.emit_node(then_value, declarations)?;
                self.instructions.push(format!("  br label %{merge_label}"));
                self.instructions.push(format!("{else_label}:"));
                let else_value = self.emit_node(else_value, declarations)?;
                self.instructions.push(format!("  br label %{merge_label}"));
                self.instructions.push(format!("{merge_label}:"));
                let ty = llvm_scalar_type(&node.ty)?;
                self.instruction(
                    format!(
                        "phi {ty} [{}, %{then_label}], [{}, %{else_label}]",
                        then_value.operand, else_value.operand
                    ),
                    &node.ty,
                )
            }
            AcceleratorIrOperation::Load { buffer, index } => {
                let buffer_value = self.local_buffer(buffer)?;
                let address_space = buffer_address_space(&buffer_value.ty)?;
                let index = self.emit_node(index, declarations)?;
                let element = llvm_scalar_type(&node.ty)?;
                let pointer = self.next_name();
                self.instructions.push(format!(
                    "  {pointer} = getelementptr inbounds {element}, {element} addrspace({address_space})* {}, i64 {}",
                    buffer_value.operand, index.operand
                ));
                let alignment = llvm_alignment(&node.ty)?;
                self.instruction(
                    format!("load {element}, {element} addrspace({address_space})* {pointer}, align {alignment}"),
                    &node.ty,
                )
            }
            AcceleratorIrOperation::Store {
                buffer,
                index,
                value,
            } => {
                let buffer_value = self.local_buffer(buffer)?;
                let address_space = buffer_address_space(&buffer_value.ty)?;
                let index = self.emit_node(index, declarations)?;
                let value = self.emit_node(value, declarations)?;
                let element = llvm_scalar_type(&value.ty)?;
                let pointer = self.next_name();
                self.instructions.push(format!(
                    "  {pointer} = getelementptr inbounds {element}, {element} addrspace({address_space})* {}, i64 {}",
                    buffer_value.operand, index.operand
                ));
                self.instructions.push(format!(
                    "  store {element} {}, {element} addrspace({address_space})* {pointer}, align {}",
                    value.operand,
                    llvm_alignment(&value.ty)?
                ));
                Ok(value_of("0", &AcceleratorIrType::Unit))
            }
            AcceleratorIrOperation::StaticLoop {
                index_name,
                start,
                end,
                accumulator_name,
                initial,
                body,
            } => {
                let count = end.saturating_sub(*start);
                if count > 4096 {
                    return Err(AcceleratorAotError::Unsupported(
                        "LLVM first-subset static loop exceeds unroll limit".to_string(),
                    ));
                }
                let original = self.locals.clone();
                let mut accumulator = self.emit_node(initial, declarations)?;
                for index in *start..*end {
                    self.locals.insert(
                        index_name.clone(),
                        value_of(index.to_string(), &scalar_i64()),
                    );
                    self.locals
                        .insert(accumulator_name.clone(), accumulator.clone());
                    accumulator = self.emit_node(body, declarations)?;
                }
                self.locals = original;
                Ok(accumulator)
            }
            AcceleratorIrOperation::Math {
                operation,
                arguments,
            } => self.emit_math(operation, arguments, &node.ty, declarations),
        }
    }

    /// Emits one maintained LLVM scalar intrinsic call.
    fn emit_math(
        &mut self,
        operation: &str,
        arguments: &[AcceleratorIrNode],
        result_type: &AcceleratorIrType,
        declarations: &mut BTreeSet<String>,
    ) -> Result<LlvmValue, AcceleratorAotError> {
        let values = arguments
            .iter()
            .map(|argument| self.emit_node(argument, declarations))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = llvm_scalar_type(result_type)?;
        let suffix = match ty.as_str() {
            "float" => "f32",
            "double" => "f64",
            _ => {
                return Err(AcceleratorAotError::Unsupported(
                    "math operation requires f32 or f64".to_string(),
                ))
            }
        };
        let intrinsic = match operation.rsplit('.').next() {
            Some("sqrt") => "sqrt",
            Some("abs") => "fabs",
            Some("min") => "minnum",
            Some("max") => "maxnum",
            _ => return Err(AcceleratorAotError::Unsupported(operation.to_string())),
        };
        let symbol = format!("llvm.{intrinsic}.{suffix}");
        let parameters = std::iter::repeat_n(ty.clone(), values.len())
            .collect::<Vec<_>>()
            .join(", ");
        declarations.insert(format!("declare {ty} @{symbol}({parameters})"));
        let arguments = values
            .iter()
            .map(|value| format!("{ty} {}", value.operand))
            .collect::<Vec<_>>()
            .join(", ");
        self.instruction(format!("call {ty} @{symbol}({arguments})"), result_type)
    }

    /// Emits one named SSA instruction.
    fn instruction(
        &mut self,
        instruction: String,
        ty: &AcceleratorIrType,
    ) -> Result<LlvmValue, AcceleratorAotError> {
        let name = self.next_name();
        self.instructions.push(format!("  {name} = {instruction}"));
        Ok(value_of(name, ty))
    }

    /// Allocates one deterministic SSA identity.
    fn next_name(&mut self) -> String {
        let value = format!("%v{}", self.next_value);
        self.next_value += 1;
        value
    }

    /// Allocates one deterministic LLVM basic-block identity.
    fn next_label(&mut self, prefix: &str) -> String {
        let value = format!("{prefix}_{}", self.next_value);
        self.next_value += 1;
        value
    }

    /// Returns a declared buffer parameter.
    fn local_buffer(&self, name: &str) -> Result<LlvmValue, AcceleratorAotError> {
        self.locals
            .get(name)
            .filter(|value| matches!(value.ty, AcceleratorIrType::Buffer { .. }))
            .cloned()
            .ok_or_else(|| AcceleratorAotError::InvalidIr(format!("unknown buffer `{name}`")))
    }
}

/// Returns the LLVM parameter signature, including a hidden scalar-result pointer.
fn function_signature(kernel: &AcceleratorIrKernel) -> Result<String, AcceleratorAotError> {
    let mut parameters = kernel
        .parameters
        .iter()
        .map(llvm_parameter_declaration)
        .collect::<Result<Vec<_>, _>>()?;
    if kernel.return_type != AcceleratorIrType::Unit {
        parameters.push(format!(
            "{} addrspace(1)* %__terlan_result",
            llvm_scalar_type(&kernel.return_type)?
        ));
    }
    Ok(parameters.join(", "))
}

/// Returns one named kernel parameter with access and alignment attributes.
fn llvm_parameter_declaration(
    parameter: &crate::compiler::accelerator::AcceleratorIrParameter,
) -> Result<String, AcceleratorAotError> {
    let ty = llvm_parameter_type(&parameter.ty)?;
    let attributes = match &parameter.ty {
        AcceleratorIrType::Buffer {
            access, alignment, ..
        } => {
            let access = match access {
                AcceleratorIrAccess::Read => " readonly",
                AcceleratorIrAccess::Write => " writeonly",
                AcceleratorIrAccess::ReadWrite => "",
            };
            format!(" align {alignment}{access}")
        }
        _ => String::new(),
    };
    Ok(format!("{ty}{attributes} %{}", parameter.name))
}

/// Returns the unnamed function type required by LLVM metadata references.
fn function_type_signature(kernel: &AcceleratorIrKernel) -> Result<String, AcceleratorAotError> {
    let mut parameters = kernel
        .parameters
        .iter()
        .map(|parameter| llvm_parameter_type(&parameter.ty))
        .collect::<Result<Vec<_>, _>>()?;
    if kernel.return_type != AcceleratorIrType::Unit {
        parameters.push(format!(
            "{} addrspace(1)*",
            llvm_scalar_type(&kernel.return_type)?
        ));
    }
    Ok(parameters.join(", "))
}

/// Returns the LLVM type for one kernel parameter.
fn llvm_parameter_type(ty: &AcceleratorIrType) -> Result<String, AcceleratorAotError> {
    match ty {
        AcceleratorIrType::Buffer {
            dtype,
            address_space,
            ..
        } => Ok(format!(
            "{} addrspace({})*",
            llvm_dtype(*dtype)?,
            llvm_address_space(*address_space)
        )),
        _ => llvm_scalar_type(ty),
    }
}

/// Returns the LLVM NVPTX numeric address-space identity.
fn llvm_address_space(address_space: AcceleratorIrAddressSpace) -> u32 {
    match address_space {
        AcceleratorIrAddressSpace::Device => 1,
        AcceleratorIrAddressSpace::Shared => 3,
        AcceleratorIrAddressSpace::Constant => 4,
        AcceleratorIrAddressSpace::Local => 5,
    }
}

/// Returns the numeric address space from one buffer type.
fn buffer_address_space(ty: &AcceleratorIrType) -> Result<u32, AcceleratorAotError> {
    match ty {
        AcceleratorIrType::Buffer { address_space, .. } => Ok(llvm_address_space(*address_space)),
        _ => Err(AcceleratorAotError::InvalidIr(
            "buffer operation used a scalar local".to_string(),
        )),
    }
}

/// Returns the LLVM scalar type for one IR value.
fn llvm_scalar_type(ty: &AcceleratorIrType) -> Result<String, AcceleratorAotError> {
    match ty {
        AcceleratorIrType::Scalar { dtype } => llvm_dtype(*dtype),
        AcceleratorIrType::Bool | AcceleratorIrType::Unit => Ok("i1".to_string()),
        AcceleratorIrType::Buffer { .. } => Err(AcceleratorAotError::Unsupported(
            "buffer used as scalar".to_string(),
        )),
    }
}

/// Maps canonical scalar dtypes to LLVM NVPTX types.
fn llvm_dtype(dtype: AcceleratorScalarType) -> Result<String, AcceleratorAotError> {
    Ok(match dtype {
        AcceleratorScalarType::Bool => "i1",
        AcceleratorScalarType::U8 | AcceleratorScalarType::I8 => "i8",
        AcceleratorScalarType::U16 | AcceleratorScalarType::I16 => "i16",
        AcceleratorScalarType::U32 | AcceleratorScalarType::I32 => "i32",
        AcceleratorScalarType::U64 | AcceleratorScalarType::I64 => "i64",
        AcceleratorScalarType::F32 => "float",
        AcceleratorScalarType::F64 => "double",
        AcceleratorScalarType::F16 | AcceleratorScalarType::Bf16 => {
            return Err(AcceleratorAotError::Unsupported(format!(
                "LLVM NVPTX scalar `{}`",
                dtype.identifier()
            )))
        }
    }
    .to_string())
}

/// Returns natural scalar alignment.
fn llvm_alignment(ty: &AcceleratorIrType) -> Result<u64, AcceleratorAotError> {
    match ty {
        AcceleratorIrType::Scalar { dtype } => Ok(dtype.alignment()),
        AcceleratorIrType::Bool | AcceleratorIrType::Unit => Ok(1),
        AcceleratorIrType::Buffer { alignment, .. } => Ok(*alignment),
    }
}

/// Selects an LLVM arithmetic opcode from exact scalar type semantics.
fn binary_opcode(
    operation: AcceleratorIrBinaryOperation,
    ty: &AcceleratorIrType,
) -> Result<&'static str, AcceleratorAotError> {
    use AcceleratorIrBinaryOperation as Operation;
    if is_float(ty) {
        return Ok(match operation {
            Operation::Add => "fadd",
            Operation::Subtract => "fsub",
            Operation::Multiply => "fmul",
            Operation::Divide => "fdiv",
            Operation::Remainder => "frem",
            Operation::And | Operation::Or => {
                return Err(AcceleratorAotError::Unsupported(
                    "floating Boolean operation".to_string(),
                ))
            }
        });
    }
    Ok(match operation {
        Operation::Add => "add",
        Operation::Subtract => "sub",
        Operation::Multiply => "mul",
        Operation::Divide if is_unsigned(ty) => "udiv",
        Operation::Divide => "sdiv",
        Operation::Remainder if is_unsigned(ty) => "urem",
        Operation::Remainder => "srem",
        Operation::And => "and",
        Operation::Or => "or",
    })
}

/// Returns an ordered floating-point comparison predicate.
fn float_comparison(comparison: AcceleratorIrComparison) -> &'static str {
    match comparison {
        AcceleratorIrComparison::Equal => "oeq",
        AcceleratorIrComparison::NotEqual => "one",
        AcceleratorIrComparison::Less => "olt",
        AcceleratorIrComparison::LessEqual => "ole",
        AcceleratorIrComparison::Greater => "ogt",
        AcceleratorIrComparison::GreaterEqual => "oge",
    }
}

/// Returns the signed or unsigned integer comparison predicate.
fn integer_comparison(comparison: AcceleratorIrComparison, ty: &AcceleratorIrType) -> &'static str {
    match comparison {
        AcceleratorIrComparison::Equal => "eq",
        AcceleratorIrComparison::NotEqual => "ne",
        AcceleratorIrComparison::Less if is_unsigned(ty) => "ult",
        AcceleratorIrComparison::Less => "slt",
        AcceleratorIrComparison::LessEqual if is_unsigned(ty) => "ule",
        AcceleratorIrComparison::LessEqual => "sle",
        AcceleratorIrComparison::Greater if is_unsigned(ty) => "ugt",
        AcceleratorIrComparison::Greater => "sgt",
        AcceleratorIrComparison::GreaterEqual if is_unsigned(ty) => "uge",
        AcceleratorIrComparison::GreaterEqual => "sge",
    }
}

/// Returns whether a scalar type is floating point.
fn is_float(ty: &AcceleratorIrType) -> bool {
    matches!(ty, AcceleratorIrType::Scalar { dtype } if dtype.is_float())
}

/// Returns whether integer arithmetic is unsigned.
fn is_unsigned(ty: &AcceleratorIrType) -> bool {
    matches!(
        ty,
        AcceleratorIrType::Scalar {
            dtype: AcceleratorScalarType::U8
                | AcceleratorScalarType::U16
                | AcceleratorScalarType::U32
                | AcceleratorScalarType::U64
        }
    )
}

/// Constructs one SSA or literal value.
fn value_of(operand: impl Into<String>, ty: &AcceleratorIrType) -> LlvmValue {
    LlvmValue {
        operand: operand.into(),
        ty: ty.clone(),
    }
}

/// Returns the canonical static-loop index type.
fn scalar_i64() -> AcceleratorIrType {
    AcceleratorIrType::Scalar {
        dtype: AcceleratorScalarType::I64,
    }
}
