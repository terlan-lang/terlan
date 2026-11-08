#!/usr/bin/env node

"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  classify,
  isCompilerFile,
  isDependencyFile,
  isProductFile,
  parseArguments,
} = require("./classify_changes.cjs");

test("workflow and documentation changes avoid compiler validation", () => {
  assert.deepEqual(classify([".github/workflows/ci.yml", "docs/release.md"]), {
    compiler: false,
    source: false,
    dependencies: false,
    changed_count: 2,
    changed_files: [".github/workflows/ci.yml", "docs/release.md"],
  });
});

test("compiler sources select compiler validation", () => {
  const result = classify(["crates/terlan/src/main.rs", "std/core/Int.terl"]);
  assert.equal(result.compiler, true);
  assert.equal(result.source, true);
  assert.equal(result.dependencies, false);
});

test("dependency manifests select both dependency and compiler validation", () => {
  const result = classify(["crates/terlan/Cargo.toml", "tools/css/package-lock.json"]);
  assert.equal(result.compiler, true);
  assert.equal(result.dependencies, true);
  assert.equal(isDependencyFile("scripts/peers/requirements.txt"), true);
});

test("manual and indeterminate dispatches force all validation", () => {
  assert.deepEqual(classify([], true), {
    compiler: true,
    source: true,
    dependencies: true,
    changed_count: 0,
    changed_files: [],
  });
});

test("classification normalizes duplicate and empty paths", () => {
  const result = classify(["Makefile", "", "Makefile", "editors/vscode/package.json"]);
  assert.equal(result.source, true);
  assert.equal(result.compiler, false);
  assert.equal(result.dependencies, false);
  assert.deepEqual(result.changed_files, ["Makefile", "editors/vscode/package.json"]);
});

test("root and prefix contracts are explicit", () => {
  assert.equal(isProductFile("Cargo.lock"), true);
  assert.equal(isCompilerFile("Cargo.lock"), true);
  assert.equal(isProductFile("tree-sitter-terlan/grammar.js"), true);
  assert.equal(isProductFile("docs/compiler.md"), false);
  assert.equal(isCompilerFile("scripts/release_control/Main.terl"), false);
  assert.equal(isDependencyFile("Cargo.lock"), true);
  assert.equal(isDependencyFile("README.md"), false);
});

test("release tooling receives source checks without compiler replay", () => {
  const result = classify(["Makefile", "scripts/release_control/Main.terl"]);
  assert.equal(result.source, true);
  assert.equal(result.compiler, false);
  assert.equal(result.dependencies, false);
});

test("argument parsing rejects malformed pairs", () => {
  assert.deepEqual(
    [...parseArguments(["--base", "a", "--head", "b"]).entries()],
    [["base", "a"], ["head", "b"]],
  );
  assert.throws(() => parseArguments(["base", "a"]), /usage/);
  assert.throws(() => parseArguments(["--base"]), /usage/);
});
