# Trusted pre-compiler bootstrap. No Terlan tool exists at this boundary.
# Compiler/runtime producers keep their ordinary Cargo configuration.
SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS ?= 3600
TERLAN_BOOTSTRAP_LOCK_WAIT_SECONDS ?= 120

.PHONY: hermetic-support-root hermetic-support-inner
hermetic-support-root:
	@case "$${MAKEFLAGS%% *}" in \
		*n*) $(MAKE) --no-print-directory -n -f mk/hermetic-support.mk hermetic-support-inner; exit ;; \
	esac; \
	test ! -L target && test ! -L target/quality || exit 1; \
	mkdir -p target/quality; \
	test ! -L target/quality/bootstrap-owner.lock; \
	test ! -e target/quality/bootstrap-owner.lock || test -f target/quality/bootstrap-owner.lock; \
	exec 6>>target/quality/bootstrap-owner.lock; \
	flock --exclusive --wait "$(TERLAN_BOOTSTRAP_LOCK_WAIT_SECONDS)" 6; \
	test /dev/fd/6 -ef target/quality/bootstrap-owner.lock; \
	$(TERLAN_HERMETIC_SUPPORT_ROOT)

# Keep the pre-tool mechanism limited to Git, archive/hash utilities, and the
# OS sandbox. The native receipt owner takes over as soon as it exists.
define TERLAN_HERMETIC_SUPPORT_ROOT
root="$$(pwd -P)"; \
scratch="$$root/target/quality/hermetic-support.pending"; \
cache="$$root/target/hermetic-support"; \
retire() { \
	test ! -L target && test ! -L target/quality || return 1; \
	if test -e "$$scratch" || test -L "$$scratch"; then \
		test -d "$$scratch" && test ! -L "$$scratch" || return 1; \
		(shopt -s nullglob dotglob; for entry in "$$scratch"/*; do \
			case "$${entry##*/}" in \
				source|install) test -d "$$entry" && test ! -L "$$entry" || exit 1 ;; \
				source.tar|paths|present|resolved|after.tar) test -f "$$entry" && test ! -L "$$entry" || exit 1 ;; \
				*) exit 1 ;; \
			esac; \
		done) || { echo "unrecognized hermetic bootstrap scratch" >&2; return 1; }; \
		rm -rf -- "$$scratch/source" "$$scratch/install"; \
		rm -f -- "$$scratch/source.tar" "$$scratch/after.tar" "$$scratch/paths" "$$scratch/present" "$$scratch/resolved"; \
		rmdir -- "$$scratch"; \
	fi; \
}; \
retire; \
mkdir -m 700 -- "$$scratch"; \
trap retire EXIT; trap "exit 129" HUP; trap "exit 130" INT; trap "exit 143" TERM; \
test "$$(git rev-parse --show-toplevel)" = "$$root"; \
channel="$$(sed -n 's/^channel = "\([0-9.]*\)"$$/\1/p' rust-toolchain.toml)"; \
test -n "$$channel"; \
native_cargo="$$(rustup which --toolchain "$$channel" cargo)"; \
toolchain="$$(dirname "$$(dirname "$$native_cargo")")"; \
native_cc="$$(readlink -f /usr/bin/cc)"; \
case "$$native_cc" in /usr/*) ;; *) echo "bootstrap linker must reside under /usr" >&2; exit 1 ;; esac; \
test "$$("$$toolchain/bin/cargo" --version | cut -d " " -f2)" = "$$channel"; \
test "$$("$$toolchain/bin/rustc" --version | cut -d " " -f2)" = "$$channel"; \
snapshot() { \
	git ls-files --cached --others --exclude-standard -z | LC_ALL=C sort -zu > "$$scratch/paths"; \
	: > "$$scratch/present"; count=0; \
	while IFS= read -r -d "" path; do \
		count=$$((count + 1)); test "$$count" -le 50000; \
		case "$$path" in .cargo|.cargo/*) continue ;; esac; \
		if test -e "$$path" || test -L "$$path"; then \
			printf "%s\0" "$$path" >> "$$scratch/present"; \
		fi; \
	done < "$$scratch/paths"; \
	test "$$count" -gt 0; \
	xargs -0 -r realpath -e -z -- < "$$scratch/present" > "$$scratch/resolved"; \
	while IFS= read -r -d "" resolved; do \
		case "$$resolved" in "$$root"/*) ;; *) echo "bootstrap source escapes checkout" >&2; return 1 ;; esac; \
		test -f "$$resolved" || { echo "bootstrap source is not a regular file" >&2; return 1; }; \
	done < "$$scratch/resolved"; \
	(ulimit -f 1048576; tar --format=gnu --null --verbatim-files-from --no-recursion --dereference \
		--mtime=@0 --owner=0 --group=0 --numeric-owner -cf "$$1" -T "$$scratch/present"); \
}; \
tool_identity() { \
	sha256sum "$$toolchain/bin/cargo" "$$toolchain/bin/rustc" "$$toolchain/bin/rustdoc" \
		/usr/bin/cc /usr/bin/ld /usr/bin/as /usr/bin/bwrap /usr/bin/make /usr/bin/bash; \
}; \
snapshot "$$scratch/source.tar"; \
source_hash="$$(sha256sum "$$scratch/source.tar" | cut -d " " -f1)"; \
tools_hash="$$(tool_identity | sha256sum | cut -d " " -f1)"; \
input="$$(printf "%s\n" terlan.hermetic-support.v1 "$$channel" "$$source_hash" "$$tools_hash" | sha256sum | cut -d " " -f1)"; \
mkdir -- "$$scratch/source" "$$scratch/install"; \
tar --touch -xf "$$scratch/source.tar" -C "$$scratch/source"; \
mkdir "$$scratch/source/target"; \
for directory in "$$cache" "$$cache/registry" "$$cache/git" "$$cache/target"; do \
	test ! -L "$$directory" && { test ! -e "$$directory" || test -d "$$directory"; } || exit 1; \
	mkdir -p -- "$$directory"; \
done; \
timeout --kill-after=10s "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)s" \
	/usr/bin/bwrap --unshare-all --share-net --die-with-parent --new-session --sync-fd 6 --clearenv \
	--ro-bind /usr /usr --symlink usr/bin /bin --symlink usr/lib /lib --symlink usr/lib64 /lib64 \
	--ro-bind /etc/ssl/certs /etc/ssl/certs --ro-bind /etc/resolv.conf /etc/resolv.conf --ro-bind /etc/hosts /etc/hosts \
	--proc /proc --dev /dev --tmpfs /tmp --dir /work --dir /cargo \
	--ro-bind "$$toolchain" /toolchain --ro-bind "$$scratch/source" /source \
	--bind "$$cache/target" /source/target --bind "$$cache/registry" /cargo/registry --bind "$$cache/git" /cargo/git \
	--setenv PATH /toolchain/bin:/usr/bin:/bin --setenv HOME /work --setenv CARGO_HOME /cargo \
	--setenv RUSTC /toolchain/bin/rustc --setenv RUSTDOC /toolchain/bin/rustdoc --setenv CARGO_BUILD_JOBS 1 \
	--setenv CARGO_INCREMENTAL 0 \
	--setenv RUSTFLAGS "-C linker=$$native_cc" --setenv CC "$$native_cc" \
	--setenv LANG C.UTF-8 --setenv TERLAN_SUPPORT_INPUT "$$input" \
	--setenv TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
	--chdir /source /usr/bin/make --no-print-directory -f mk/hermetic-support.mk hermetic-support-inner </dev/null; \
snapshot "$$scratch/after.tar"; \
test "$$source_hash" = "$$(sha256sum "$$scratch/after.tar" | cut -d " " -f1)" || { echo "bootstrap source changed during execution" >&2; exit 1; }; \
test "$$tools_hash" = "$$(tool_identity | sha256sum | cut -d " " -f1)" || { echo "bootstrap tools changed during execution" >&2; exit 1; }; \
test ! -L target/debug && { test ! -e target/debug || test -d target/debug; } || exit 1; \
mkdir -p target/debug; \
test "$$(stat -c %d "$$scratch/install")" = "$$(stat -c %d target/debug)"; \
for name in terlan-build-cache terlan-test-orchestrator; do \
	test ! -L "target/debug/$$name" && { test ! -e "target/debug/$$name" || test -f "target/debug/$$name"; } || exit 1; \
	if ! cmp -s "$$cache/target/debug/$$name" "target/debug/$$name"; then \
		cp --reflink=auto -- "$$cache/target/debug/$$name" "$$scratch/install/$$name"; \
		chmod 755 "$$scratch/install/$$name"; \
		mv -T -- "$$scratch/install/$$name" "target/debug/$$name"; \
	fi; \
done
endef

hermetic-support-inner:
	@test "$${PWD}" = /source && test -n "$${TERLAN_SUPPORT_INPUT}" || exit 1; \
	test "$${CARGO_HOME}" = /cargo && test "$${RUSTC}" = /toolchain/bin/rustc || exit 1; \
	mkdir -p target/quality; \
	receipt=target/quality/hermetic-support.json; \
	log=target/quality/hermetic-support-cargo.jsonl; \
	test ! -L "$$log" && { test ! -e "$$log" || test -f "$$log"; } || exit 1; \
	trap 'rm -f -- "$$log"' EXIT; \
	set -- cargo --locked build -p terlan-test-orchestrator -p terlan-build-cache; \
	owner=(owner --receipt "$$receipt" --input-sha256 "$$TERLAN_SUPPORT_INPUT" \
		--timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
		--output target/debug/terlan-build-cache --output target/debug/terlan-test-orchestrator); \
	if test -x target/debug/terlan-build-cache && test -x target/debug/terlan-test-orchestrator \
		&& test "$$(timeout 5s target/debug/terlan-build-cache owner-protocol 2>/dev/null)" = terlan.build-owner.v4; then \
		target/debug/terlan-build-cache "$${owner[@]}" -- "$$@"; \
	else \
		"$$@" --message-format=json-render-diagnostics > "$$log"; \
		target/debug/terlan-build-cache "$${owner[@]}" --completed-cargo-log "$$log"; \
	fi
