# VM Artifact Build Internals

This directory owns deterministic construction and verified reuse of native VM
application images produced from checked compiler output.

## Responsibilities

- Compile development builds as independently reusable module objects followed
  by one application link.
- Compile release builds as one speed-optimized whole-application object with
  optimized native linking.
- Include code-generation policy, target, ABI topology, implementation, and
  debug metadata in content-addressed cache identities.
- Verify cached objects and images before publication or reuse.
- Bind dependency-free reuse indexes to the exact source generation, native
  image key, code-generation policy, target, and public adapter ABI.
- Keep worker concurrency bounded and restore canonical module order before
  lowering and linking.

## Testing Notes

Changes require focused build-artifact and VM round-trip tests. Cache admission
tests must cover poisoned keys, missing payloads, incomplete publications,
target or ABI drift, and valid images from the wrong source generation.
