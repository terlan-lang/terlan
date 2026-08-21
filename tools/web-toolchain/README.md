# Terlan Web Toolchain

This is the compiler-owned browser toolchain. Terlan applications do not carry
Rspack, Rsbuild, Angular.ts, or a handwritten bundler configuration.

The package versions are exact and the lockfile is part of the compiler source
contract. Development checkouts provision it with:

```bash
npm ci --prefix tools/web-toolchain
```

Installed Terlan distributions place the same locked tree under
`lib/terlan/web-toolchain`. `TERLAN_WEB_TOOLCHAIN_ROOT` is reserved for release
tests and explicit toolchain relocation; application manifests cannot select a
different bundler implementation or version.
