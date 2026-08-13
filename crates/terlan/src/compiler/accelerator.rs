//! Backend-neutral accelerator package metadata.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::ops::Deref;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

#[path = "accelerator/aot.rs"]
mod aot;
#[path = "accelerator/assembly.rs"]
mod assembly;
#[path = "accelerator/ir.rs"]
mod ir;
#[path = "accelerator/placement.rs"]
mod placement;
#[path = "accelerator/provenance.rs"]
mod provenance;
#[path = "accelerator/semantics.rs"]
mod semantics;
#[path = "accelerator/target.rs"]
mod target;
#[path = "accelerator/value.rs"]
mod value;

pub use aot::*;
pub use assembly::*;
pub use ir::*;
pub use placement::*;
pub use provenance::{AcceleratorDescriptorSpans, AcceleratorSourceSpan};
pub use semantics::*;
pub use target::*;
pub use value::*;

/// Current accelerator descriptor schema version.
pub const ACCELERATOR_DESCRIPTOR_SCHEMA: u64 = 1;

/// Typed failure produced while validating or specializing accelerator metadata.
pub struct AcceleratorError(terlan_runtime_abi::BoundaryError);

impl AcceleratorError {
    /// Creates an accelerator compiler failure from a stable rendered diagnostic.
    pub fn message(operation: &'static str, rendered: impl Into<String>) -> Self {
        Self(terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::CompilerPhase,
            operation,
            rendered,
        ))
    }

    /// Creates an accelerator compiler failure while preserving its concrete source.
    pub fn sourced<E>(
        code: impl Into<String>,
        operation: &'static str,
        context: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(terlan_runtime_abi::BoundaryError::sourced(
            terlan_runtime_abi::ErrorDomain::CompilerPhase,
            code,
            operation,
            context,
            source,
        ))
    }

    /// Returns the stable machine-readable diagnostic code.
    pub fn code(&self) -> &str {
        self.0.code()
    }

    /// Returns the compiler operation that failed.
    pub const fn operation(&self) -> &'static str {
        self.0.operation()
    }
}

impl fmt::Debug for AcceleratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Display for AcceleratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for AcceleratorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl Deref for AcceleratorError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.context()
    }
}

impl PartialEq<&str> for AcceleratorError {
    fn eq(&self, other: &&str) -> bool {
        self.0.context() == *other
    }
}

impl PartialEq<String> for AcceleratorError {
    fn eq(&self, other: &String) -> bool {
        self.0.context() == other
    }
}

impl From<String> for AcceleratorError {
    fn from(rendered: String) -> Self {
        Self::message("validate accelerator contract", rendered)
    }
}

impl From<&str> for AcceleratorError {
    fn from(rendered: &str) -> Self {
        rendered.to_owned().into()
    }
}

impl From<AcceleratorError> for String {
    fn from(error: AcceleratorError) -> Self {
        error.to_string()
    }
}

/// Result returned by typed accelerator compiler operations.
pub type AcceleratorResult<T> = Result<T, AcceleratorError>;

/// Normalized accelerator capability descriptor loaded without native code.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorDescriptor {
    /// Non-serialized source locations retained for stable diagnostics.
    #[serde(skip)]
    pub diagnostic_spans: AcceleratorDescriptorSpans,
    /// Descriptor schema version.
    pub schema: u64,
    /// Stable backend identifier owned by the package.
    pub backend: String,
    /// Device classes accepted by this package.
    pub device_classes: Vec<String>,
    /// Host targets and their package availability state.
    pub targets: Vec<AcceleratorTarget>,
    /// Artifact formats loadable by the package.
    pub artifact_formats: Vec<String>,
    /// Scalar dtypes admitted at package boundaries.
    pub dtypes: Vec<String>,
    /// Memory layouts admitted by package operations.
    pub layouts: Vec<String>,
    /// Logical address spaces used without exposing pointers.
    pub address_spaces: Vec<String>,
    /// Opaque resource classes owned by the package.
    pub resource_classes: Vec<String>,
    /// Named asynchronous operation classes.
    pub asynchronous_operations: Vec<String>,
    /// Capabilities that a target must explicitly admit.
    pub capabilities: Vec<String>,
    /// Capabilities that must be provided by another package in the closure.
    #[serde(default)]
    pub requirements: Vec<String>,
    /// Native host libraries required by package features.
    #[serde(default)]
    pub host_libraries: Vec<AcceleratorHostLibrary>,
    /// Maintained toolchains used to produce accelerator artifacts.
    #[serde(default)]
    pub toolchains: Vec<AcceleratorToolchain>,
    /// Public package operations and their effects.
    #[serde(default)]
    pub operations: Vec<AcceleratorOperation>,
    /// Package or compiler-produced kernel entrypoint contracts.
    #[serde(default)]
    pub kernels: Vec<AcceleratorKernel>,
}

/// Native library identity required by an accelerator package.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorHostLibrary {
    /// Stable library identifier.
    pub name: String,
    /// Exact version or ABI identity required by the package.
    pub version: String,
    /// Whether the base package requires the library rather than an optional feature.
    pub required: bool,
}

/// Package availability and artifact support for one host target.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorTarget {
    /// Stable Rust-style target triple.
    pub triple: String,
    /// Support state published by the package.
    pub availability: AcceleratorAvailability,
    /// Accelerator artifacts usable on this host target.
    pub artifact_formats: Vec<String>,
    /// Accelerator architectures admitted by this target, or any when empty.
    #[serde(default)]
    pub architectures: Vec<String>,
    /// Driver API contract required for runtime execution.
    #[serde(default)]
    pub driver_api: Option<String>,
    /// Static memory requirements checked by target admission.
    #[serde(default)]
    pub memory: AcceleratorMemoryRequirements,
    /// Numerical determinism required by this target.
    #[serde(default)]
    pub determinism: AcceleratorDeterminism,
}

/// Static accelerator memory requirements for one package target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorMemoryRequirements {
    /// Minimum device-local bytes required by the package target.
    #[serde(default)]
    pub minimum_device_bytes: u64,
    /// Minimum pinned-host bytes required by the package target.
    #[serde(default)]
    pub minimum_pinned_host_bytes: u64,
    /// Whether the target requires unified host/device memory.
    #[serde(default)]
    pub unified_memory: bool,
}

/// Numerical determinism contract required by an accelerator target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorDeterminism {
    /// Operations must satisfy deterministic execution policy.
    Strict,
    /// Operations may select deterministic implementations when available.
    #[default]
    BestEffort,
    /// The target explicitly admits nondeterministic accelerator behavior.
    Nondeterministic,
}

/// Published support level for an accelerator host target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorAvailability {
    /// Required and release-gated package target.
    Supported,
    /// Implemented target that is not yet a release gate.
    Experimental,
    /// Explicitly unavailable target with typed admission failure.
    Unsupported,
}

/// Maintained external toolchain identity used for AOT artifact generation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorToolchain {
    /// Stable toolchain identifier.
    pub name: String,
    /// Exact supported toolchain version.
    pub version: String,
    /// Artifact formats emitted by this toolchain.
    pub artifact_formats: Vec<String>,
    /// Whether package installation requires this toolchain.
    pub required: bool,
}

/// Public accelerator operation and its static effect contract.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorOperation {
    /// Stable package operation identifier.
    pub id: String,
    /// Effects performed by the operation.
    pub effects: Vec<AcceleratorEffect>,
    /// Whether completion can occur after the native request returns control.
    pub asynchronous: bool,
}

/// Static effects used by accelerator admission and placement.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorEffect {
    /// Allocates an opaque package resource.
    Allocate,
    /// Transfers values between logical address spaces.
    Transfer,
    /// Executes work on an accelerator device.
    Execute,
    /// Synchronizes host, stream, event, or device work.
    Synchronize,
    /// Schedules a bounded host callback.
    HostCallback,
    /// Consumes or advances explicit random state.
    Random,
    /// May produce nondeterministic results under its declared policy.
    Nondeterministic,
    /// May block the package worker and must not run on a scheduler worker.
    Blocking,
}

/// Versioned kernel entrypoint descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorKernel {
    /// Stable package kernel identifier.
    pub id: String,
    /// Artifact format containing the entrypoint.
    pub artifact_format: String,
    /// Package-relative artifact path.
    pub artifact: String,
    /// Native entrypoint symbol.
    pub symbol: String,
    /// Target architectures accepted by the artifact.
    pub target_architectures: Vec<String>,
    /// Ordered typed parameters.
    pub parameters: Vec<AcceleratorKernelParameter>,
    /// Maximum dynamic shared-memory bytes admitted by the descriptor.
    pub max_shared_memory_bytes: u64,
}

/// One typed kernel parameter without a raw address.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorKernelParameter {
    /// Stable parameter name.
    pub name: String,
    /// Scalar dtype or `scalar` marker declared by the package.
    pub dtype: String,
    /// Logical address space containing the parameter.
    pub address_space: String,
    /// Read/write contract used for alias analysis.
    pub access: AcceleratorAccess,
}

/// Kernel parameter access contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorAccess {
    /// Parameter is read-only for the duration of the launch.
    Read,
    /// Parameter is written but its prior value is not read.
    Write,
    /// Parameter may be both read and written.
    ReadWrite,
    /// Parameter is passed by value rather than through an address space.
    Value,
}

/// One package-owned accelerator descriptor in a resolved package closure.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPackageDescriptor {
    /// Stable Terlan package name.
    pub package: String,
    /// Package version selected by ordinary package resolution.
    pub version: String,
    /// Manifest-relative descriptor provenance used in diagnostics.
    pub source: String,
    /// Validated backend-neutral accelerator contract.
    pub descriptor: AcceleratorDescriptor,
}

/// Validated accelerator contracts and capability owners for a package closure.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorDependencyClosure {
    /// Deterministically ordered package descriptors.
    pub packages: Vec<AcceleratorPackageDescriptor>,
    /// Unique package owner for every accelerator capability.
    pub capability_owners: BTreeMap<String, String>,
}

impl AcceleratorDescriptor {
    /// Parses and validates one descriptor without loading package-native code.
    pub fn parse(source: &str, provenance: &Path) -> AcceleratorResult<Self> {
        let mut descriptor: Self = basic_toml::from_str(source).map_err(|error| {
            format!(
                "error[accelerator.descriptor-parse]: {}: invalid accelerator descriptor: {error}",
                AcceleratorSourceSpan::start(provenance)
            )
        })?;
        descriptor.diagnostic_spans = AcceleratorDescriptorSpans::scan(source, provenance);
        descriptor.validate(provenance)?;
        Ok(descriptor)
    }

    /// Reads a package-relative descriptor and rejects path escape before access.
    pub fn read(package_root: &Path, relative: &Path) -> AcceleratorResult<Self> {
        validate_relative_path(relative, package_root)?;
        let path = package_root.join(relative);
        let source = std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "cannot read accelerator descriptor {}: {error}",
                path.display()
            )
        })?;
        Self::parse(&source, &path)
    }

    /// Validates normalized identities, references, and kernel contracts.
    pub fn validate(&self, provenance: &Path) -> AcceleratorResult<()> {
        self.validate_contract(provenance).map_err(|error| {
            self.diagnostic_spans
                .decorate("descriptor-invalid", &error, provenance)
                .into()
        })
    }

    /// Validates the descriptor independently of diagnostic presentation.
    fn validate_contract(&self, provenance: &Path) -> AcceleratorResult<()> {
        if self.schema != ACCELERATOR_DESCRIPTOR_SCHEMA {
            return Err(format!(
                "{}: unsupported accelerator descriptor schema `{}`; supported schemas: {}",
                provenance.display(),
                self.schema,
                ACCELERATOR_DESCRIPTOR_SCHEMA
            )
            .into());
        }
        validate_identifier(&self.backend, "backend", provenance)?;
        validate_unique(&self.device_classes, "device_classes", provenance)?;
        validate_named(&self.targets, |value| &value.triple, "target", provenance)?;
        if self.targets.is_empty() {
            return Err(format!(
                "{}: accelerator descriptor must declare at least one target",
                provenance.display()
            )
            .into());
        }
        validate_unique(&self.artifact_formats, "artifact_formats", provenance)?;
        validate_unique(&self.dtypes, "dtypes", provenance)?;
        for dtype in &self.dtypes {
            AcceleratorScalarType::try_from(dtype.as_str())
                .map_err(|error| format!("{}: {error}", provenance.display()))?;
        }
        validate_unique(&self.layouts, "layouts", provenance)?;
        validate_unique(&self.address_spaces, "address_spaces", provenance)?;
        validate_unique(&self.resource_classes, "resource_classes", provenance)?;
        validate_unique(
            &self.asynchronous_operations,
            "asynchronous_operations",
            provenance,
        )?;
        validate_unique_allow_empty(&self.capabilities, "capabilities", provenance)?;
        validate_unique_allow_empty(&self.requirements, "requirements", provenance)?;
        validate_named(
            &self.host_libraries,
            |value| &value.name,
            "host library",
            provenance,
        )?;
        validate_named(
            &self.toolchains,
            |value| &value.name,
            "toolchain",
            provenance,
        )?;
        validate_named(&self.operations, |value| &value.id, "operation", provenance)?;
        validate_named(&self.kernels, |value| &value.id, "kernel", provenance)?;
        if self.operations.is_empty() {
            return Err(format!(
                "{}: accelerator descriptor must declare at least one operation",
                provenance.display()
            )
            .into());
        }

        for library in &self.host_libraries {
            require_nonempty(&library.version, "host library version", provenance)?;
        }
        for target in &self.targets {
            validate_unique_allow_empty(&target.architectures, "target architectures", provenance)?;
            if let Some(driver_api) = &target.driver_api {
                require_nonempty(driver_api, "target driver API", provenance)?;
            }
            validate_unique(
                &target.artifact_formats,
                "target artifact_formats",
                provenance,
            )?;
            ensure_subset(
                &target.artifact_formats,
                &self.artifact_formats,
                "target artifact format",
                provenance,
            )?;
        }
        for toolchain in &self.toolchains {
            require_nonempty(&toolchain.version, "toolchain version", provenance)?;
            validate_unique(
                &toolchain.artifact_formats,
                "toolchain artifact_formats",
                provenance,
            )?;
            ensure_subset(
                &toolchain.artifact_formats,
                &self.artifact_formats,
                "toolchain artifact format",
                provenance,
            )?;
        }
        for operation in &self.operations {
            if operation.effects.is_empty() {
                return Err(format!(
                    "{}: accelerator operation `{}` must declare at least one effect",
                    provenance.display(),
                    operation.id
                )
                .into());
            }
            let unique = operation.effects.iter().copied().collect::<BTreeSet<_>>();
            if unique.len() != operation.effects.len() {
                return Err(format!(
                    "{}: accelerator operation `{}` contains duplicate effects",
                    provenance.display(),
                    operation.id
                )
                .into());
            }
        }
        for kernel in &self.kernels {
            validate_kernel(kernel, self, provenance)?;
        }
        Ok(())
    }
}

impl AcceleratorDependencyClosure {
    /// Resolves capability ownership and validates requirements across packages.
    pub fn resolve(
        mut packages: Vec<AcceleratorPackageDescriptor>,
        provenance: &Path,
    ) -> AcceleratorResult<Self> {
        packages.sort_by(|left, right| {
            (&left.package, &left.version, &left.source).cmp(&(
                &right.package,
                &right.version,
                &right.source,
            ))
        });
        let mut package_names = BTreeSet::new();
        let mut capability_owners = BTreeMap::new();
        for package in &packages {
            validate_identifier(&package.package, "package", provenance)?;
            require_nonempty(&package.version, "package version", provenance)?;
            require_nonempty(&package.source, "descriptor source", provenance)?;
            package.descriptor.validate(Path::new(&package.source))?;
            if !package_names.insert(package.package.clone()) {
                return Err(package
                    .descriptor
                    .diagnostic_spans
                    .diagnostic(
                        "duplicate-package",
                        package.descriptor.diagnostic_spans.descriptor(),
                        format!("duplicate accelerator package owner `{}`", package.package),
                        provenance,
                    )
                    .into());
            }
            for capability in &package.descriptor.capabilities {
                if let Some(owner) =
                    capability_owners.insert(capability.clone(), package.package.clone())
                {
                    return Err(package.descriptor.diagnostic_spans.diagnostic(
                        "duplicate-capability",
                        package
                            .descriptor
                            .diagnostic_spans
                            .capability(capability),
                        format!(
                            "accelerator capability `{capability}` has duplicate owners `{owner}` and `{}`",
                            package.package
                        ),
                        provenance,
                    ).into());
                }
            }
        }

        let package_by_name = packages
            .iter()
            .map(|package| (package.package.as_str(), package))
            .collect::<BTreeMap<_, _>>();
        let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
        for package in &packages {
            for requirement in &package.descriptor.requirements {
                let Some(owner) = capability_owners.get(requirement) else {
                    return Err(package
                        .descriptor
                        .diagnostic_spans
                        .diagnostic(
                            "unowned-capability",
                            package.descriptor.diagnostic_spans.requirement(requirement),
                            format!(
                            "accelerator package `{}` requires unowned capability `{requirement}`",
                            package.package
                        ),
                            provenance,
                        )
                        .into());
                };
                let provider = package_by_name
                    .get(owner.as_str())
                    .expect("capability owner must be a resolved package");
                validate_target_overlap(package, provider, requirement, provenance)?;
                dependencies
                    .entry(package.package.clone())
                    .or_default()
                    .insert(owner.clone());
            }
        }
        reject_capability_cycles(&packages, &dependencies, provenance)?;
        Ok(Self {
            packages,
            capability_owners,
        })
    }
}

fn validate_target_overlap(
    consumer: &AcceleratorPackageDescriptor,
    provider: &AcceleratorPackageDescriptor,
    capability: &str,
    provenance: &Path,
) -> AcceleratorResult<()> {
    let compatible = consumer.descriptor.targets.iter().any(|consumer_target| {
        consumer_target.availability != AcceleratorAvailability::Unsupported
            && provider.descriptor.targets.iter().any(|provider_target| {
                provider_target.triple == consumer_target.triple
                    && provider_target.availability != AcceleratorAvailability::Unsupported
            })
    });
    if !compatible {
        let consumer_targets = available_targets(consumer);
        let provider_targets = available_targets(provider);
        return Err(consumer.descriptor.diagnostic_spans.diagnostic(
            "target-mismatch",
            consumer
                .descriptor
                .diagnostic_spans
                .requirement(capability),
            format!(
                "accelerator package `{}` targets [{}] require capability `{capability}` from `{}` targets [{}], but they have no common available target",
                consumer.package,
                consumer_targets.join(", "),
                provider.package,
                provider_targets.join(", ")
            ),
            provenance,
        ).into());
    }
    Ok(())
}

/// Returns deterministically ordered available target triples for diagnostics.
fn available_targets(package: &AcceleratorPackageDescriptor) -> Vec<&str> {
    package
        .descriptor
        .targets
        .iter()
        .filter(|target| target.availability != AcceleratorAvailability::Unsupported)
        .map(|target| target.triple.as_str())
        .collect()
}

fn reject_capability_cycles(
    packages: &[AcceleratorPackageDescriptor],
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    provenance: &Path,
) -> AcceleratorResult<()> {
    fn visit(
        package: &str,
        dependencies: &BTreeMap<String, BTreeSet<String>>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Option<String> {
        if visited.contains(package) {
            return None;
        }
        if !visiting.insert(package.to_string()) {
            return Some(package.to_string());
        }
        if let Some(required) = dependencies.get(package) {
            for dependency in required {
                if let Some(member) = visit(dependency, dependencies, visiting, visited) {
                    return Some(member);
                }
            }
        }
        visiting.remove(package);
        visited.insert(package.to_string());
        None
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for package in packages {
        if let Some(member) = visit(&package.package, dependencies, &mut visiting, &mut visited) {
            let member_package = packages
                .iter()
                .find(|candidate| candidate.package == member)
                .unwrap_or(package);
            return Err(member_package
                .descriptor
                .diagnostic_spans
                .diagnostic(
                    "capability-cycle",
                    member_package.descriptor.diagnostic_spans.descriptor(),
                    format!("accelerator capability dependency cycle includes package `{member}`"),
                    provenance,
                )
                .into());
        }
    }
    Ok(())
}

fn validate_kernel(
    kernel: &AcceleratorKernel,
    descriptor: &AcceleratorDescriptor,
    provenance: &Path,
) -> AcceleratorResult<()> {
    if !descriptor
        .artifact_formats
        .contains(&kernel.artifact_format)
    {
        return Err(format!(
            "{}: accelerator kernel `{}` uses undeclared artifact format `{}`",
            provenance.display(),
            kernel.id,
            kernel.artifact_format
        )
        .into());
    }
    validate_relative_path(Path::new(&kernel.artifact), provenance)?;
    require_nonempty(&kernel.symbol, "kernel symbol", provenance)?;
    validate_unique(
        &kernel.target_architectures,
        "kernel target_architectures",
        provenance,
    )?;
    validate_named(
        &kernel.parameters,
        |value| &value.name,
        "kernel parameter",
        provenance,
    )?;
    for parameter in &kernel.parameters {
        if parameter.dtype != "scalar" && !descriptor.dtypes.contains(&parameter.dtype) {
            return Err(format!(
                "{}: kernel `{}` parameter `{}` uses undeclared dtype `{}`",
                provenance.display(),
                kernel.id,
                parameter.name,
                parameter.dtype
            )
            .into());
        }
        if parameter.access != AcceleratorAccess::Value
            && !descriptor.address_spaces.contains(&parameter.address_space)
        {
            return Err(format!(
                "{}: kernel `{}` parameter `{}` uses undeclared address space `{}`",
                provenance.display(),
                kernel.id,
                parameter.name,
                parameter.address_space
            )
            .into());
        }
    }
    Ok(())
}

fn validate_named<T>(
    values: &[T],
    name: impl Fn(&T) -> &str,
    label: &str,
    provenance: &Path,
) -> AcceleratorResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        let name = name(value);
        validate_identifier(name, label, provenance)?;
        if !seen.insert(name) {
            return Err(format!(
                "{}: duplicate accelerator {label} `{name}`",
                provenance.display()
            )
            .into());
        }
    }
    Ok(())
}

fn validate_unique(values: &[String], label: &str, provenance: &Path) -> AcceleratorResult<()> {
    if values.is_empty() {
        return Err(format!(
            "{}: accelerator `{label}` must not be empty",
            provenance.display()
        )
        .into());
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_identifier(value, label, provenance)?;
        if !seen.insert(value) {
            return Err(format!(
                "{}: accelerator `{label}` contains duplicate `{value}`",
                provenance.display()
            )
            .into());
        }
    }
    Ok(())
}

fn validate_unique_allow_empty(
    values: &[String],
    label: &str,
    provenance: &Path,
) -> AcceleratorResult<()> {
    if values.is_empty() {
        return Ok(());
    }
    validate_unique(values, label, provenance)
}

fn ensure_subset(
    values: &[String],
    allowed: &[String],
    label: &str,
    provenance: &Path,
) -> AcceleratorResult<()> {
    for value in values {
        if !allowed.contains(value) {
            return Err(format!(
                "{}: accelerator {label} `{value}` is not declared by the package",
                provenance.display()
            )
            .into());
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, label: &str, provenance: &Path) -> AcceleratorResult<()> {
    let valid = !value.is_empty()
        && value == value.trim()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
    if !valid {
        return Err(format!(
            "{}: accelerator {label} `{value}` is not a stable lowercase identifier",
            provenance.display()
        )
        .into());
    }
    Ok(())
}

fn require_nonempty(value: &str, label: &str, provenance: &Path) -> AcceleratorResult<()> {
    if value.trim().is_empty() || value != value.trim() {
        return Err(format!(
            "{}: accelerator {label} must not be empty or padded",
            provenance.display()
        )
        .into());
    }
    Ok(())
}

fn validate_relative_path(path: &Path, provenance: &Path) -> AcceleratorResult<()> {
    let unsafe_path = path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        });
    if unsafe_path {
        return Err(format!(
            "{}: accelerator artifact `{}` must be package-relative without traversal",
            provenance.display(),
            path.display()
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "accelerator_test.rs"]
mod accelerator_test;
