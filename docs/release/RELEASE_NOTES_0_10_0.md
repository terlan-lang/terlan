# Terlan 0.10.0 Release Notes

Terlan 0.10.0 adds single executable application deployment, broader TypeScript
interop, and typed CSS generation. Angular binding generation and lifecycle
helpers belong to Angular.ts's `integrations/terlan` package; the compiler
accepts its pinned declarations through `terlc bind typescript --manifest`.

## Before upgrading

Rebuild applications and native packages with the 0.10.0 toolchain. Handwritten
native methods use bodyless `@compiler.native` declarations. Rename `FixedArray`
to `Array`, use the owning module's public type path, and replace the old CSS
optional-property wrappers and getters with direct typed values.

Native applications carry their image and VM in the executable. External service
dependencies retain their own deployment requirements. This does not freeze the
runtime ABI or claim completion of compiler self-hosting.

The classified changes from 0.0.9 and their migration guidance are recorded in
`COMPATIBILITY_0_10_0.json`.

## Validation requirements

Installed-candidate validation must cover the direct-AOT runtime, fixed-owner
multicore execution shards, the compiler and native worker, and package and
service deployment. Executable `.terls` scripts remain part of that validation.
Platform support is determined by the current candidate's successful artifact
matrix and smoke tests. Publication requires fresh candidate-bound evidence;
these notes do not substitute for it.
