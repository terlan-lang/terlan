mod pattern_shape;
mod shape;

/// Backend-capability profile for backend-aware compile gating.
///
/// Inputs:
/// - Caller-selected backend profile.
///
/// Output:
/// - Profile rules used by formal pipeline profile validation.
///
/// Transformation:
/// - Encodes profile constraints over proof-coverage classes and core
///   expression form families.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TargetProfile {
    /// Legacy VM backend path retained only for migration/reference checks.
    #[default]
    Vm,
    /// Frozen 0.0.1 release-candidate VM artifact subset.
    A0Vm,
    /// Named A0.1 successor VM artifact subset for simple Int expressions.
    A01Vm,
    /// Named A0.2 successor VM artifact subset for boolean expressions.
    A02Vm,
    /// Named A0.3 successor VM artifact subset for conditional expressions.
    A03Vm,
    /// Named A0.4 successor VM artifact subset for simple case expressions.
    A04Vm,
    /// Named A0.5 successor VM artifact subset for raw atom literals.
    A05Vm,
    /// Named A0.6 successor VM artifact subset for tuple values.
    A06Vm,
    /// Named A0.7 successor VM artifact subset for list values.
    A07Vm,
    /// Named A0.8 successor VM artifact subset for binary/string literals.
    A08Vm,
    /// Named A0.9 successor VM artifact subset for expression-side list cons.
    A09Vm,
    /// Named A0.10 successor VM artifact subset for local named calls.
    A010Vm,
    /// Named A0.11 successor VM artifact subset for unary negation.
    A011Vm,
    /// Named A0.12 successor VM artifact subset for resolved constructor calls.
    A012Vm,
    /// Named A0.13 successor VM artifact subset for resolved constructor patterns.
    A013Vm,
    /// Named A0.14 successor VM artifact subset for anonymous function values.
    A014Vm,
    /// Named A0.15 successor VM artifact subset for constructor extension.
    A015Vm,
    /// Named A0.16 successor VM artifact subset for function-value invocation.
    A016Vm,
    /// Named A0.17 successor VM artifact subset for struct field access.
    A017Vm,
    /// Named A0.18 successor VM artifact subset for local let bindings.
    A018Vm,
    /// Named A0.19 successor VM artifact subset for index access.
    A019Vm,
    /// Named A0.20 successor VM artifact subset for qualified/scoped calls.
    A020Vm,
    /// Named A0.21 successor VM diagnostic subset for unsupported references.
    A021Vm,
    /// Shared JavaScript module profile with no browser-only ambient access.
    JsShared,
    /// Browser JavaScript profile for explicit browser and DOM bindings.
    JsBrowser,
    /// Worker JavaScript profile for explicit worker-safe bindings.
    JsWorker,
    /// Core WebAssembly scalar ABI profile for pure exported functions.
    WasmCore,
    /// Retired proof-subset profile retained only while old profile tests are removed.
    CoreV0,
}

/// Coarse backend/runtime family for target routing.
///
/// Inputs:
/// - Supported target-profile variants and reserved future target spellings.
///
/// Output:
/// - Stable family identity used by CLI dispatch and diagnostics.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetFamily {
    Vm,
    Js,
    Wasm,
    Wasi,
    Mobile,
    NativeConstrained,
    Core,
}

impl TargetFamily {
    /// Human-readable family name for CLI diagnostics.
    ///
    /// Inputs:
    /// - One target family.
    ///
    /// Output:
    /// - Stable ASCII family label.
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Vm => "VM",
            Self::Js => "JS",
            Self::Wasm => "Wasm",
            Self::Wasi => "WASI",
            Self::Mobile => "Mobile",
            Self::NativeConstrained => "native constrained",
            Self::Core => "Core",
        }
    }

    /// Classifies reserved target names that do not have an implementation yet.
    ///
    /// Inputs:
    /// - Raw CLI target spelling.
    ///
    /// Output:
    /// - `Some(TargetFamily)` for reserved Wasm/WASI target families.
    /// - `None` for supported or unrelated target names.
    pub(crate) fn reserved_target(value: &str) -> Option<Self> {
        match value {
            "wasm" | "wasm.browser" | "wasm.component" | "wasm.worker" => Some(Self::Wasm),
            "wasi" | "wasi.cli" | "wasi.http" | "wasi.worker" => Some(Self::Wasi),
            "mobile" | "mobile.shell" | "mobile.android" | "mobile.ios" => Some(Self::Mobile),
            "native.no-std" | "native.bare-metal" | "native.kernel" | "native.rtos"
            | "native.riscv" | "native.arm" => Some(Self::NativeConstrained),
            _ => None,
        }
    }
}

impl TargetProfile {
    /// Human-readable profile name.
    ///
    /// Inputs:
    /// - One profile variant.
    ///
    /// Output:
    /// - Stable ASCII profile name.
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Vm => "vm",
            Self::A0Vm => "a0-vm",
            Self::A01Vm => "a0.1-vm",
            Self::A02Vm => "a0.2-vm",
            Self::A03Vm => "a0.3-vm",
            Self::A04Vm => "a0.4-vm",
            Self::A05Vm => "a0.5-vm",
            Self::A06Vm => "a0.6-vm",
            Self::A07Vm => "a0.7-vm",
            Self::A08Vm => "a0.8-vm",
            Self::A09Vm => "a0.9-vm",
            Self::A010Vm => "a0.10-vm",
            Self::A011Vm => "a0.11-vm",
            Self::A012Vm => "a0.12-vm",
            Self::A013Vm => "a0.13-vm",
            Self::A014Vm => "a0.14-vm",
            Self::A015Vm => "a0.15-vm",
            Self::A016Vm => "a0.16-vm",
            Self::A017Vm => "a0.17-vm",
            Self::A018Vm => "a0.18-vm",
            Self::A019Vm => "a0.19-vm",
            Self::A020Vm => "a0.20-vm",
            Self::A021Vm => "a0.21-vm",
            Self::JsShared => "js.shared",
            Self::JsBrowser => "js.browser",
            Self::JsWorker => "js.worker",
            Self::WasmCore => "wasm.core",
            Self::CoreV0 => "core-v0",
        }
    }

    /// Returns whether this profile targets JavaScript emission.
    ///
    /// Inputs:
    /// - One profile variant.
    ///
    /// Output:
    /// - `true` for JavaScript target profiles.
    ///
    /// Transformation:
    /// - Groups the initial JS profile family behind one predicate so import
    ///   validation can gate `std.js.*` without duplicating enum matches.
    pub(crate) const fn is_js(&self) -> bool {
        matches!(self, Self::JsShared | Self::JsBrowser | Self::JsWorker)
    }

    /// Returns the coarse runtime family for a supported target profile.
    ///
    /// Inputs:
    /// - One implemented target profile.
    ///
    /// Output:
    /// - Family identity used by dispatch code.
    #[allow(dead_code)]
    pub(crate) const fn family(&self) -> TargetFamily {
        match self {
            Self::JsShared | Self::JsBrowser | Self::JsWorker => TargetFamily::Js,
            Self::WasmCore => TargetFamily::Wasm,
            Self::CoreV0 => TargetFamily::Core,
            Self::Vm
            | Self::A0Vm
            | Self::A01Vm
            | Self::A02Vm
            | Self::A03Vm
            | Self::A04Vm
            | Self::A05Vm
            | Self::A06Vm
            | Self::A07Vm
            | Self::A08Vm
            | Self::A09Vm
            | Self::A010Vm
            | Self::A011Vm
            | Self::A012Vm
            | Self::A013Vm
            | Self::A014Vm
            | Self::A015Vm
            | Self::A016Vm
            | Self::A017Vm
            | Self::A018Vm
            | Self::A019Vm
            | Self::A020Vm
            | Self::A021Vm => TargetFamily::Vm,
        }
    }
}
