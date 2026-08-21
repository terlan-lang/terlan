use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

struct RequiredFile {
    path: &'static str,
    markers: &'static [&'static str],
}

const REQUIRED_FILES: &[RequiredFile] = &[
    RequiredFile {
        path: "docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md",
        markers: &[
            "current implemented normative contract for Terlan 0.0.7",
            "current-pre-freeze",
            "Language guarantee",
            "Managed VM guarantee",
            "NativeBoundary guarantee",
            "unsafe external adapters remain process-isolated by default",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_image/descriptor.rs",
        markers: &[
            "validate_descriptor(descriptor)?",
            "validate_managed_layouts",
            "validate_managed_collections",
            "validate_sorted_unique",
            "error[tvm.image.descriptor_size]",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_image/sealed.rs",
        markers: &[
            "pub(crate) fn admit",
            "error[tvm.image.seal_changed]",
            "admitted whole-image digest",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_image/package_validation.rs",
        markers: &[
            "error[tvm.package.loaded_image_drift]",
            "Successful admission and execution report",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/mod.rs",
        markers: &["#![forbid(unsafe_code)]", "without pulling in async runtimes"],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/resource.rs",
        markers: &[
            "generation-tagged handles",
            "pub fn validate_owner",
            "checked_add",
            "stale handles",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/capability_wire.rs",
        markers: &[
            "CAPABILITY_PROTOCOL_VERSION",
            "MAX_CAPABILITY_TERM_COUNT",
            "deny_unknown_fields",
            "validate_capability_term_budget",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/worker.rs",
        markers: &[
            "credit_limit",
            "RejectedBackpressure",
            "NativeBoundaryCancellationToken",
            "owner_process_id",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/commands/bind/c_abi_binding_generator/generator/binding_validation.rs",
        markers: &[
            "PointerOwnershipUnknown",
            "BorrowedLifetime",
            "MissingDestructor",
            "UnsupportedVariadicFunction",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/commands/bind/cpp_binding_generator/generator.rs",
        markers: &[
            "cpp.exception.crossing",
            "cpp.ownership.unknown",
            "Public package error without native exception payloads",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_image/native_image_test.rs",
        markers: &[
            "descriptor_validates_canonical_managed_layout_registry",
            "descriptor_rejects_invalid_abi_ids_and_boundary_types",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/resource_test.rs",
        markers: &[
            "process_owned_resource_rejects_non_owner",
            "dispose_removes_resource_and_rejects_stale_handle",
            "rejects_stale_generation_with_stable_error_code",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/capability_wire_test.rs",
        markers: &[
            "capability_wire_fails_closed_at_frame_and_version_limits",
            "capability_term_budget_rejects_excessive_recursive_work",
        ],
    },
    RequiredFile {
        path: "crates/terlan/src/runtime/native_boundary/dispatch/panic_boundary_test.rs",
        markers: &[
            "native_boundary_worker_panic_becomes_typed_error_without_payload_leak",
            "native_boundary_panic_guard_preserves_success_and_typed_error",
        ],
    },
];

const FORBIDDEN_SOURCE_FRAGMENTS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/native_boundary/mod.rs",
        &[
            "#![allow(unsafe_code)]",
            "#![allow(unsafe_op_in_unsafe_fn)]",
        ],
    ),
    (
        "crates/terlan/src/runtime/native_boundary/capability_sandbox.rs",
        &["trusted_in_shard", "trusted-in-shard"],
    ),
    (
        "crates/terlan/src/runtime/native_boundary/worker.rs",
        &[
            "extern \"C\"",
            "std::mem::transmute",
            "std::ptr::from_raw_parts",
        ],
    ),
];

/// Summary of the ABI 1 pre-freeze contract checks performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abi1PreFreezeSummary {
    /// Number of contract-owner files inspected.
    pub file_count: usize,
    /// Number of required implementation markers checked.
    pub required_marker_count: usize,
    /// Number of forbidden implementation shortcuts checked.
    pub forbidden_fragment_count: usize,
}

/// Checks that ABI 1's pre-freeze contract has its required implementation owners and controls.
pub fn run_abi1_pre_freeze(root: &Path) -> QualityResult<Abi1PreFreezeSummary> {
    let mut diagnostics = Vec::new();
    let mut required_marker_count = 0;

    for required in REQUIRED_FILES {
        let text = read(root, required.path, &mut diagnostics);
        required_marker_count += required.markers.len();
        if let Some(text) = text {
            for marker in required.markers {
                if !text.contains(marker) {
                    diagnostics.push(format!(
                        "{}: missing ABI 1 pre-freeze marker `{marker}`",
                        required.path
                    ));
                }
            }
        }
    }

    let mut forbidden_fragment_count = 0;
    for (path, fragments) in FORBIDDEN_SOURCE_FRAGMENTS {
        let text = read(root, path, &mut diagnostics);
        forbidden_fragment_count += fragments.len();
        if let Some(text) = text {
            for fragment in *fragments {
                if text.contains(fragment) {
                    diagnostics.push(format!("{path}: forbidden ABI 1 shortcut `{fragment}`"));
                }
            }
        }
    }

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(Abi1PreFreezeSummary {
        file_count: REQUIRED_FILES.len(),
        required_marker_count,
        forbidden_fragment_count,
    })
}

fn read(root: &Path, path: &str, diagnostics: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(root.join(path)) {
        Ok(text) => Some(text),
        Err(error) => {
            diagnostics.push(format!("{path}: failed to read ABI 1 owner: {error}"));
            None
        }
    }
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[abi1-pre-freeze] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "abi1_pre_freeze_test.rs"]
mod abi1_pre_freeze_test;
