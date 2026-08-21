#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
site_root="$(cd "${script_dir}/.." && pwd)"
repo_root="$(cd "${site_root}/../.." && pwd)"
"${script_dir}/check-angular-css.sh"
if [[ -z "${TERLC:-}" ]]; then
  cargo build --locked --manifest-path "${repo_root}/Cargo.toml" -p terlan --bin terlc
  terlc="${repo_root}/target/debug/terlc"
else
  terlc="${TERLC}"
  if [[ ! -x "${terlc}" ]]; then
    echo "TERLC is not executable: ${terlc}" >&2
    exit 1
  fi
fi
TERLC="${terlc}" "${repo_root}/crates/terl-docs/scripts/check-search-policy.sh"
"${terlc}" test "${repo_root}/crates/terl-docs/terlan" --target terlan-vm

check_root="$(mktemp -d)"
trap 'rm -rf -- "${check_root}"' EXIT

check_build() {
  local name="$1"
  local base_path="$2"
  local out_dir="${check_root}/${name}"

  "${terlc}" static check \
    "${site_root}/src/terlan_io/Site.terl" \
    --out-dir "${out_dir}" \
    --docs \
    --as-of "2026-08-16" \
    --base-path "${base_path}"

  test -f "${out_dir}/index.html"
  test -f "${out_dir}/docs/index.html"
  test -f "${out_dir}/docs/getting-started/index.html"
  test -f "${out_dir}/start/index.html"
  test -f "${out_dir}/blog/introducing-terlan/index.html"
  test -f "${out_dir}/blog/archive/index.html"
  test -f "${out_dir}/blog/tags/compiler/index.html"
  test -f "${out_dir}/blog/tags/documentation/index.html"
  test -f "${out_dir}/blog/authors/terlan-team/index.html"
  test ! -e "${out_dir}/blog/search-roadmap/index.html"
  test ! -e "${out_dir}/search-notes/index.html"
  test ! -e "${out_dir}/blog/compiler-notes/index.html"
  test ! -e "${out_dir}/upcoming/index.html"
  test -f "${out_dir}/site.css"
  test -f "${out_dir}/angular.css"
  test -f "${out_dir}/CNAME"
  test -f "${out_dir}/search-index.json"
  test -f "${out_dir}/blog-index.json"
  test -f "${out_dir}/blog-collections.json"
  test -f "${out_dir}/navigation.json"
  test -f "${out_dir}/assets/terl-docs/search.js"
  test -f "${out_dir}/assets/terl-docs/search-policy.js"

  rg -F "<base href=\"${base_path}\">" "${out_dir}/index.html" >/dev/null
  rg -F "<base href=\"${base_path}\">" \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'data-terl-docs-search' "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'angular-ts' "${out_dir}/assets/terl-docs/search.js" >/dev/null
  rg -F 'data-slot="command-input"' "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'class="docs-navigation"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'href="docs/getting-started/" data-active="true" aria-current="page"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'data-slot="sidebar-menu-sub-button"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'aria-label="Breadcrumb"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'aria-label="On this page"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'href="docs/getting-started/#install"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'href="docs/getting-started/#main-content"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'id="install"' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F 'http-equiv="refresh" content="0; url=docs/getting-started/"' \
    "${out_dir}/start/index.html" >/dev/null
  rg -F "<base href=\"${base_path}\">" "${out_dir}/start/index.html" >/dev/null
  rg -F 'data-slot="pagination-next" rel="next" href="docs/language/">' \
    "${out_dir}/docs/getting-started/index.html" >/dev/null
  rg -F '"url": "docs/getting-started/"' "${out_dir}/search-index.json" >/dev/null
  rg -F '"parent": "docs/"' "${out_dir}/navigation.json" >/dev/null
  rg -F '"published_at": "2026-08-15"' "${out_dir}/blog-index.json" >/dev/null
  rg -F '"summary": "Why terlan.io is built on the compiler' \
    "${out_dir}/blog-index.json" >/dev/null
  rg -F '"Terlan team"' "${out_dir}/blog-index.json" >/dev/null
  rg -F '"documentation"' "${out_dir}/blog-index.json" >/dev/null
  rg -F '<h1>Blog&#32;archive</h1>' "${out_dir}/blog/archive/index.html" >/dev/null
  rg -F 'Introducing the Terlan documentation stack' \
    "${out_dir}/blog/archive/index.html" >/dev/null
  rg -F 'blog/authors/terlan-team/' \
    "${out_dir}/blog-collections.json" >/dev/null
  if rg -F 'Search roadmap notes' "${out_dir}/search-index.json" >/dev/null; then
    echo "draft leaked into production search index" >&2
    return 1
  fi
  if rg -F 'Compiler notes for the next release' "${out_dir}/search-index.json" >/dev/null; then
    echo "scheduled post leaked past production cutoff" >&2
    return 1
  fi
  test "$(<"${out_dir}/CNAME")" = "terlan.io"
}

check_build root "/"
check_build project-prefix "/terlan-preview/"

preview_dir="${check_root}/preview"
"${terlc}" static check \
  "${site_root}/src/terlan_io/Site.terl" \
  --out-dir "${preview_dir}" \
  --docs \
  --preview \
  --base-path "/"
test -f "${preview_dir}/blog/search-roadmap/index.html"
test -f "${preview_dir}/search-notes/index.html"
test -f "${preview_dir}/blog/compiler-notes/index.html"
test -f "${preview_dir}/upcoming/index.html"
test -f "${preview_dir}/blog/tags/search/index.html"
test -f "${preview_dir}/blog/tags/release/index.html"
rg -F 'Search roadmap notes' "${preview_dir}/search-index.json" >/dev/null
rg -F '"draft": true' "${preview_dir}/blog-index.json" >/dev/null
rg -F 'Compiler notes for the next release' "${preview_dir}/blog-index.json" >/dev/null
"${terlc}" static check \
  "${site_root}/src/terlan_io/Site.terl" \
  --out-dir "${preview_dir}" \
  --docs \
  --as-of "2026-08-16" \
  --base-path "/"
test ! -e "${preview_dir}/blog/search-roadmap/index.html"
test ! -e "${preview_dir}/search-notes/index.html"
test ! -e "${preview_dir}/blog/compiler-notes/index.html"
test ! -e "${preview_dir}/upcoming/index.html"
test ! -e "${preview_dir}/blog/tags/search/index.html"
test ! -e "${preview_dir}/blog/tags/release/index.html"
if rg -F 'Search roadmap notes' "${preview_dir}/search-index.json" >/dev/null; then
  echo "draft remained after preview output was rebuilt for production" >&2
  exit 1
fi

echo "terlan.io static checks passed"
