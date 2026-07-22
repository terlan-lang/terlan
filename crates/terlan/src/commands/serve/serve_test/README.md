# Serve Command Test Internals

This directory contains split integration tests for browser-package validation
and VM-owned HTTP serving.

## Responsibilities

- Validate package manifests before sockets or handlers start.
- Cover malformed metadata and missing artifact diagnostics.
- Keep large fixtures isolated from production serve code.

## Testing Notes

Run the serve exact-test selectors and `http-runtime-stack-check` after changes.
