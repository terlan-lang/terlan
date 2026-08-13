# Terlan Tail-Recursion Lowering

Status: compiler and executable-backend contract for Terlan 0.0.7.

## Source contract

Tail recursion is a Terlan language guarantee, not a host compiler
optimization. Portable source uses ordinary typed Terlan functions:

```terlan
module countdown.

pub count(n: Int, acc: Int): Int ->
    case n {
        0 -> acc;
        _ -> count(n - 1, acc + 1)
    }.
```

An admitted executable backend must run a statically resolved tail-recursive
component with bounded host-stack growth. Developers do not write
backend-specific loops to obtain this guarantee.

## Compiler analysis

NativeIR tail analysis runs after application-global function identities and
continuations have been materialized. Its iterative strongly connected
component analysis recognizes direct and mutual recursion across admitted
application modules without recursively walking the call graph.

A call is terminal only when its result is forwarded unchanged by the
enclosing function, terminal `let` body, selected `if`/`case` branch, or
cleanup-free result-forwarding control form. Calls used by argument evaluation,
operators, constructors, bindings, cleanup, or later work remain ordinary
calls. A dynamically selected terminal target inside a statically recursive
component is rejected before code generation because it cannot enter the
bounded dispatcher safely.

## Native AOT lowering

Cranelift receives compiler-structured loops:

- Direct self recursion jumps to the function loop header.
- A mutually recursive component uses a typed function tag and one bounded
  component dispatcher, including components split across object units.
- All next arguments are evaluated before the loop-header jump, giving
  simultaneous parameter replacement rather than sequential mutation.
- Typed managed argument slots are declared as precise stack-map roots in the
  loop frame. Padding is the canonical zero word, and a mutual component that
  would reuse one occupied slot as both scalar and managed fails before
  code generation.
- A suspending tail edge forwards status, continuation identity, transition
  values, and exact transition length without retaining a caller native frame.
- Non-tail recursion remains a real call and therefore retains its ordinary
  evaluation and stack behavior.

Object inspection is part of the contract: lowered recursive components cannot
relocate to their own component symbols. An equivalent unlowered control object
must retain that relocation so the quality gate is sensitive to removal of the
transform.

## JavaScript lowering boundary

The maintained JavaScript backend emits explicit `while` loops or component
dispatchers for the pure typed recursive subset it admits. It does not rely on
JavaScript engine proper-tail-call support. Results, checked failures, and
aggregate/collection identity must match native execution for the shared
subset.

This rule does not create a JavaScript actor runtime. Mailboxes, actor
suspension/resume, cancellation, shard ownership, capability execution, and
hot-reload generation retention are native VM obligations. JavaScript source
requiring those facilities remains a loud target error.

## Release evidence

`make tail-recursion-lowering-check` owns the focused proof:

- direct, mutual, terminal-`let`, and source-level terminal-`case` recursion at
  one million calls on a 128 KiB native stack;
- a 10,000-function recursive SCC analysis stress case;
- managed identity, allocation-count, parallel replacement, and precise-root
  stack-map checks;
- suspension, cancellation, checked failure, split-module object, non-tail,
  and transform-sensitivity checks;
- JavaScript direct/mutual recursion and value-identity execution without host
  proper-tail-call support; and
- native hot-reload generation retention while deeply recursive old and new
  generations remain leased.
