# Deploy Command Internals

This directory owns the experimental `terlc --experimental deploy plan`
command. The implementation is centered on projecting `terlan.toml` into a
deterministic Terlan Cloud deploy-plan artifact. Its most important boundary is
that it reads existing compiler manifest data and does not provision
infrastructure directly.

## Responsibilities

- Parse hidden deploy command arguments.
- Read project manifests through the shared manifest parser.
- Emit `_build/cloud/deploy-plan.json` with stable schema metadata.
- Keep experimental deploy behavior out of the public top-level help surface.

## Public Surface

- `run`: CLI entrypoint for the hidden deploy command group.
- `write_deploy_plan`: manifest-to-plan artifact writer used by command tests.

## Core Model

The command converts a project manifest into a JSON deploy plan containing
package identity, source roots, artifact kind, web assets, TLS configuration,
targets, dependencies, and Erlang package adapter metadata.

The main flow is:

1. Require the global experimental flag.
2. Parse `deploy plan [project-dir]`.
3. Read `terlan.toml`, build a deploy-plan value, and write it under the build
   output directory.

Important invariants:

- Deploy planning is deterministic and filesystem-local.
- Manifest parsing remains owned by the build manifest module.
- Experimental command output must not imply that Terlan Cloud provisioning is
  implemented in the compiler.

## Integration Points

- `commands::build::project_manifest`: source manifest parser and typed
  manifest model.
- `serde_json`: stable JSON artifact serialization.
- Terlan Cloud prototypes: future consumer of the generated deploy plan.

## Edge Cases

- Missing `--experimental` returns a usage-style failure before reading files.
- Unknown deploy subcommands are rejected before artifact creation.
- Manifest read and output-directory failures are surfaced as command errors.

## Types And Interfaces

`DeployArgs`
: Parsed hidden command shape.

`DeployPlan`
: Serializable cloud-facing manifest projection.

## Testing Notes

- `deploy_test.rs` covers argument parsing and deploy-plan projection.
- Add focused tests when new manifest fields become deploy-plan fields.
- Provisioning behavior belongs outside this command until cloud runtime
  ownership is explicit.
