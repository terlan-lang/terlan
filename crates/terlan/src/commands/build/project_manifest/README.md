# Project Manifest Internals

This directory owns `terlan.toml` parsing for build commands. The implementation
is centered on project source roots, dependencies, and build profiles. Its most
important boundary is that callers consume normalized manifest data rather than
raw TOML tables.

## Responsibilities

- Read and validate project manifests.
- Normalize source roots and dependency metadata.
- Preserve deterministic build configuration for compiler commands.

## Public Surface

- `mod.rs`: project manifest parser and normalized manifest types.

## Core Model

The manifest is the project-level contract between source layout and compiler
commands.

The main flow is:

1. Locate `terlan.toml`.
2. Parse TOML into typed manifest data.
3. Resolve source roots and dependency paths.

Important invariants:

- Build commands must not guess source roots when the manifest is invalid.
- Local dependency paths must resolve relative to the manifest.
- Diagnostics must point to the manifest field where possible.

## Integration Points

- `commands::build`: consumes normalized project build settings.
- `commands::init`: writes manifests that this module must accept.

## Testing Notes

- Build command tests cover manifest source roots and local dependencies.
