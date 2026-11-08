#!/usr/bin/env node

"use strict";

const { execFileSync } = require("node:child_process");
const fs = require("node:fs");

const PRODUCT_ROOTS = [
  "benchmarks/",
  "build/",
  "crates/",
  "mk/",
  "proofs/",
  "std/",
  "tests/",
  "tools/",
  "scripts/",
  "tree-sitter-terlan/",
];

const PRODUCT_FILES = new Set([
  "Cargo.toml",
  "Cargo.lock",
  "Makefile",
  "rust-toolchain.toml",
  "rustfmt.toml",
  "install.sh",
  "install.ps1",
]);

const COMPILER_ROOTS = ["build/", "crates/", "std/", "tests/", "tree-sitter-terlan/"];
const COMPILER_FILES = new Set(["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml"]);

function isProductFile(path) {
  return PRODUCT_FILES.has(path) || PRODUCT_ROOTS.some((root) => path.startsWith(root));
}

function isCompilerFile(path) {
  return COMPILER_FILES.has(path) || COMPILER_ROOTS.some((root) => path.startsWith(root));
}

function isDependencyFile(path) {
  return path === "Cargo.lock"
    || path === "Cargo.toml"
    || path.endsWith("/Cargo.toml")
    || path.endsWith("/package-lock.json")
    || path.endsWith("/requirements.txt");
}

function classify(paths, forceFull = false) {
  const normalized = [...new Set(paths.filter(Boolean))].sort();
  return {
    source: forceFull || normalized.some(isProductFile),
    compiler: forceFull || normalized.some(isCompilerFile),
    dependencies: forceFull || normalized.some(isDependencyFile),
    changed_count: normalized.length,
    changed_files: normalized,
  };
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index];
    const value = argv[index + 1];
    if (!name?.startsWith("--") || value === undefined) {
      throw new Error("usage: classify_changes.cjs --base SHA --head SHA --output PATH");
    }
    values.set(name.slice(2), value);
  }
  return values;
}

function changedPaths(base, head) {
  if (!/^[0-9a-f]{40}$/.test(head)) {
    throw new Error("head must be a full Git commit SHA");
  }
  if (!/^[0-9a-f]{40}$/.test(base) || /^0{40}$/.test(base)) {
    return { paths: [], forceFull: true };
  }
  const output = execFileSync("git", ["diff", "--name-only", "-z", base, head], {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
  return { paths: output.split("\0").filter(Boolean), forceFull: false };
}

function writeOutputs(path, result) {
  const lines = [
    `source=${result.source}`,
    `compiler=${result.compiler}`,
    `dependencies=${result.dependencies}`,
    `changed-count=${result.changed_count}`,
  ];
  fs.appendFileSync(path, `${lines.join("\n")}\n`, { encoding: "utf8" });
}

function main() {
  const argumentsMap = parseArguments(process.argv.slice(2));
  const base = argumentsMap.get("base") || "";
  const head = argumentsMap.get("head") || "";
  const output = argumentsMap.get("output") || "";
  if (!output) {
    throw new Error("output path is required");
  }
  const changed = changedPaths(base, head);
  const result = classify(changed.paths, changed.forceFull);
  writeOutputs(output, result);
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (require.main === module) {
  main();
}

module.exports = { classify, isCompilerFile, isDependencyFile, isProductFile, parseArguments };
