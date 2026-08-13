# Exact binding navigation

Terlan editor features use compiler-owned lexical binding identities.
Definition, references, rename, readonly-variable semantic tokens, debugger
locals, and the duplicate-binding quick fix do not group variables merely
because they have the same spelling.

When a cursor is on a local declaration or use, the language server resolves
its `CoreBindingId` and edits only occurrences of that identity. A same-spelled
name in an outer, sibling, or nested region remains untouched. A rename that
would collide with another declaration in the same region is rejected.

The stable duplicate diagnostic is `error[binding.same_region]`. Its quick fix
targets the second declaration and proposes the first available suffixed name,
such as `value_2`; it does not rewrite the original binding.

Non-local symbols continue through module/import navigation. The exact local
binding index is rebuilt from the same expanded compiler syntax used for
CoreIR evidence, so editor behavior and executable backends agree.
