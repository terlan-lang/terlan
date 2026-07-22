# Doctor Command Internals

This directory owns the `terlc doctor` command. The implementation is centered
on scanning project files for VM-pivot migration hazards and reporting exact
fixes. Its most important boundary is that it diagnoses existing project state
without rewriting files or running a build.

## Responsibilities

- Parse the optional project-directory argument.
- Scan manifests, generated outputs, source files, summary artifacts, and
  scripts for retired Erlang/BEAM-era contracts.
- Report stable finding codes with actionable fix text.
- Keep filesystem and parse failures deterministic for migration workflows.

## Public Surface

- `run`: CLI entrypoint called by the top-level command router.
- `doctor_project`: project scanner used by focused command tests.
- `parse_doctor_args`: command-local argument parser.

## Core Model

The command walks one project root, collects `DoctorFinding` values, sorts them
for stable output, and returns a command exit code based on whether findings
exist. Each finding carries a project-relative path, stable code, summary, and
fix so diagnostics stay actionable.

The main flow is:

1. Parse zero or one project-directory operand.
2. Canonicalize the project root and scan known project surfaces.
3. Compile readable Terlan sources far enough to detect VM execution gaps.
4. Sort and render findings, or print `terlc doctor: ok`.

Important invariants:

- `doctor` never mutates project files.
- Finding codes are stable because tests and migration docs may reference them.
- Generated output and dependency directories are skipped during recursive
  project scans.

## Integration Points

- `terlan_syntax`: parses source files before deeper VM-gap checks.
- `terlan_typeck`: produces CoreIR for execution-gap diagnostics.
- `validation::target_profile`: keeps VM capability checks aligned with the
  compiler target-profile model.

## Edge Cases

- Missing project directories are reported as command errors.
- Stale summary fingerprints are diagnosed without rebuilding summaries.
- Retired manifest artifact metadata receives generic clean/build migration
  wording.

## Types And Interfaces

`DoctorFinding`
: Stable diagnostic record rendered by the command and asserted by tests.

## Testing Notes

- `doctor_test.rs` covers argument parsing, clean-project behavior, VM-pivot
  hazards, stale summary diagnostics, and manifest migration wording.
- Add a focused test whenever a new finding code or fix string is introduced.
