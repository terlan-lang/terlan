# Standard-library interface summaries

`std/summaries/` stores compiler-readable `.typi` summaries for public standard-library modules.

These summaries let user projects depend on stdlib public interfaces without reparsing or rechecking stdlib source during normal builds.

## Naming

Summary files use the public Terlan module name:

```text
std.core.Option               -> std.core.Option.typi
std.collections.List          -> std.collections.List.typi
std.distributed.CRDT.ORSet    -> std.distributed.CRDT.ORSet.typi
```

## Contents

A stdlib `.typi` summary contains only public interface data:

```text
module name
public type declarations
public opaque type declarations
public struct declarations and exported field layouts
public function signatures
public trait declarations
public impl summaries
kind summaries
documentation summaries
interface hash
documentation hash
```

It must not include private function bodies or private helper definitions.

## Generation

Summary generation is deterministic:

```text
stdlib .tl source
  -> parse
  -> lower public interface
  -> validate docs and examples
  -> emit deterministic .typi
```

Generated summaries are complete only when:

- `.tl` implementation coverage is 100%
- public documentation coverage is 100%
- public documentation examples pass doctests
- generated summary hashes are stable across repeated builds

## Build behavior

Normal user builds load shipped summaries:

```text
load std/summaries/*.typi
do not reparse stdlib source
do not re-type-check stdlib source
```

Stdlib development builds regenerate summaries from source and compare them against the checked-in/generated summaries.

See `std/summaries/PRECOMPILED_LOADING.md` for the precompiled loading model used by normal user builds.
