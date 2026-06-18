# Target Profile Validation

This module owns compile-time validation that checks whether a `CoreModule`
conforms to a specific backend-capability profile before backend emission.

The formal pipeline now lowers through backend-agnostic `CoreIR` first. Backend
profiles describe which `CoreIR` forms and evidence levels are acceptable for a
given target family. The current profiles are:

- `erlang`: permissive backend profile for the current CoreIR-gated Erlang
  emission path.
- `a0-erlang`: frozen 0.0.1 release-candidate Erlang artifact subset.
- `a0.1-erlang`: named successor profile for simple Int arithmetic and
  comparison expressions; it does not broaden the frozen 0.0.1 RC profile.
- `core-v0`: portable backend-agnostic subset profile that accepts only typed,
  Lean-covered CoreIR forms from the current v0 proof baseline and rejects
  runtime-boundary or broader backend-specific shapes.

## Responsibilities

- Define explicit backend target profiles used by compiler emit paths.
- Validate that `CoreExpr`/`CorePattern` summaries match the target profile
  contracts.
- Report deterministic, CLI-safe diagnostics when unsupported forms appear.
- Keep policy decisions isolated from command wiring and emit backends.

## Public Surface

- `TargetProfile`: profile options used by the formal pipeline.
- `target_profile_checks`: validates a lowered `CoreModule` for one profile and
  returns violation messages.
- `TargetProfileViolation`: normalized violation record including function,
  clause, and kind details.

## Core Model

The profile is intentionally conservative and currently expresses three
orthogonal constraints:

1. Allowed proof-coverage classes for expressions and patterns.
2. Required checked-preservation evidence level.
3. Required constructor-resolution state: unresolved constructor calls, chains,
   and patterns are rejected before backend validation.
4. Allowed core expression forms (for future structural subsets).

Validation is a pure traversal over `CoreModule::functions`:

- constructor-resolution metadata is checked before structural traversal;
- each function body and guard `CoreExprSummary` is validated recursively;
- each covered/unsupported function pattern summary entry is validated;
- each typed `CoreExpr` shape is additionally validated against the profile
  form matrix.

## CoreV0 Coverage Matrix

`core-v0` is the portable, Lean-covered baseline. The Erlang profile remains
permissive while the portable profile rejects every form that is not yet part of
that baseline.

| CoreIR family | CoreV0 status | Gate coverage |
| --- | --- | --- |
| Int, binary, atom, variable | accepted | validator traversal |
| Tuple and list values | accepted when all children are accepted | validator traversal |
| List cons expressions | accepted when head and tail are accepted | validator traversal |
| Resolved constructor calls | accepted when all arguments are accepted | constructor-resolution gate |
| Local calls | accepted when all arguments are accepted | validator traversal |
| Case expressions | accepted when guardless and patterns/bodies are accepted | validator traversal |
| If expressions | accepted when conditions and bodies are accepted | validator traversal |
| Field access | accepted when the base is accepted | validator traversal |
| Lambdas | accepted when params and body are accepted | validator traversal |
| Unary `-` | accepted when the operand is accepted | validator traversal |
| Binary `+`, `-`, `*`, `==`, `<`, `<=`, `>`, `>=` | accepted when operands are accepted | validator traversal |
| Floats | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| Fixed arrays | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| Index expressions | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| List comprehensions | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| Maps | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| Record construction/access/update | rejected | validator tests, CLI exact tests, `formal-core-proof-gate` |
| Template instantiation | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| Constructor chains | rejected | validator test, CLI exact test, `formal-core-proof-gate` |
| Remote function refs and remote calls | rejected | validator tests, CLI exact tests, `formal-core-proof-gate` |
| Try expressions | rejected | validator tests, CLI exact tests, `formal-core-proof-gate` |
| Wildcard, variable, int, atom patterns | accepted | validator traversal |
| Tuple and list patterns | accepted when all children are accepted | validator traversal |
| Resolved constructor patterns | accepted when all arguments are accepted | constructor-resolution gate |
| Float, list-cons, map, and record patterns | rejected | validator tests, CLI exact tests, `formal-core-proof-gate` |
| Typed expression and pattern payloads without checked-preservation evidence | rejected | validator tests, `formal-core-proof-gate` |

Any new `CoreExpr` or `CorePattern` variant must update this matrix before it is
accepted by `core-v0` or deliberately rejected by the profile validator.

## Integration Points

- `formal_pipeline.rs`: calls this validator after `CoreIR` lowering.
- `main.rs`: parses `--target-profile erlang|a0-erlang|a0.1-erlang|core-v0`
  and passes the selected profile into command execution state.
- `formal pipeline target-specific compile wrappers` (when command-specific target
  selection is introduced).

## Testing Notes

- `target_profile_accepts_float_for_erlang_profile` exercises that the Erlang
  profile permits permissive proof coverage.
- `target_profile_accepts_mathx_for_a0_erlang_profile` exercises the frozen A0
  arithmetic fixture profile.
- `target_profile_accepts_arithmetic_for_a0_1_erlang_profile` exercises the
  named A0.1 successor arithmetic/comparison profile.
- `target_profile_keeps_subtraction_out_of_a0_erlang_profile` exercises that
  the frozen A0 profile does not silently widen when A0.1 is introduced.
- `target_profile_rejects_float_expr_for_core_v0_profile` exercises that float
  literals remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_rejects_fixed_array_expr_for_core_v0_profile` exercises that
  fixed-array literals remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_rejects_index_expr_for_core_v0_profile` exercises that index
  expressions remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_rejects_list_comprehension_expr_for_core_v0_profile`
  exercises that list comprehensions remain outside `core-v0` while their
  proof coverage is `proof-model-required`.
- `target_profile_rejects_receive_expr_for_core_v0_profile` exercises that
  receive expressions remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_rejects_try_expr_for_core_v0_profile` exercises that try
  expressions remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_accepts_subtraction_for_core_v0_profile` exercises that the
  portable profile accepts Lean-covered arithmetic CoreIR.
- `target_profile_accepts_documented_core_v0_shape_matrix` exercises that the
  portable profile accepts the documented CoreV0 expression and pattern shape
  matrix through direct typed CoreIR validation.
- `target_profile_rejects_missing_expr_evidence_for_core_v0_profile` and
  `target_profile_rejects_missing_pattern_evidence_for_core_v0_profile`
  exercise that `core-v0` requires checked-preservation evidence for typed
  expression and pattern payloads.
- `target_profile_rejects_map_expr_for_core_v0_profile` exercises that the
  portable profile rejects broader backend-specific CoreIR while Erlang remains
  permissive.
- `target_profile_rejects_map_pattern_for_core_v0_profile` exercises that map
  patterns remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_rejects_list_cons_pattern_for_core_v0_profile` exercises
  that list-cons patterns remain outside `core-v0` while their proof coverage
  is `proof-model-required`.
- `target_profile_rejects_record_pattern_for_core_v0_profile` exercises that
  record patterns remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_allows_float_pattern_for_erlang_profile` and
  `target_profile_rejects_float_pattern_for_core_v0_profile` exercise that
  float patterns remain outside `core-v0` while Erlang remains permissive.
- `target_profile_rejects_constructor_chain_expr_for_core_v0_profile` exercises
  that constructor chains remain outside `core-v0` while their proof coverage is
  `partial`, even when the base constructor identity resolves.
- `target_profile_rejects_remote_call_expr_for_core_v0_profile` exercises that
  remote calls remain outside `core-v0` while their proof coverage is
  `proof-model-required`.
- `target_profile_rejects_remote_fun_ref_expr_for_core_v0_profile` exercises
  that remote function references remain outside `core-v0` while their proof
  coverage is `proof-model-required`.
- `target_profile_rejects_record_construct_expr_for_core_v0_profile` exercises
  that record construction remains outside `core-v0` while its proof coverage
  is `proof-model-required`.
- `target_profile_rejects_record_access_expr_for_core_v0_profile` exercises
  that record access remains outside `core-v0` while its proof coverage is
  `proof-model-required`.
- `target_profile_rejects_record_update_expr_for_core_v0_profile` exercises
  that record update remains outside `core-v0` while its proof coverage is
  `proof-model-required`.
- `target_profile_rejects_template_instantiate_expr_for_core_v0_profile`
  exercises that template instantiation remains outside `core-v0` while its
  proof coverage is `proof-model-required`.
- `parse_args_accepts_core_v0_target_profile`,
  `run_check_single_file_accepts_subtraction_for_core_v0_target_profile`,
  `run_check_single_file_accepts_alias_constructor_call_for_core_v0_target_profile`,
  `run_check_single_file_rejects_map_for_core_v0_target_profile`,
  `run_check_single_file_rejects_map_pattern_for_core_v0_target_profile`,
  `run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile`,
  `run_check_single_file_rejects_record_pattern_for_core_v0_target_profile`,
  `run_check_single_file_rejects_float_pattern_for_core_v0_target_profile`,
  `run_check_single_file_rejects_float_for_core_v0_target_profile`,
  `run_check_single_file_rejects_fixed_array_for_core_v0_target_profile`,
  `run_check_single_file_rejects_index_for_core_v0_target_profile`,
  `run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile`,
  `run_check_single_file_rejects_receive_for_core_v0_target_profile`,
  `run_check_single_file_rejects_try_for_core_v0_target_profile`,
  `run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile`,
  `run_check_single_file_rejects_alias_constructor_chain_for_core_v0_target_profile`,
  `run_check_single_file_rejects_remote_call_for_core_v0_target_profile`,
  `run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile`,
  `run_check_single_file_rejects_record_construct_for_core_v0_target_profile`,
  `run_check_single_file_rejects_record_access_for_core_v0_target_profile`,
  `run_check_single_file_rejects_record_update_for_core_v0_target_profile`, and
  `run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile`
  exercise the command-surface path from CLI state to `check` execution.
- `target_profile_allows_lambda_for_erlang_profile` exercises that the Erlang
  profile accepts lambda-shaped CoreIR terms.
- `target_profile_rejects_unresolved_constructor_call_candidate`,
  `target_profile_rejects_unresolved_constructor_pattern_candidate`, and
  `target_profile_rejects_unresolved_constructor_chain_candidate` exercise the
  hard constructor-resolution gate used before backend validation. These tests
  assert the shared `target_profile_unresolved_constructor` diagnostic code and
  the exact count-bearing payload for call, chain, and pattern candidates.
