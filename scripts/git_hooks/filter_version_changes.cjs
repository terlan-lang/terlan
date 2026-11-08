#!/usr/bin/env node

"use strict";

const { execFileSync } = require("node:child_process");
const fs = require("node:fs");

function workspaceVersion(source) {
  const match = source.match(/\[workspace\.package\][\s\S]*?^version = "([0-9]+\.[0-9]+\.[0-9]+)"$/m);
  return match?.[1] || "";
}

function replaceAll(source, before, after) {
  return source.split(before).join(after);
}

function containsOnlySelectedReplacements(source, candidate, before, after) {
  let sourceOffset = 0;
  let candidateOffset = 0;
  let replacements = 0;
  while (sourceOffset < source.length && candidateOffset < candidate.length) {
    if (source.startsWith(before, sourceOffset)
      && candidate.startsWith(after, candidateOffset)) {
      sourceOffset += before.length;
      candidateOffset += after.length;
      replacements += 1;
      continue;
    }
    if (source[sourceOffset] === candidate[candidateOffset]) {
      sourceOffset += 1;
      candidateOffset += 1;
      continue;
    }
    return false;
  }
  return replacements > 0
    && sourceOffset === source.length
    && candidateOffset === candidate.length;
}

function blob(revision, path) {
  try {
    return execFileSync("git", ["show", `${revision}:${path}`], {
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      stdio: ["ignore", "pipe", "ignore"],
    });
  } catch (_error) {
    return null;
  }
}

function pairedPath(path, before, after) {
  return path.includes(before) ? replaceAll(path, before, after) : "";
}

function filterPaths(paths, base, tree, before, after, reader = blob) {
  if (!before || !after || before === after) {
    return { paths, versionOnly: [] };
  }
  const pathSet = new Set(paths);
  const beforeFile = before.replaceAll(".", "_");
  const afterFile = after.replaceAll(".", "_");
  const versionOnly = new Set();

  for (const path of paths) {
    const oldSource = reader(base, path);
    const newSource = reader(tree, path);
    if (oldSource !== null && newSource !== null
      && containsOnlySelectedReplacements(oldSource, newSource, before, after)) {
      versionOnly.add(path);
      continue;
    }

    const newPath = pairedPath(path, beforeFile, afterFile);
    if (oldSource !== null && newSource === null && newPath && pathSet.has(newPath)) {
      const renamedSource = reader(tree, newPath);
      if (renamedSource !== null
        && containsOnlySelectedReplacements(oldSource, renamedSource, before, after)) {
        versionOnly.add(path);
        versionOnly.add(newPath);
      }
    }
  }

  const kept = paths.filter((path) => !versionOnly.has(path));
  if (versionOnly.size > 0) {
    kept.push(".terlan-hook/version-metadata");
  }
  return { paths: kept, versionOnly: [...versionOnly].sort() };
}

function main() {
  const [base, tree, input, output] = process.argv.slice(2);
  if (!base || !tree || !input || !output) {
    throw new Error("usage: filter_version_changes.cjs BASE TREE INPUT OUTPUT");
  }
  const baseCargo = blob(base, "Cargo.toml");
  const treeCargo = blob(tree, "Cargo.toml");
  const before = baseCargo === null ? "" : workspaceVersion(baseCargo);
  const after = treeCargo === null ? "" : workspaceVersion(treeCargo);
  const paths = fs.readFileSync(input, "utf8").split("\n").filter(Boolean);
  const result = filterPaths(paths, base, tree, before, after);
  fs.writeFileSync(output, result.paths.map((path) => `${path}\n`).join(""), "utf8");
  process.stdout.write(
    `pre-commit: ${result.versionOnly.length} exact version metadata path(s), ${result.paths.length} semantic path(s)\n`,
  );
}

if (require.main === module) {
  main();
}

module.exports = {
  containsOnlySelectedReplacements,
  filterPaths,
  pairedPath,
  replaceAll,
  workspaceVersion,
};
