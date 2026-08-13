# Terlan termination and actor productivity

Terlan treats three questions independently:

1. Will this function terminate for every admitted input?
2. Can recursion execute without growing the native stack?
3. If an actor is intentionally persistent, can the VM regain control within
   a bounded reduction budget?

Stack-safe recursion is not automatically terminating. Failure to construct a
termination proof is recorded as `unproven`; it is not recorded as divergent
and does not reject an ordinary runtime function. A compiler context that
requires total behavior must call `CoreTerminationEvidence::require_total`.

## Checked CoreIR evidence

Every syntax-backed checked `CoreModule` carries
`terlan.core_termination.v1`. Each function has one deterministic state:

- `proven`;
- `unproven`;
- `intentional_persistent`;
- `productive_persistent`.

The initial proof subset recognizes nonrecursive functions, structural
subterms from list and constructor patterns, integer descent guarded by a
lower bound, one fixed lexicographic parameter order, and mutually recursive
components whose every internal edge decreases that order. Evidence records
every recursive edge, its per-argument relation, its tail-position fact, the
selected measure, and any productivity boundary.

The verifier recomputes the evidence from checked CoreIR. Missing, stale, or
forged evidence fails with `error[termination.evidence_invalid]`. Backends do
not create or strengthen proofs.

Compile-time const functions remain absent from executable CoreIR. Before the
const evaluator admits a local function call, it constructs the same proof-only
Core function shape and consumes its recomputable certificate through
`require_total`. An unproven recursive const function fails with
`CONST_TOTALITY_UNPROVEN`; deterministic step and output-size limits remain
resource bounds, not termination evidence. Recursive imported const functions
without validated evidence also fail loudly.

## Actor classification

There is no `actor` keyword and no required termination annotation. Typed
process operations identify actor behavior. A recursive actor component with
a receive, yield, timer, scheduler handoff, asynchronous capability boundary,
or compiler-owned tail-backedge reduction safepoint is productive. An actor
cycle without such a boundary remains valid but is reported as
`intentional_persistent`, making the missing productivity proof visible.

Message-handler termination remains separate from mailbox-loop persistence.
A finite handler may be proven total even when the selecting mailbox loop is
intentionally nonterminating.

## Native reduction boundary

NativeIR gives every recursive tail edge a stable reduction continuation.
Cranelift executes at most 1,024 such edges in one generated-code slice. At
budget exhaustion it returns the ordinary `Yield` transition, placing the
next arguments in the existing transition frame. The execution shard owns
parking, peer scheduling, cancellation, inspection, shutdown, and exact-owner
resume. A resumed continuation receives a fresh budget.

This mechanism does not poll a host event loop and does not block the shard
owner. It uses the same VM transition ABI as source-level yield and receive.
Non-tail recursive cycles do not receive this certificate and remain visibly
unproductive until rewritten or given another bounded handoff.

## Gate

`make termination-productivity-analysis-check` owns the focused evidence,
formal-pipeline, native reduction-yield, mailbox-pressure, scheduler,
cancellation, and supervision checks.
