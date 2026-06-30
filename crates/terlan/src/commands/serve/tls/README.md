# Serve TLS Internals

This directory owns TLS runtime planning for `terlc serve`. The implementation
turns validated project/server TLS configuration into runtime certificate and
ACME handling decisions without making the HTTP handler own certificate policy.

## Responsibilities

- Build runtime TLS plans for plain HTTP, manual certificates, internal local
  certificates, and ACME-managed certificates.
- Keep certificate cache and ACME challenge paths explicit and project-scoped.
- Reject invalid or unsafe TLS configuration before the server starts.

## Public Surface

- `runtime_tls_config`: converts a served package and optional manifest TLS
  configuration into the runtime TLS configuration.
- `acme_runtime_plan`: converts ACME configuration into the HTTP-01 challenge
  and certificate-cache plan.

## Core Model

TLS configuration is derived from the project manifest and package layout. This
module validates paths, provider mode, domain requirements, and cache
availability before serving begins.

Important invariants:

- Manual certificate paths must stay inside the project boundary.
- ACME auto TLS must have a certificate cache and valid domain configuration.
- Plain HTTP packages must not accidentally enable TLS behavior.

## Integration Points

- `commands/serve`: consumes runtime TLS plans when starting the Hyper server.
- `commands/build/project_manifest`: owns manifest parsing for TLS settings.
- `rustls` and ACME support code: own concrete TLS and certificate behavior.

## Testing Notes

- `tls_test.rs` covers runtime TLS plan construction and rejection paths.
- Serve command tests cover ACME challenge routing and cache preconditions.
