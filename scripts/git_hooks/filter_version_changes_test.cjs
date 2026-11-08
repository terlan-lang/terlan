#!/usr/bin/env node

"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  filterPaths,
  pairedPath,
  replaceAll,
  workspaceVersion,
} = require("./filter_version_changes.cjs");

test("workspace version is read only from workspace package", () => {
  const source = '[package]\nversion = "9.9.9"\n[workspace.package]\nversion = "0.0.10"\n';
  assert.equal(workspaceVersion(source), "0.0.10");
  assert.equal(workspaceVersion("[package]\nversion = \"0.0.10\"\n"), "");
});

test("exact substitutions are removed from semantic paths", () => {
  const values = new Map([
    ["base:Cargo.toml", '[workspace.package]\nversion = "0.10.0"\n'],
    ["tree:Cargo.toml", '[workspace.package]\nversion = "0.0.10"\n'],
    ["base:std/A.terl", "version 0.10.0\n"],
    ["tree:std/A.terl", "version 0.0.10\n"],
  ]);
  const reader = (revision, path) => values.get(`${revision}:${path}`) ?? null;
  assert.deepEqual(
    filterPaths(["Cargo.toml", "std/A.terl"], "base", "tree", "0.10.0", "0.0.10", reader),
    {
      paths: [".terlan-hook/version-metadata"],
      versionOnly: ["Cargo.toml", "std/A.terl"],
    },
  );
});

test("mixed changes retain their normal semantic path", () => {
  const reader = (revision, path) => {
    if (path !== "Makefile") return null;
    return revision === "base" ? "VERSION=0.10.0\nold\n" : "VERSION=0.0.10\nnew\n";
  };
  assert.deepEqual(
    filterPaths(["Makefile"], "base", "tree", "0.10.0", "0.0.10", reader),
    { paths: ["Makefile"], versionOnly: [] },
  );
});

test("version-bearing renamed files are paired", () => {
  const oldPath = "docs/RELEASE_0_10_0.md";
  const newPath = "docs/RELEASE_0_0_10.md";
  const reader = (revision, path) => {
    if (revision === "base" && path === oldPath) return "Terlan 0.10.0\n";
    if (revision === "tree" && path === newPath) return "Terlan 0.0.10\n";
    return null;
  };
  assert.deepEqual(
    filterPaths([oldPath, newPath], "base", "tree", "0.10.0", "0.0.10", reader),
    {
      paths: [".terlan-hook/version-metadata"],
      versionOnly: [newPath, oldPath].sort(),
    },
  );
});

test("unchanged or missing versions disable filtering", () => {
  assert.deepEqual(filterPaths(["Cargo.toml"], "a", "b", "0.0.10", "0.0.10"), {
    paths: ["Cargo.toml"],
    versionOnly: [],
  });
  assert.equal(pairedPath("release_0_10_0.md", "0_10_0", "0_0_10"), "release_0_0_10.md");
  assert.equal(replaceAll("0.10.0 0.10.0", "0.10.0", "0.0.10"), "0.0.10 0.0.10");
});
