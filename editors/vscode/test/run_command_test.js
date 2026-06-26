"use strict";

const assert = require("assert");
const {
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
} = require("../src/run_command");

/**
 * Verifies POSIX shell quoting preserves spaces and quotes.
 *
 * @returns {void}
 *
 * @description
 * Exercises the quoting helper with a workspace path that would otherwise be
 * split by a shell.
 */
function testPosixShellQuote() {
  assert.strictEqual(shellQuote("/tmp/hello world", "linux"), "'/tmp/hello world'");
  assert.strictEqual(shellQuote("it's", "linux"), "'it'\\''s'");
}

/**
 * Verifies Windows shell quoting preserves spaces and quotes.
 *
 * @returns {void}
 *
 * @description
 * Exercises the Windows quoting branch used when the extension runs on win32.
 */
function testWindowsShellQuote() {
  assert.strictEqual(
    shellQuote("C:\\Terlan Apps\\demo", "win32"),
    "\"C:\\Terlan Apps\\demo\""
  );
}

/**
 * Verifies the run terminal command calls `terlc run`.
 *
 * @returns {void}
 *
 * @description
 * Confirms the VS Code run button delegates to the compiler's run command
 * instead of duplicating build or runtime behavior in the extension.
 */
function testBuildRunCommandLine() {
  assert.strictEqual(
    buildRunCommandLine("terlc", "/tmp/hello-terlan", "linux"),
    "'terlc' run '/tmp/hello-terlan'"
  );
}

/**
 * Verifies POSIX terminal commands refresh shell command hashes.
 *
 * @returns {void}
 *
 * @description
 * Locks the extension behavior that reused integrated terminals pick up a
 * newly installed `terlc` binary instead of a stale shell-hashed path.
 */
function testWithShellCommandCacheRefreshOnPosix() {
  assert.strictEqual(
    withShellCommandCacheRefresh("'terlc' test 'file.terl'", "linux"),
    "hash -r 2>/dev/null || true; 'terlc' test 'file.terl'"
  );
}

/**
 * Verifies Windows terminal commands are not prefixed with POSIX shell syntax.
 *
 * @returns {void}
 *
 * @description
 * Keeps command-cache refresh portable by leaving Windows terminal commands
 * unchanged.
 */
function testWithShellCommandCacheRefreshOnWindows() {
  assert.strictEqual(
    withShellCommandCacheRefresh("\"terlc\" test \"file.terl\"", "win32"),
    "\"terlc\" test \"file.terl\""
  );
}

/**
 * Verifies file-level test command construction.
 *
 * @returns {void}
 *
 * @description
 * Confirms the editor command delegates a whole test file to `terlc test`.
 */
function testBuildTestFileCommandLine() {
  assert.strictEqual(
    buildTestFileCommandLine("terlc", "/tmp/app/std/core/BoolTest.terl", "linux"),
    "'terlc' test '/tmp/app/std/core/BoolTest.terl'"
  );
}

/**
 * Verifies single-test command construction.
 *
 * @returns {void}
 *
 * @description
 * Confirms the editor command delegates individual test selection to
 * `terlc test --name`.
 */
function testBuildTestNameCommandLine() {
  assert.strictEqual(
    buildTestNameCommandLine(
      "terlc",
      "/tmp/app/std/core/BoolTest.terl",
      "to_string_renders_true",
      "linux"
    ),
    "'terlc' test '/tmp/app/std/core/BoolTest.terl' --name 'to_string_renders_true'"
  );
}

/**
 * Verifies Terlan test-file path detection.
 *
 * @returns {void}
 *
 * @description
 * Mirrors the compiler's `*Test.terl` file convention for editor command
 * enablement and warnings.
 */
function testIsTerlanTestFilePath() {
  assert.strictEqual(isTerlanTestFilePath("/tmp/BoolTest.terl"), true);
  assert.strictEqual(isTerlanTestFilePath("/tmp/bool_test.terl"), false);
  assert.strictEqual(isTerlanTestFilePath("/tmp/Bool.terl"), false);
}

/**
 * Verifies `@test` function range discovery.
 *
 * @returns {void}
 *
 * @description
 * Checks that the editor can map cursor positions to source-level test names
 * without needing a separate editor-only parser.
 */
function testDiscoverTestRanges() {
  const ranges = discoverTestRanges([
    "module sample.Test.",
    "",
    "@test",
    "pub first(): Bool ->",
    "    true.",
    "",
    "@test",
    "second(): Bool ->",
    "    true."
  ].join("\n"));

  assert.deepStrictEqual(
    ranges.map((range) => ({
      name: range.name,
      startLine: range.startLine,
      declarationLine: range.declarationLine,
      endLine: range.endLine
    })),
    [
      { name: "first", startLine: 2, declarationLine: 3, endLine: 5 },
      { name: "second", startLine: 6, declarationLine: 7, endLine: 8 }
    ]
  );
}

/**
 * Verifies cursor-to-test-name lookup.
 *
 * @returns {void}
 *
 * @description
 * Ensures the command can run the test containing the active cursor line.
 */
function testFindTestNameAtLine() {
  const source = [
    "@test",
    "pub first(): Bool ->",
    "    true.",
    "",
    "@test",
    "pub second(): Bool ->",
    "    true."
  ].join("\n");

  assert.strictEqual(findTestNameAtLine(source, 2), "first");
  assert.strictEqual(findTestNameAtLine(source, 5), "second");
  assert.strictEqual(findTestNameAtLine(source, 20), undefined);
}

/**
 * Verifies runnable declaration discovery for CodeLens icons.
 *
 * @returns {void}
 *
 * @description
 * Confirms editor CodeLens controls are generated for package entrypoints and
 * individual `@test` functions without introducing an editor-only compiler.
 */
function testDiscoverRunnableEntries() {
  const source = [
    "module app.Main.",
    "",
    "pub main(): Unit ->",
    "    Unit.",
    "",
    "@test",
    "pub first(): Bool ->",
    "    true.",
    "",
    "@test",
    "pub second(): Bool ->",
    "    true."
  ].join("\n");

  assert.deepStrictEqual(
    discoverRunnableEntries(source, true),
    [
      { kind: "main", name: undefined, line: 2 },
      { kind: "test", name: "first", line: 6 },
      { kind: "test", name: "second", line: 10 }
    ]
  );
  assert.deepStrictEqual(
    discoverRunnableEntries(source, false),
    [{ kind: "main", name: undefined, line: 2 }]
  );
}

/**
 * Verifies qualified module references can be discovered from one line.
 *
 * @returns {void}
 *
 * @description
 * Supports editor import quick fixes for source such as `Other.test()`.
 */
function testFindQualifiedModuleReferenceAtLine() {
  const source = [
    "module test.Main.",
    "",
    "pub main(): Unit ->",
    "    Other.test()."
  ].join("\n");

  assert.deepStrictEqual(
    findQualifiedModuleReferenceAtLine(source, 3),
    { name: "Other", start: 4, end: 9 }
  );
  assert.strictEqual(findQualifiedModuleReferenceAtLine(source, 1), undefined);
}

/**
 * Verifies uppercase module prefixes can be discovered at the cursor.
 *
 * @returns {void}
 *
 * @description
 * Supports auto-import completions for partially typed module references.
 */
function testFindModuleReferencePrefixAtPosition() {
  const source = [
    "module test.Main.",
    "",
    "pub main(): Unit ->",
    "    Vec"
  ].join("\n");

  assert.deepStrictEqual(
    findModuleReferencePrefixAtPosition(source, 3, 7),
    { name: "Vec", start: 4, end: 7 }
  );
  assert.strictEqual(findModuleReferencePrefixAtPosition(source, 3, 4), undefined);
}

/**
 * Verifies module declarations are extracted from Terlan source.
 *
 * @returns {void}
 *
 * @description
 * Allows the VS Code extension to build a lightweight workspace module index
 * for import quick fixes.
 */
function testParseModuleDeclaration() {
  assert.strictEqual(
    parseModuleDeclaration("module app.other.Other.\n\npub test(): Unit -> Unit.\n"),
    "app.other.Other"
  );
}

/**
 * Verifies module leaf names use the final dotted segment.
 *
 * @returns {void}
 *
 * @description
 * Supports matching `Other.test()` to a discovered `app.other.Other` module.
 */
function testModuleLeafName() {
  assert.strictEqual(moduleLeafName("app.other.Other"), "Other");
}

/**
 * Verifies import insertion follows module and import headers.
 *
 * @returns {void}
 *
 * @description
 * Keeps auto-import edits predictable and consistent with Terlan source layout.
 */
function testImportInsertionLine() {
  const source = [
    "module test.Main.",
    "",
    "import std.io.Console.{println}.",
    "import std.core.Bool.",
    "",
    "pub main(): Unit -> Unit."
  ].join("\n");

  assert.strictEqual(importInsertionLine(source), 4);
  assert.strictEqual(
    importInsertionLine(source, "import std.core.Atom."),
    2
  );
  assert.strictEqual(
    importInsertionLine(source, "import std.zzz.Late."),
    4
  );
  assert.strictEqual(
    importInsertionLine(source, "import app.other.Other."),
    2
  );
}

/**
 * Verifies existing imports are detected before adding quick-fix imports.
 *
 * @returns {void}
 *
 * @description
 * Prevents duplicate imports when a source file already imports the candidate
 * module.
 */
function testHasModuleImport() {
  const source = "module test.Main.\n\nimport app.other.Other.\n";

  assert.strictEqual(hasModuleImport(source, "app.other.Other"), true);
  assert.strictEqual(hasModuleImport(source, "app.other.Missing"), false);
}

/**
 * Verifies active-document workspace resolution wins over first workspace.
 *
 * @returns {void}
 *
 * @description
 * Uses a minimal workspace stub to ensure multi-root projects run the workspace
 * containing the active Terlan file.
 */
function testResolveRunWorkspaceFolderUsesActiveDocument() {
  const selected = { uri: { fsPath: "/workspace/app" } };
  const fallback = { uri: { fsPath: "/workspace/other" } };
  const workspace = {
    workspaceFolders: [fallback],
    getWorkspaceFolder(uri) {
      assert.strictEqual(uri.fsPath, "/workspace/app/src/app/Main.terl");
      return selected;
    }
  };
  const activeEditor = {
    document: {
      uri: {
        fsPath: "/workspace/app/src/app/Main.terl"
      }
    }
  };

  assert.strictEqual(resolveRunWorkspaceFolder(workspace, activeEditor), selected);
}

/**
 * Verifies workspace resolution falls back to the first open folder.
 *
 * @returns {void}
 *
 * @description
 * Keeps command behavior deterministic when the run command is invoked without
 * an active editor.
 */
function testResolveRunWorkspaceFolderFallsBackToFirstFolder() {
  const fallback = { uri: { fsPath: "/workspace/first" } };
  const workspace = {
    workspaceFolders: [fallback],
    getWorkspaceFolder() {
      return undefined;
    }
  };

  assert.strictEqual(resolveRunWorkspaceFolder(workspace, undefined), fallback);
}

testPosixShellQuote();
testWindowsShellQuote();
testBuildRunCommandLine();
testWithShellCommandCacheRefreshOnPosix();
testWithShellCommandCacheRefreshOnWindows();
testBuildTestFileCommandLine();
testBuildTestNameCommandLine();
testIsTerlanTestFilePath();
testDiscoverTestRanges();
testFindTestNameAtLine();
testDiscoverRunnableEntries();
testFindQualifiedModuleReferenceAtLine();
testFindModuleReferencePrefixAtPosition();
testParseModuleDeclaration();
testModuleLeafName();
testImportInsertionLine();
testHasModuleImport();
testResolveRunWorkspaceFolderUsesActiveDocument();
testResolveRunWorkspaceFolderFallsBackToFirstFolder();

console.log("terlan vscode run command tests passed");
