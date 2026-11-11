#!/usr/bin/env bash
# Build changed Terlan sources with their package context and execute affected tests.
set -euo pipefail
changes=$1
base=$2
terlc=$3
typed_css_checked=$4
prechecked=${5:-/dev/null}
specialized_tests=${6:-/dev/null}
terlan_cache_dir=${TERLAN_HOOK_TERLAN_CACHE_DIR:-}
receipt_dir=${TERLAN_HOOK_VALIDATION_RECEIPT_DIR:-}
receipt_schema=terlan-hook-validation-v2
coverage_sources_supported=false
if "$terlc" test --help 2>&1 | rg -q -- '--coverage-source'; then
  coverage_sources_supported=true
fi
hash_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{ print $1 }'
  else
    echo 'Terlan validation receipts require sha256sum or shasum' >&2
    return 1
  fi
}
project_root() {
  local path=$1 directory
  directory=$(dirname "$path")
  while [[ "$directory" != / ]]; do
    if [[ -f "$directory/terlan.toml" ]]; then
      printf '%s\n' "$directory"
      return 0
    fi
    [[ "$directory" == . ]] && break
    directory=$(dirname "$directory")
  done
  case "$path" in
    std/*/*) printf '%s/%s\n' "${path%%/*}" "$(cut -d/ -f2 <<< "$path")" ;;
    *) dirname "$path" ;;
  esac
}
dependency_input_roots() {
  local root=$1 manifest="$1/terlan.toml" dependency resolved
  if [[ -f "$manifest" ]]; then
    while IFS= read -r dependency; do
      resolved=$(cd "$root" && cd "$dependency" 2>/dev/null && pwd -P) || continue
      case "$resolved/" in
        "$PWD"/*) printf '%s\n' "${resolved#"$PWD"/}" ;;
      esac
    done < <(sed -n 's/.*path[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest")
  fi
}
production_input_blobs() {
  local root=$1
  git ls-files -s -z -- "$root" \
    ":(exclude,glob)$root/**/*Test.terl" \
    ":(exclude,glob)$root/**/tests/**"
  if [[ $root == std ]]; then
    # `std.test.Test` is the assertion library, not a test module.
    git ls-files -s -z -- std/test/Test.terl
  fi
}
unit_input_blobs() {
  local root=$1 dependency
  git ls-files -s -z -- "$root"
  if [[ $root != . && $root != std ]]; then
    production_input_blobs std
  fi
  while IFS= read -r dependency; do
    [[ $dependency == "$root" ]] || production_input_blobs "$dependency"
  done < <(dependency_input_roots "$root")
}
unit_key() {
  local kind=$1 root=$2; shift 2
  local compiler_hash
  compiler_hash=$(git hash-object "$terlc")
  {
    printf '%s\n' "$receipt_schema" "$kind" "$root" "$compiler_hash"
    printf 'argument:%s\n' "$@"
    # The staged validation checkout has a clean index. Each unit owns its
    # tests, while shared stdlib and path dependencies contribute production
    # sources only. An unrelated test module cannot invalidate every project.
    unit_input_blobs "$root" | sort -zu
  } | hash_stream
}
run_cached_unit() {
  local kind=$1 root=$2; shift 2
  local compiler_hash key receipt temporary unit_cache= unit_cache_key
  if [[ -z "$receipt_dir" ]]; then
    run_terlc "$@"
    return
  fi
  test ! -L "$receipt_dir"
  mkdir -p "$receipt_dir"
  key=$(unit_key "$kind" "$root" "$@")
  if [[ -n "$terlan_cache_dir" ]]; then
    compiler_hash=$(git hash-object "$terlc")
    unit_cache_key="$(printf '%s\n' terlan-project-cache-v1 "$compiler_hash" "$root" | hash_stream)"
    unit_cache="$terlan_cache_dir/$unit_cache_key"
  fi
  receipt="$receipt_dir/$key.passed"
  if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 \
    && -f "$receipt" \
    && $(wc -l < "$receipt") == 2 \
    && $(sed -n '1p' "$receipt") == "$receipt_schema" \
    && $(sed -n '2p' "$receipt") == "$key" ]]; then
    echo "pre-commit: reuse $kind validation for $root ($key)"
    return 0
  fi
  if [[ $1 == project-check ]]; then
    shift
    TERLAN_ACTIVE_UNIT_CACHE_DIR="$unit_cache" run_project_check "$root" "$@"
  else
    TERLAN_ACTIVE_UNIT_CACHE_DIR="$unit_cache" run_terlc "$@"
  fi
  temporary=$(mktemp "$receipt_dir/.terlan-validation.XXXXXX")
  printf '%s\n%s\n' "$receipt_schema" "$key" > "$temporary"
  mv "$temporary" "$receipt"
}
run_terlc() {
  local started=$SECONDS status selected_cache_dir
  selected_cache_dir=${TERLAN_ACTIVE_UNIT_CACHE_DIR:-$terlan_cache_dir}
  echo "pre-commit: terlc $*"
  if [[ -n "$selected_cache_dir" ]]; then
    mkdir -p "$selected_cache_dir"
    if "$terlc" --cache-dir "$selected_cache_dir" "$@"; then status=0; else status=$?; fi
  else
    if "$terlc" "$@"; then status=0; else status=$?; fi
  fi
  local elapsed=$((SECONDS - started))
  echo "pre-commit: terlc ${1:-command} finished in ${elapsed}s"
  if (( elapsed > 60 )); then
    echo "pre-commit: red flag: terlc ${1:-command} exceeded 60s; rerun with --timings" >&2
  fi
  return "$status"
}
run_project_check() {
  local root=$1 path relative; shift
  if [[ -f "$root/terlan.toml" ]]; then
    (cd "$root" && run_terlc check src)
    for path in "$@"; do
      relative=${path#"$root"/}
      case "$relative" in
        src/*|terlan.toml) ;;
        *) (cd "$root" && run_terlc check "$relative") ;;
      esac
    done
  else
    for path in "$@"; do run_terlc check "$path"; done
  fi
}
tested_terlan=(scripts/git_hooks/src/git_hooks/PlanTest.terl scripts/self_validation/release_promotion/src/release_promotion/PreparationEnvironmentTest.terl)
queued_terlan=()
queued_test_roots=()
coverage_sources=()
queued_checks=()
queued_check_roots=()
queue_check() {
  local previous root
  for previous in "${queued_checks[@]}"; do
    if [[ "$previous" == "$1" ]]; then return 0; fi
  done
  root=$(project_root "$1")
  queued_checks+=("$1")
  queued_check_roots+=("$root")
}
run_queued_checks() {
  local roots=() root index previous command
  if [[ ${#queued_checks[@]} -eq 0 ]]; then return 0; fi
  for root in "${queued_check_roots[@]}"; do
    for previous in "${roots[@]}"; do [[ "$previous" == "$root" ]] && continue 2; done
    roots+=("$root")
  done
  for root in "${roots[@]}"; do
    command=(project-check)
    for index in "${!queued_checks[@]}"; do
      [[ ${queued_check_roots[$index]} == "$root" ]] && command+=("${queued_checks[$index]}")
    done
    echo "pre-commit: project-scoped Terlan source check: $root"
    run_cached_unit check "$root" "${command[@]}"
  done
}
run_terlan_test() {
  local coverage_source=$2 previous root
  for previous in "${tested_terlan[@]}"; do
    if [[ "$previous" == "$1" ]]; then return 0; fi
  done
  if [[ -f "$specialized_tests" ]] && rg -Fqx -- "$1" "$specialized_tests"; then
    queue_check "$1"
    tested_terlan+=("$1")
    echo "pre-commit: specialized owner will execute: $1"
    return 0
  fi
  # Older modules named `*Test.terl` expose a `self_test()` helper instead of
  # test declarations. Their owning package build above still type-checks them,
  # but `terlc test` correctly rejects an empty test selection. Only launch the
  # test runner when it can discover at least one declaration.
  if ! rg -q '^[[:space:]]*@test([[:space:](]|$)' "$1"; then
    echo "pre-commit: compiled legacy self-test module: $1"
    queue_check "$coverage_source"
    queue_check "$1"
    tested_terlan+=("$1")
    return 0
  fi
  root=$(project_root "$1")
  queued_terlan+=("$1")
  queued_test_roots+=("$root")
  coverage_sources+=("$coverage_source")
  tested_terlan+=("$1")
}
run_queued_terlan_tests() {
  local roots=() root index previous priority
  if [[ ${#queued_terlan[@]} -eq 0 ]]; then return 0; fi
  for root in "${queued_test_roots[@]}"; do
    for previous in "${roots[@]}"; do [[ "$previous" == "$root" ]] && continue 2; done
    roots+=("$root")
  done
  # Cheap package failures must surface before compiler self-validation. The
  # latter can take minutes even when a later stdlib unit fails in seconds.
  for priority in fast compiler; do
    for root in "${roots[@]}"; do
      case "$root" in
        compiler/self_host|compiler/semantic)
          [[ "$priority" == compiler ]] || continue
          ;;
        *) [[ "$priority" == fast ]] || continue ;;
      esac
      run_queued_test_root "$root"
    done
  done
}
run_queued_test_root() {
  local root=$1 index previous source
  local command=(test) sources=()
  for index in "${!queued_terlan[@]}"; do
    if [[ ${queued_test_roots[$index]} == "$root" ]]; then
      command+=("${queued_terlan[$index]}")
      source=${coverage_sources[$index]}
      for previous in "${sources[@]}"; do [[ "$previous" == "$source" ]] && continue 2; done
      sources+=("$source")
    fi
  done
  command+=(--coverage)
  if [[ "$coverage_sources_supported" == true ]]; then
    for source in "${sources[@]}"; do command+=(--coverage-source "$source"); done
  fi
  command+=(--coverage-threshold 100)
  echo "pre-commit: project-scoped affected Terlan tests: $root"
  run_cached_unit test "$root" "${command[@]}"
}
check_terlan_source() {
  if [[ "$1" == *terlan.toml ]]; then
    queue_check "$(dirname "$1")"
  elif [[ -f "$1" && "$1" != *Test.terl ]]; then
    queue_check "$1"
  fi
}

while IFS= read -r path; do
  if [[ "$path" == tests/fixtures/* \
    || "$path" == docs/grammar/fixtures/* \
    || "$path" == compiler/self_host/corpus/* ]]; then continue; fi
  # The typed CSS gate checks every generated property in bounded batches
  # and executes the handwritten CSS tests and browser fixture.
  if [[ "$typed_css_checked" == true && "$path" == std/css/* ]]; then continue; fi
  # JavaScript sources execute through the browser matrix below.
  if [[ "$path" == std/js/* || "$path" == tests/js/* ]]; then continue; fi
  if [[ "$path" == *.terl || "$path" == *.terls || "$path" == *.terli || "$path" == *terlan.toml ]]; then
    if [[ -f "$prechecked" ]] && rg -Fqx -- "$path" "$prechecked"; then
      echo "pre-commit: reused earlier Terlan validation: $path"
      continue
    fi
    if bash scripts/git_hooks/formatting_only.sh "$base" "$path" "$terlc"; then
      # Formatting-only tests do not enter the executable test queue, so retain
      # a cheap source check without linking a separate native image.
      if [[ "$path" == *Test.terl ]]; then
        queue_check "$path"
      fi
      echo "pre-commit: canonical formatting only: $path"
      continue
    fi
    if [[ "$path" == *Test.terl && -f "$path" ]]; then
      owner=${path%Test.terl}.terl
      if [[ ! -f "$owner" ]]; then owner=$path; fi
      run_terlan_test "$path" "$owner"
    elif [[ -f "${path%.terl}Test.terl" ]]; then
      run_terlan_test "${path%.terl}Test.terl" "$path"
    else
      check_terlan_source "$path"
    fi
  fi
done < "$changes"

run_queued_terlan_tests
run_queued_checks
