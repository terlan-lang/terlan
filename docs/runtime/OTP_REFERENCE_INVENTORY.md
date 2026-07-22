# OTP Reference Inventory

Status: 0.0.7 baseline inventory.

This inventory records OTP, Erlang, and BEAM material that may be read as
semantic reference material while Terlan moves to a compiler-owned runtime
contract. These entries are not compatibility gates and do not make OTP a
supported runtime target.

## Rules

Every retained reference must name a Terlan capability. A reference may be
mined into a Terlan-owned conformance test only when it maps to one of these
ownership categories:

- `compiler-owned`
- `vm-owned`
- `boundary-owned`
- `reference-only`
- `out-of-contract`

Reference-only material must not be required by `make check`, `make
test-release`, or the default Terlan runtime. Unsupported OTP behavior must be
reported as an unsupported capability when it appears in an active Terlan
product path.

Only `compiler-owned`, `vm-owned`, and `boundary-owned` entries may be promoted
into active corpus fixtures. `reference-only` entries may be read or benchmarked
as evidence, and `out-of-contract` entries may be used only for rejection tests.
Stock OTP and `erlc` may be used as a reference compiler/oracle or temporary
migration bridge, but not as the default runtime path for active Terlan VM
artifacts.

## Entries

| Id | Source | Ownership | Terlan capability | Extraction status |
| --- | --- | --- | --- | --- |
| otp-pure-arithmetic | OTP arithmetic expression examples | compiler-owned | pure arithmetic lowering outside the VM when safe | pending |
| otp-loader-literals | OTP BEAM literal/chunk loader examples | vm-owned | VM artifact loading and literal decoding | mined |
| otp-send-receive | OTP process send/receive examples | vm-owned | Terlan process message delivery | pending |
| otp-selective-receive | OTP selective receive examples | vm-owned | Terlan selective receive cursor semantics | pending |
| otp-timer-timeout | OTP receive timeout examples | vm-owned | Terlan process timers and timeouts | pending |
| otp-supervision-exit | OTP link/monitor/exit examples | vm-owned | Terlan supervision and failure propagation | pending |
| otp-port-io | OTP port and IO examples | boundary-owned | typed host resource and IO boundary behavior | pending |
| otp-http-baseline | Existing generated Erlang HTTP lane | reference-only | pre-removal HTTP runtime performance baseline | mined |
| otp-nif-abi | OTP NIF ABI behavior | out-of-contract | native boundary rejects NIF ABI compatibility | rejected |

## Active Corpus Fixtures

| Id | Ownership | Terlan capability | Gate |
| --- | --- | --- | --- |
| otp-loader-literals | vm-owned | VM artifact loading and literal decoding | vm-artifact-format-check |

## Unsupported Corpus Rejections

| Id | Diagnostic |
| --- | --- |
| otp-nif-abi | `error[unsupported_capability]: native boundary rejects NIF ABI compatibility` |

## Gate

The inventory is guarded by:

```bash
make otp-reference-inventory-check
```
