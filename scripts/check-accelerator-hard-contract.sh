#!/usr/bin/env bash
set -euo pipefail

root=${1:-$(pwd)}
cd "$root"

for path in \
  crates/terlan/src/compiler/syntax \
  crates/terlan/src/compiler/typeck \
  crates/terlan/src/compiler/hir \
  crates/terlan/src/compiler/native_ir \
  crates/terlan/src/runtime/vm/scheduler.rs \
  crates/terlan/src/runtime/vm/resource.rs
do
  if rg -n -i '\b(cuda|nvidia|cudarc|ptx)\b' "$path" -g '*.rs' -g '!**/*test*'; then
    echo "error[accelerator_hard_contract]: package-specific accelerator term in $path" >&2
    exit 1
  fi
done

if rg -n -i '\b(cuda|gpu|kernel|device|accelerator)\b' docs/grammar/TERLAN_SYNTAX_SPEC.ebnf; then
  echo 'error[accelerator_hard_contract]: accelerator-specific language syntax detected' >&2
  exit 1
fi

if rg -n -i '\b(cudarc|cuda-sys|nvidia)\b' Cargo.toml crates/*/Cargo.toml; then
  echo 'error[accelerator_hard_contract]: compiler workspace depends on a CUDA implementation' >&2
  exit 1
fi

test -f crates/terlan/src/compiler/accelerator/aot/llvm_nvptx.rs
test -f tests/fixtures/accelerator-synthetic/accelerator.toml
echo '[accelerator-hard-contract] status=passed syntax=ordinary package_boundary=isolated cpu_build=independent'
