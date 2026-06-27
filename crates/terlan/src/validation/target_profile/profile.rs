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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TargetProfile {
    /// Current formal frontend/backend path: accepts the existing CoreIR surface.
    #[default]
    Erlang,
    /// Frozen 0.0.1 release-candidate Erlang artifact subset.
    A0Erlang,
    /// Named A0.1 successor Erlang artifact subset for simple Int expressions.
    A01Erlang,
    /// Named A0.2 successor Erlang artifact subset for boolean expressions.
    A02Erlang,
    /// Named A0.3 successor Erlang artifact subset for conditional expressions.
    A03Erlang,
    /// Named A0.4 successor Erlang artifact subset for simple case expressions.
    A04Erlang,
    /// Named A0.5 successor Erlang artifact subset for raw atom literals.
    A05Erlang,
    /// Named A0.6 successor Erlang artifact subset for tuple values.
    A06Erlang,
    /// Named A0.7 successor Erlang artifact subset for list values.
    A07Erlang,
    /// Named A0.8 successor Erlang artifact subset for binary/string literals.
    A08Erlang,
    /// Named A0.9 successor Erlang artifact subset for expression-side list cons.
    A09Erlang,
    /// Named A0.10 successor Erlang artifact subset for local named calls.
    A010Erlang,
    /// Named A0.11 successor Erlang artifact subset for unary negation.
    A011Erlang,
    /// Named A0.12 successor Erlang artifact subset for resolved constructor calls.
    A012Erlang,
    /// Named A0.13 successor Erlang artifact subset for resolved constructor patterns.
    A013Erlang,
    /// Named A0.14 successor Erlang artifact subset for anonymous function values.
    A014Erlang,
    /// Named A0.15 successor Erlang artifact subset for constructor extension.
    A015Erlang,
    /// Named A0.16 successor Erlang artifact subset for function-value invocation.
    A016Erlang,
    /// Named A0.17 successor Erlang artifact subset for struct field access.
    A017Erlang,
    /// Named A0.18 successor Erlang artifact subset for local let bindings.
    A018Erlang,
    /// Named A0.19 successor Erlang artifact subset for index access.
    A019Erlang,
    /// Named A0.20 successor Erlang artifact subset for qualified/scoped calls.
    A020Erlang,
    /// Named A0.21 successor Erlang diagnostic subset for unsupported references.
    A021Erlang,
    /// Shared JavaScript module profile with no browser-only ambient access.
    JsShared,
    /// Browser JavaScript profile for explicit browser and DOM bindings.
    JsBrowser,
    /// Worker JavaScript profile for explicit worker-safe bindings.
    JsWorker,
    /// Portable CoreIR v0 subset: accepts only typed, Lean-covered CoreIR forms.
    CoreV0,
}

/// Coarse backend/runtime family for target routing.
///
/// Inputs:
/// - Supported target-profile variants and reserved future target spellings.
///
/// Output:
/// - Stable family identity used by CLI dispatch and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetFamily {
    Beam,
    Js,
    Wasm,
    Wasi,
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
            Self::Beam => "BEAM",
            Self::Js => "JS",
            Self::Wasm => "Wasm",
            Self::Wasi => "WASI",
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
            "wasm" | "wasm.core" | "wasm.browser" | "wasm.component" | "wasm.worker" => {
                Some(Self::Wasm)
            }
            "wasi" | "wasi.cli" | "wasi.http" | "wasi.worker" => Some(Self::Wasi),
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
            Self::Erlang => "erlang",
            Self::A0Erlang => "a0-erlang",
            Self::A01Erlang => "a0.1-erlang",
            Self::A02Erlang => "a0.2-erlang",
            Self::A03Erlang => "a0.3-erlang",
            Self::A04Erlang => "a0.4-erlang",
            Self::A05Erlang => "a0.5-erlang",
            Self::A06Erlang => "a0.6-erlang",
            Self::A07Erlang => "a0.7-erlang",
            Self::A08Erlang => "a0.8-erlang",
            Self::A09Erlang => "a0.9-erlang",
            Self::A010Erlang => "a0.10-erlang",
            Self::A011Erlang => "a0.11-erlang",
            Self::A012Erlang => "a0.12-erlang",
            Self::A013Erlang => "a0.13-erlang",
            Self::A014Erlang => "a0.14-erlang",
            Self::A015Erlang => "a0.15-erlang",
            Self::A016Erlang => "a0.16-erlang",
            Self::A017Erlang => "a0.17-erlang",
            Self::A018Erlang => "a0.18-erlang",
            Self::A019Erlang => "a0.19-erlang",
            Self::A020Erlang => "a0.20-erlang",
            Self::A021Erlang => "a0.21-erlang",
            Self::JsShared => "js.shared",
            Self::JsBrowser => "js.browser",
            Self::JsWorker => "js.worker",
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
    pub(crate) const fn family(&self) -> TargetFamily {
        match self {
            Self::JsShared | Self::JsBrowser | Self::JsWorker => TargetFamily::Js,
            Self::CoreV0 => TargetFamily::Core,
            Self::Erlang
            | Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => TargetFamily::Beam,
        }
    }
}
