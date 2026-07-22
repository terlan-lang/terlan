#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/terlan_readme_hello_world.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/src/hello"
mkdir -p "$tmp_dir/home" "$tmp_dir/xdg-cache" "$tmp_dir/xdg-config" "$tmp_dir/xdg-data"

awk '
    BEGIN { in_block = 0; seen = 0 }
    /^```terlan[[:space:]]*$/ {
        if (!seen) {
            in_block = 1
            seen = 1
            next
        }
    }
    /^```[[:space:]]*$/ {
        if (in_block) {
            exit
        }
    }
    {
        if (in_block) {
            print
        }
    }
' "$repo_root/README.md" > "$tmp_dir/src/hello/Main.terl"

cat > "$tmp_dir/terlan.toml" <<'EOF'
[package]
name = "hello"
version = "0.0.0"

[build]
source_roots = ["src"]
artifact = "terlan-vm"
EOF

output="$(
    cd "$tmp_dir"
    env \
        HOME="$tmp_dir/home" \
        XDG_CACHE_HOME="$tmp_dir/xdg-cache" \
        XDG_CONFIG_HOME="$tmp_dir/xdg-config" \
        XDG_DATA_HOME="$tmp_dir/xdg-data" \
        "$repo_root/target/debug/terlc" run
)"
expected="$(
    awk '
        BEGIN { in_block = 0; seen = 0 }
        /^```text[[:space:]]*$/ {
            if (!seen) {
                in_block = 1
                seen = 1
                next
            }
        }
        /^```[[:space:]]*$/ {
            if (in_block) {
                exit
            }
        }
        {
            if (in_block) {
                print
            }
        }
    ' "$repo_root/README.md"
)"

if [[ -z "$expected" ]]; then
    printf 'README hello-world expected-output block is empty or missing\n' >&2
    exit 1
fi

if [[ "$output" != "$expected" ]]; then
    printf 'README hello-world output mismatch\nexpected: %s\nactual: %s\n' "$expected" "$output" >&2
    exit 1
fi
