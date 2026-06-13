# Precompiled stdlib summary loading

Terlan user builds should not reparse or re-type-check the standard library during normal compilation.

Instead, released compiler packages ship precomputed `.typi` summaries for all public stdlib modules.

## Normal user build

Normal builds load stdlib summaries before project modules:

```text
1. locate compiler-bundled stdlib summary directory
2. load std/summaries/*.typi
3. validate summary format version
4. validate stdlib version compatibility
5. expose public stdlib modules to name resolution
6. type-check user project against loaded summaries
```

Normal user builds must not:

- parse stdlib `.tl` source
- type-check stdlib `.tl` source
- regenerate stdlib `.typi`
- run stdlib doctests
- run stdlib law checks
- run distributed simulations

## Stdlib development build

Stdlib development mode regenerates summaries from source:

```text
1. parse stdlib `.tl` source
2. type-check stdlib source
3. enforce 100% `.tl` implementation coverage
4. enforce 100% public documentation coverage
5. run stdlib doctests
6. emit deterministic `.typi` summaries
7. compare summary hashes against checked/generated outputs
```

## Version checks

Each shipped summary must include:

- summary format version
- Terlan compiler version
- stdlib version
- module name
- interface hash
- documentation hash

The compiler may reject summaries when:

- the summary format version is unsupported
- the stdlib version is incompatible with the compiler
- the summary content hash is invalid
- duplicate public stdlib modules are found

## Search order

Summary lookup order:

```text
1. explicit --stdlib-summary-dir, if provided
2. compiler-bundled stdlib summary directory
3. development checkout std/summaries, only in stdlib development mode
```

Project-local modules must not override the reserved `std.*` namespace unless an explicit stdlib-development flag is enabled.
