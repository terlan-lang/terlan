# Terlan lexical binding identities

Terlan local bindings are immutable identities, not mutable slots. The
compiler rejects a second declaration of the same name in one lexical binding
region with `error[binding.same_region]`. It never interprets a repeated name
as an equality match or silently replaces the earlier value.

Function-head parameters and the function body's top-level sequential `let`
chain share one region. Lambda heads and their top-level chains behave the same
way. Every individual structural pattern and every grouped refutable-let
success chain also rejects duplicate names. Tuple, list, list-cons, map,
record, constructor, string-capture, binary-layout, expanded shape, and alias
patterns follow this rule.

A selected `case` or `if` branch, nested lambda, comprehension body, handler
body, or another compiler-defined nested lexical scope creates a new region.
The nested binding may use the same spelling, but receives a different
`CoreBindingId`; leaving that region reveals the unchanged outer value.
Identity requirements inside a pattern must use a guard or explicit `==`.

## Compiler contract

Binding analysis runs after hygienic macro and shape expansion and before type
inference populates its local type environment. Every accepted declaration and
local reference is resolved to a deterministic `CoreBindingId` and
`CoreBindingRegionId`. The IDs derive from semantic declaration and lexical
paths, so inserting an unrelated declaration does not renumber an existing
function's bindings.

Checked CoreIR carries `CoreBindingIdentityEvidence`, including declarations,
references, source paths, regions, and a deterministic fingerprint. The formal
pipeline validates this evidence before backend lowering. Missing targets,
duplicate IDs, zero IDs, and stale fingerprints are loud
`error[core.binding_identity]` failures.

Cranelift and JavaScript consume the same checked CoreIR. Debugger local
enumeration, source navigation, closure analysis, and incremental invalidation
therefore share compiler-owned identities instead of guessing from identifier
text.

## Pattern failure and tooling

Binding identity does not change refutability. Refutable assertions still
produce `MatchError`; a grouped `<- ... else` chain commits bindings only after
the complete success path matches.

The formatter preserves lexical regions and never creates a collision while
migrating retired implicit repeated-let syntax. LSP definition, references,
rename, semantic tokens, and duplicate-binding quick fixes resolve the exact
identity. Renaming a nested binding cannot rewrite an outer or sibling binding.

`make binding-shadowing-safety-check` owns the focused compiler, expansion,
formatter, CoreIR, Cranelift, JavaScript, debugger, incremental, and editor
evidence.
