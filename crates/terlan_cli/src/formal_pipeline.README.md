# Formal Pipeline Support

This module owns shared formal compiler-path orchestration for CLI commands.
It is deliberately outside `main.rs` so the top-level file can stay focused on
entry-point parsing and routing.

## Responsibilities

- Parse `.terl` and `.terli` source text into formal syntax output.
- Load external interfaces from sibling files and optional cache directories.
- Resolve syntax output against interfaces.
- Run template-aware type validation.
- Return checked artifacts or phase diagnostics for command modules.
- Detect whether changed interfaces affect a syntax-output module.

## Public Surface

- `CheckedSyntaxModuleArtifacts`: syntax output, interface map, and resolved HIR.
- `CompileSyntaxModuleThroughPhasesResult`: artifacts plus phase diagnostics.
- `load_external_interfaces`: loads file-set and cached interfaces.
- `terlan_sources_in_dir`: lists implementation sources for directory commands.
- `syntax_module_imports_changed_interface`: checks dependency invalidation reachability.
- `parse_source_as_syntax_output`: dispatches `.terl` and `.terli` parsing.
- `compile_syntax_module_through_phases_with_diagnostics_for_profile`: full phase
  run with diagnostics and optional target-profile gate.
- `compile_syntax_module_through_phases_with_profile`: strict checked-artifact
  helper that fails when target-profile validation rejects the lowered CoreIR.

## Core Model

The formal pipeline consumes source text and produces backend-agnostic compiler
artifacts. It does not emit backend code or write cache files. Commands decide
how to present diagnostics, write artifacts, and handle command-specific flags.

Important invariants:

- Every formal compiler command should enter through this module or a narrower
  helper derived from it.
- The pipeline consumes `SyntaxModuleOutput`, not the AST adapter.
- Backend outputs must depend on resolved formal artifacts, not parser-specific
  implementation details.
- Parser, resolver, and type diagnostics are preserved separately for phase
  manifest output.
- Target-profile validation runs after CoreIR lowering. `erlang` remains the
  permissive backend profile, while `core-v0` is the portable subset profile
  used to reject broader CoreIR before backend emission experiments.

## Integration Points

- `commands::check`: emits phase manifests and incremental dependency checks.
- `commands::emit`: emits Erlang and interface artifacts from checked output.
- `commands::emit_js`: emits JavaScript from checked syntax output.
- `commands::static_site`: renders static HTML from checked syntax output.
- `commands::doc`: parses source through the formal parser and renders
  documentation artifacts.

## Testing Notes

Current coverage includes module-local tests for target-profile compile gating
plus CLI integration tests and phase-contract fixtures in `main.rs`. Move the
remaining command-heavy tests closer to this module when the remaining test
block is split out of `main.rs`.
