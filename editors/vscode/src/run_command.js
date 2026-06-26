"use strict";

/**
 * Quotes one terminal argument for the active platform shell.
 *
 * @param {string} value Argument value to quote.
 * @param {string} platform Node platform identifier such as `linux` or `win32`.
 * @returns {string} Shell-safe argument text.
 *
 * @description
 * Converts command and path values into terminal text without changing their
 * semantic value so the VS Code run command can safely pass project paths with
 * spaces to `terlc run`.
 */
function shellQuote(value, platform = process.platform) {
  if (platform === "win32") {
    return `"${String(value).replace(/"/g, '\\"')}"`;
  }
  return `'${String(value).replace(/'/g, "'\\''")}'`;
}

/**
 * Builds the terminal command used by the Terlan run button.
 *
 * @param {string} command Configured compiler command.
 * @param {string} projectPath Workspace/project path to run.
 * @param {string} platform Node platform identifier used for shell quoting.
 * @returns {string} Terminal command line.
 *
 * @description
 * Produces the exact `terlc run <project>` command sent to VS Code terminals
 * while keeping extension activation free of command-line string assembly.
 */
function buildRunCommandLine(command, projectPath, platform = process.platform) {
  return `${shellQuote(command, platform)} run ${shellQuote(projectPath, platform)}`;
}

/**
 * Prefixes terminal commands with shell command-cache refresh when supported.
 *
 * @param {string} commandLine Shell command line to run.
 * @param {string} platform Node platform identifier used for shell selection.
 * @returns {string} Terminal command line with command cache refresh prefix.
 *
 * @description
 * Reused integrated terminals may keep an old `terlc` path in the shell hash
 * table after a local compiler reinstall. On POSIX shells, `hash -r` clears
 * that cache before the Terlan command runs. Windows shells do not use this
 * command, so the input is returned unchanged there.
 */
function withShellCommandCacheRefresh(commandLine, platform = process.platform) {
  if (platform === "win32") {
    return commandLine;
  }
  return `hash -r 2>/dev/null || true; ${commandLine}`;
}

/**
 * Builds the terminal command used to run every test in one Terlan file.
 *
 * @param {string} command Configured compiler command.
 * @param {string} filePath Terlan `*Test.terl` file path.
 * @param {string} platform Node platform identifier used for shell quoting.
 * @returns {string} Terminal command line.
 *
 * @description
 * Produces the exact `terlc test <file>` command sent to VS Code terminals so
 * file-level test execution stays compiler-owned.
 */
function buildTestFileCommandLine(command, filePath, platform = process.platform) {
  return `${shellQuote(command, platform)} test ${shellQuote(filePath, platform)}`;
}

/**
 * Builds the terminal command used to run one named Terlan test.
 *
 * @param {string} command Configured compiler command.
 * @param {string} filePath Terlan `*Test.terl` file path.
 * @param {string} testName Exact `@test` function name.
 * @param {string} platform Node platform identifier used for shell quoting.
 * @returns {string} Terminal command line.
 *
 * @description
 * Produces the exact `terlc test <file> --name <test>` command sent to VS Code
 * terminals so individual test execution uses the compiler selector.
 */
function buildTestNameCommandLine(command, filePath, testName, platform = process.platform) {
  return `${buildTestFileCommandLine(command, filePath, platform)} --name ${shellQuote(testName, platform)}`;
}

/**
 * Selects the workspace folder that owns the active document.
 *
 * @param {object} workspace VS Code workspace API surface.
 * @param {object | undefined} activeEditor Active text editor, when present.
 * @returns {object | undefined} Workspace folder selected for `terlc run`.
 *
 * @description
 * Prefers the active document's containing workspace folder and falls back to
 * the first open workspace so the run button works in single-root projects and
 * remains deterministic in multi-root workspaces.
 */
function resolveRunWorkspaceFolder(workspace, activeEditor) {
  if (activeEditor && activeEditor.document && activeEditor.document.uri) {
    const folder = workspace.getWorkspaceFolder(activeEditor.document.uri);
    if (folder) {
      return folder;
    }
  }
  const folders = workspace.workspaceFolders || [];
  return folders[0];
}

/**
 * Returns whether a path is a Terlan test source file.
 *
 * @param {string} filePath File path from the active editor.
 * @returns {boolean} True for paths ending in `Test.terl`.
 *
 * @description
 * Mirrors the compiler's test-file layout check on the editor side only to
 * decide whether test commands should run against the active file.
 */
function isTerlanTestFilePath(filePath) {
  const fileName = String(filePath).split(/[\\/]/).pop() || "";
  return fileName.endsWith("Test.terl");
}

/**
 * Discovers source ranges for `@test` functions in one document.
 *
 * @param {string} text Terlan source text.
 * @returns {{name: string, startLine: number, declarationLine: number, endLine: number}[]} Test ranges.
 *
 * @description
 * Scans line-oriented Terlan source for `@test` annotations followed by a
 * zero-argument or regular function declaration and expands each discovered
 * test until the next `@test` annotation or the end of the file.
 */
function discoverTestRanges(text) {
  const lines = String(text).split(/\r?\n/);
  const tests = [];
  let pendingAnnotationLine = undefined;
  const functionPattern = /^\s*(?:pub\s+)?([a-z][A-Za-z0-9_]*)\s*\(/;

  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim();
    if (trimmed.startsWith("@test")) {
      pendingAnnotationLine = index;
      continue;
    }
    if (pendingAnnotationLine !== undefined) {
      const match = lines[index].match(functionPattern);
      if (match) {
        tests.push({
          name: match[1],
          startLine: pendingAnnotationLine,
          declarationLine: index,
          endLine: lines.length - 1
        });
        pendingAnnotationLine = undefined;
      }
    }
  }

  for (let index = 0; index < tests.length - 1; index += 1) {
    tests[index].endLine = tests[index + 1].startLine - 1;
  }
  return tests;
}

/**
 * Finds the `@test` function that contains one document line.
 *
 * @param {string} text Terlan source text.
 * @param {number} line Zero-based document line.
 * @returns {string | undefined} Exact test function name, when found.
 *
 * @description
 * Uses discovered `@test` ranges to map the active cursor line to the compiler
 * test selector consumed by `terlc test --name`.
 */
function findTestNameAtLine(text, line) {
  const ranges = discoverTestRanges(text);
  const range = ranges.find((test) => line >= test.startLine && line <= test.endLine);
  return range ? range.name : undefined;
}

/**
 * Discovers runnable Terlan declarations for editor CodeLens controls.
 *
 * @param {string} text Terlan source text.
 * @param {boolean} isTestFile Whether the document path follows `*Test.terl`.
 * @returns {{kind: string, name: string | undefined, line: number}[]} Runnable entries.
 *
 * @description
 * Finds a package entrypoint declaration and, for test files, all `@test`
 * declarations so the extension can render clickable run actions without
 * maintaining a second compiler or test runner.
 */
function discoverRunnableEntries(text, isTestFile) {
  const lines = String(text).split(/\r?\n/);
  const entries = [];
  const mainPattern = /^\s*pub\s+main\s*\(\s*\)\s*:/;

  for (let index = 0; index < lines.length; index += 1) {
    if (mainPattern.test(lines[index])) {
      entries.push({ kind: "main", name: undefined, line: index });
    }
  }

  if (isTestFile) {
    for (const test of discoverTestRanges(text)) {
      entries.push({ kind: "test", name: test.name, line: test.declarationLine });
    }
  }

  return entries;
}

/**
 * Finds an uppercase qualified module reference on one source line.
 *
 * @param {string} text Terlan source text.
 * @param {number} line Zero-based document line.
 * @returns {{name: string, start: number, end: number} | undefined} Module reference.
 *
 * @description
 * Detects call heads such as `Other.test()` without parsing the whole module
 * so editor quick fixes can offer imports for missing module references.
 */
function findQualifiedModuleReferenceAtLine(text, line) {
  const lines = String(text).split(/\r?\n/);
  const sourceLine = lines[line];
  if (sourceLine === undefined) {
    return undefined;
  }
  const pattern = /\b([A-Z][A-Za-z0-9_]*)\s*\./g;
  const match = pattern.exec(sourceLine);
  if (!match) {
    return undefined;
  }
  return {
    name: match[1],
    start: match.index,
    end: match.index + match[1].length
  };
}

/**
 * Finds an uppercase module-name prefix immediately before a cursor position.
 *
 * @param {string} text Terlan source text.
 * @param {number} line Zero-based document line.
 * @param {number} character Zero-based UTF-16 character offset on the line.
 * @returns {{name: string, start: number, end: number} | undefined} Prefix range.
 *
 * @description
 * Supports auto-import completions by identifying partially typed module
 * references such as `Vec` before the user accepts a completion for `Vector`.
 */
function findModuleReferencePrefixAtPosition(text, line, character) {
  const lines = String(text).split(/\r?\n/);
  const sourceLine = lines[line];
  if (sourceLine === undefined) {
    return undefined;
  }
  const prefixText = sourceLine.slice(0, Math.max(0, character));
  const match = prefixText.match(/\b([A-Z][A-Za-z0-9_]*)$/);
  if (!match) {
    return undefined;
  }
  return {
    name: match[1],
    start: prefixText.length - match[1].length,
    end: prefixText.length
  };
}

/**
 * Extracts a Terlan module declaration from source text.
 *
 * @param {string} text Terlan source text.
 * @returns {string | undefined} Declared module name.
 *
 * @description
 * Reads the canonical `module name.` header used by workspace import
 * discovery without invoking the compiler.
 */
function parseModuleDeclaration(text) {
  const match = String(text).match(/^\s*module\s+([A-Za-z_][A-Za-z0-9_.]*)\s*\./m);
  return match ? match[1] : undefined;
}

/**
 * Returns the final segment of a dotted Terlan module name.
 *
 * @param {string} moduleName Full module name.
 * @returns {string} Final module segment.
 *
 * @description
 * Supports default-export style module lookup where `app.other.Other` can
 * satisfy a source reference to `Other`.
 */
function moduleLeafName(moduleName) {
  const parts = String(moduleName).split(".");
  return parts[parts.length - 1] || "";
}

/**
 * Computes the line where a new import should be inserted.
 *
 * @param {string} text Terlan source text.
 * @param {string | undefined} importText Import declaration being inserted.
 * @returns {number} Zero-based insertion line.
 *
 * @description
 * Places new imports in alphabetic order inside the module/import header block.
 * When no import text is supplied, returns the end of the existing import block.
 */
function importInsertionLine(text, importText = undefined) {
  const lines = String(text).split(/\r?\n/);
  let insertionLine = 0;
  const normalizedImport = importText && String(importText).trim();
  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim();
    if (
      normalizedImport &&
      trimmed.startsWith("import ") &&
      trimmed.localeCompare(normalizedImport) > 0
    ) {
      return index;
    }
    if (trimmed.startsWith("module ") || trimmed.startsWith("import ")) {
      insertionLine = index + 1;
    }
  }
  return insertionLine;
}

/**
 * Returns whether a source file already imports a module.
 *
 * @param {string} text Terlan source text.
 * @param {string} moduleName Full module name.
 * @returns {boolean} True when an import for the module already exists.
 *
 * @description
 * Prevents duplicate editor quick-fix edits for modules that are already
 * imported in the active source file.
 */
function hasModuleImport(text, moduleName) {
  const escaped = String(moduleName).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^\\s*import\\s+${escaped}(?:\\.|\\s|\\{|$)`, "m").test(String(text));
}

module.exports = {
  buildTestFileCommandLine,
  buildTestNameCommandLine,
  buildRunCommandLine,
  discoverRunnableEntries,
  discoverTestRanges,
  findQualifiedModuleReferenceAtLine,
  findModuleReferencePrefixAtPosition,
  findTestNameAtLine,
  hasModuleImport,
  importInsertionLine,
  isTerlanTestFilePath,
  moduleLeafName,
  parseModuleDeclaration,
  resolveRunWorkspaceFolder,
  shellQuote,
  withShellCommandCacheRefresh
};
