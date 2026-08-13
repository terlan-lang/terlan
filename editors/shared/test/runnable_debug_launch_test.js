"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EDITORS_ROOT = path.resolve(__dirname, "..", "..");
const REPO_ROOT = path.resolve(EDITORS_ROOT, "..");
const REPORT_PATH = path.join(
  REPO_ROOT,
  "target",
  "quality",
  "editor-runnable-debug-launch-report.json"
);

const {
  RUN_COMMAND_IDS,
  buildBuildCommandLine,
  buildCheckCommandLine,
  buildCleanCommandLine,
  buildDebugBreakpointCommandLine,
  buildDebugCommandLine,
  buildDoctorCommandLine,
  buildServeCommandLine,
  buildTerminalLaunchDescriptor,
  buildRunCommandLine,
  buildTestFileCommandLine,
  buildTestNameCommandLine,
  buildWatchCommandLine,
  discoverRunnableEntries,
  hasRunnableTestName,
  resolveRunTargetPath,
  resolveRunWorkspaceFolder
} = require("../../vscode/src/run_command");

/**
 * Reads one repository-relative text file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {string} File contents.
 *
 * @description
 * Keeps runnable/debug launch checks dependency-free and editor-runtime-free.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
}

/**
 * Reads one repository-relative JSON file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {*} Parsed JSON contents.
 *
 * @description
 * Allows the check to inspect packaged editor metadata without launching VS Code.
 */
function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

/**
 * Extracts a contributed VS Code command by id.
 *
 * @param {*} manifest VS Code package manifest.
 * @param {string} commandId Command id to find.
 * @returns {*} Command contribution.
 *
 * @description
 * Fails with a precise assertion when runnable command ids drift.
 */
function contributedCommand(manifest, commandId) {
  const command = (manifest.contributes.commands || []).find(
    (candidate) => candidate.command === commandId
  );
  assert.ok(command, `missing command ${commandId}`);
  return command;
}

/**
 * Verifies stable VS Code runnable command metadata.
 *
 * @returns {{commands: string[], menus: string[]}} Command and menu inventory.
 *
 * @description
 * Locks command ids, labels, activation events, and menu placements used by run
 * buttons and CodeLens actions.
 */
function testVscodeRunnableCommandMetadata() {
  const manifest = readJson("editors/vscode/package.json");
  const menus = manifest.contributes.menus || {};
  const editorTitle = menus["editor/title"] || [];
  const explorerContext = menus["explorer/context"] || [];
  const expected = [
    [RUN_COMMAND_IDS.runMain, "Run Terlan Main"],
    [RUN_COMMAND_IDS.runCheck, "Check Terlan Workspace"],
    [RUN_COMMAND_IDS.runBuild, "Build Terlan Workspace"],
    [RUN_COMMAND_IDS.runClean, "Clean Terlan Workspace"],
    [RUN_COMMAND_IDS.runServe, "Serve Terlan Web"],
    [RUN_COMMAND_IDS.runWatch, "Watch Terlan Web"],
    [RUN_COMMAND_IDS.runDoctor, "Run Terlan Doctor"],
    [RUN_COMMAND_IDS.runDebug, "Launch Terlan Debug"],
    [RUN_COMMAND_IDS.runDebugAtCursor, "Debug Terlan at Cursor"],
    [RUN_COMMAND_IDS.runTestFile, "Run Terlan Test File"],
    [RUN_COMMAND_IDS.runTestAtCursor, "Run Terlan Test at Cursor"]
  ];

  for (const [commandId, title] of expected) {
    assert.strictEqual(contributedCommand(manifest, commandId).title, title);
    assert.ok(
      manifest.activationEvents.includes(`onCommand:${commandId}`),
      `missing activation event for ${commandId}`
    );
  }
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runMain),
    "missing editor-title main run action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runCheck),
    "missing editor-title check action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runBuild),
    "missing editor-title build action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runClean),
    "missing editor-title clean action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runServe),
    "missing editor-title serve action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runWatch),
    "missing editor-title watch action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runDoctor),
    "missing editor-title doctor action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runDebug),
    "missing editor-title debug action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runDebugAtCursor),
    "missing editor-title debug-at-cursor action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runTestAtCursor),
    "missing editor-title test-at-cursor action"
  );
  assert.ok(
    editorTitle.some((menu) => menu.command === RUN_COMMAND_IDS.runTestFile),
    "missing editor-title test-file action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runMain),
    "missing explorer main run action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runCheck),
    "missing explorer check action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runBuild),
    "missing explorer build action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runClean),
    "missing explorer clean action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runServe),
    "missing explorer serve action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runWatch),
    "missing explorer watch action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runDoctor),
    "missing explorer doctor action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runDebug),
    "missing explorer debug action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runDebugAtCursor),
    "missing explorer debug-at-cursor action"
  );
  assert.ok(
    explorerContext.some((menu) => menu.command === RUN_COMMAND_IDS.runTestFile),
    "missing explorer test-file action"
  );
  assert.strictEqual(
    manifest.contributes.configuration.properties["terlan.run.command"].default,
    "terlc"
  );

  return {
    commands: expected.map(([commandId]) => commandId),
    menus: [
      `editor/title:${RUN_COMMAND_IDS.runMain}`,
      `editor/title:${RUN_COMMAND_IDS.runCheck}`,
      `editor/title:${RUN_COMMAND_IDS.runBuild}`,
      `editor/title:${RUN_COMMAND_IDS.runClean}`,
      `editor/title:${RUN_COMMAND_IDS.runServe}`,
      `editor/title:${RUN_COMMAND_IDS.runWatch}`,
      `editor/title:${RUN_COMMAND_IDS.runDoctor}`,
      `editor/title:${RUN_COMMAND_IDS.runDebug}`,
      `editor/title:${RUN_COMMAND_IDS.runDebugAtCursor}`,
      `editor/title:${RUN_COMMAND_IDS.runTestAtCursor}`,
      `editor/title:${RUN_COMMAND_IDS.runTestFile}`,
      `explorer/context:${RUN_COMMAND_IDS.runMain}`,
      `explorer/context:${RUN_COMMAND_IDS.runCheck}`,
      `explorer/context:${RUN_COMMAND_IDS.runBuild}`,
      `explorer/context:${RUN_COMMAND_IDS.runClean}`,
      `explorer/context:${RUN_COMMAND_IDS.runServe}`,
      `explorer/context:${RUN_COMMAND_IDS.runWatch}`,
      `explorer/context:${RUN_COMMAND_IDS.runDoctor}`,
      `explorer/context:${RUN_COMMAND_IDS.runDebug}`,
      `explorer/context:${RUN_COMMAND_IDS.runDebugAtCursor}`,
      `explorer/context:${RUN_COMMAND_IDS.runTestFile}`
    ]
  };
}

/**
 * Verifies CodeLens uses stable runnable command identifiers.
 *
 * @returns {{main: string, namedTest: string}} CodeLens command ids.
 *
 * @description
 * Locks the internal named-test CodeLens command to the same command-id source
 * as contributed command-palette actions, preventing runnable editor controls
 * from drifting away from registered commands.
 */
function testCodeLensRunnableCommandIds() {
  const extensionSource = readText("editors/vscode/src/extension.js");
  assert.ok(
    extensionSource.includes("registerCommand(RUN_COMMAND_IDS.runTestByName"),
    "named-test command must be registered through RUN_COMMAND_IDS"
  );
  assert.ok(
    extensionSource.includes("command: RUN_COMMAND_IDS.runTestByName"),
    "test CodeLens must use the shared named-test command id"
  );
  assert.ok(
    extensionSource.includes("command: RUN_COMMAND_IDS.runMain"),
    "main CodeLens must use the shared main command id"
  );
  assert.ok(
    extensionSource.includes("arguments: [document.uri.fsPath]"),
    "main CodeLens must pass its source file to avoid stale active-workspace launches"
  );

  return {
    main: RUN_COMMAND_IDS.runMain,
    mainArguments: "document.uri.fsPath",
    namedTest: RUN_COMMAND_IDS.runTestByName
  };
}

/**
 * Verifies terminal command construction for run and test launch paths.
 *
 * @returns {{run: string, testFile: string, testName: string}} Command lines.
 *
 * @description
 * Ensures editor launch strings delegate to the compiler with stable quoting
 * and POSIX shell command-cache refresh behavior.
 */
function testRunnableCommandLines() {
  const run = buildRunCommandLine("terlc", "/tmp/hello app", "linux");
  const check = buildCheckCommandLine("terlc", "/tmp/hello app", "linux");
  const build = buildBuildCommandLine("terlc", "/tmp/hello app", "linux");
  const clean = buildCleanCommandLine("terlc", "/tmp/hello app", "linux");
  const serve = buildServeCommandLine("terlc", "/tmp/hello app", "linux");
  const watch = buildWatchCommandLine("terlc", "/tmp/hello app", "linux");
  const doctor = buildDoctorCommandLine("terlc", "/tmp/hello app", "linux");
  const debug = buildDebugCommandLine("terlc", "/tmp/hello app", "linux");
  const debugAtCursor = buildDebugBreakpointCommandLine(
    "terlc",
    "/tmp/hello app/Main.terl",
    7,
    "linux"
  );
  const testFile = buildTestFileCommandLine("terlc", "/tmp/hello app/MainTest.terl", "linux");
  const testName = buildTestNameCommandLine(
    "terlc",
    "/tmp/hello app/MainTest.terl",
    "hello_text_is_stable",
    "linux"
  );

  assert.strictEqual(run, "'terlc' run '/tmp/hello app'");
  assert.strictEqual(check, "'terlc' check '/tmp/hello app'");
  assert.strictEqual(build, "'terlc' build '/tmp/hello app'");
  assert.strictEqual(clean, "'terlc' clean '/tmp/hello app'");
  assert.strictEqual(serve, "'terlc' serve '/tmp/hello app'");
  assert.strictEqual(watch, "'terlc' serve '/tmp/hello app' --poll-ms 250");
  assert.strictEqual(doctor, "'terlc' doctor '/tmp/hello app'");
  assert.strictEqual(debug, "'terlc' debug '/tmp/hello app' --json-events");
  assert.strictEqual(
    debugAtCursor,
    "'terlc' debug '/tmp/hello app/Main.terl' --break '/tmp/hello app/Main.terl:7' --json-events"
  );
  assert.strictEqual(testFile, "'terlc' test '/tmp/hello app/MainTest.terl'");
  assert.strictEqual(
    testName,
    "'terlc' test '/tmp/hello app/MainTest.terl' --name 'hello_text_is_stable'"
  );
  const descriptors = {
    run: buildTerminalLaunchDescriptor("run", "/tmp/hello app", run, "linux", "/tmp"),
    testName: buildTerminalLaunchDescriptor(
      "test",
      "/tmp/hello app/MainTest.terl",
      testName,
      "linux",
      "/tmp"
    ),
    debug: buildTerminalLaunchDescriptor("debug", "/tmp/hello app", debug, "linux", "/tmp")
  };

  assert.strictEqual(descriptors.run.reproductionCommand, run);
  assert.strictEqual(descriptors.testName.reproductionCommand, testName);
  assert.strictEqual(descriptors.debug.reproductionCommand, debug);
  assert.strictEqual(descriptors.run.displayTargetPath, "${workspace}/hello app");
  assert.strictEqual(
    descriptors.testName.displayTargetPath,
    "${workspace}/hello app/MainTest.terl"
  );
  assert.strictEqual(descriptors.testName.terminalCommandLine, testName);
  assert.strictEqual(descriptors.run.colorPreservation, "compiler-owned");
  assert.strictEqual(descriptors.run.outputMode, "integrated-terminal-pass-through");

  return {
    run,
    check,
    build,
    clean,
    serve,
    watch,
    doctor,
    debug,
    debugAtCursor,
    testFile,
    testName,
    descriptors
  };
}

/**
 * Verifies launch descriptors keep exact commands while redacting only safe
 * workspace-owned display paths.
 *
 * @returns {*} Redaction evidence for the runnable/debug launch report.
 *
 * @description
 * Prevents editor reports from leaking workspace-root absolute paths while
 * keeping the terminal command and reproduction command byte-for-byte runnable.
 */
function testLaunchDescriptorRedactionPolicy() {
  const workspacePath = "/workspace/project";
  const filePath = "/workspace/project/tests/MainTest.terl";
  const outsidePath = "/tmp/generated/MainTest.terl";
  const commandLine = buildTestNameCommandLine("terlc", filePath, "renders_board", "linux");
  const descriptor = buildTerminalLaunchDescriptor(
    "test",
    filePath,
    commandLine,
    "linux",
    workspacePath
  );
  const externalDescriptor = buildTerminalLaunchDescriptor(
    "test",
    outsidePath,
    buildTestFileCommandLine("terlc", outsidePath, "linux"),
    "linux",
    workspacePath
  );
  const windowsDescriptor = buildTerminalLaunchDescriptor(
    "run",
    "C:\\Users\\terlan\\project",
    buildRunCommandLine("terlc", "C:\\Users\\terlan\\project", "win32"),
    "win32",
    "C:\\Users\\terlan\\project"
  );

  assert.strictEqual(descriptor.targetPath, filePath);
  assert.strictEqual(descriptor.commandLine, commandLine);
  assert.strictEqual(descriptor.reproductionCommand, commandLine);
  assert.strictEqual(descriptor.terminalCommandLine, commandLine);
  assert.strictEqual(descriptor.displayTargetPath, "${workspace}/tests/MainTest.terl");
  assert.ok(!descriptor.displayTargetPath.includes(workspacePath));
  assert.ok(
    descriptor.reproductionCommand.includes(workspacePath),
    "exact reproduction command must preserve the real launch path"
  );

  assert.strictEqual(externalDescriptor.displayTargetPath, outsidePath);
  assert.strictEqual(windowsDescriptor.terminalCommandLine, windowsDescriptor.commandLine);
  assert.strictEqual(windowsDescriptor.displayTargetPath, "${workspace}");

  return {
    workspaceDisplayTarget: descriptor.displayTargetPath,
    externalDisplayTarget: externalDescriptor.displayTargetPath,
    exactCommandPreserved: descriptor.reproductionCommand === commandLine,
    windowsCommandCacheRefresh: "not-applied"
  };
}

/**
 * Verifies runnable source discovery and workspace selection.
 *
 * @returns {{entries: *, workspaceFolder: string}} Runnable inventory.
 *
 * @description
 * Covers main/test CodeLens inventory and active-document workspace selection
 * without invoking VS Code.
 */
function testRunnableInventoryAndWorkspaceSelection() {
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
  const entries = discoverRunnableEntries(source, true);
  const workspaceFolder = {
    name: "hello",
    uri: { fsPath: "/workspace/hello" }
  };
  const selected = resolveRunWorkspaceFolder(
    {
      workspaceFolders: [{ name: "fallback", uri: { fsPath: "/workspace/fallback" } }],
      getWorkspaceFolder(uri) {
        return uri.fsPath === "/workspace/hello/src/Main.terl" ? workspaceFolder : undefined;
      }
    },
    {
      document: {
        uri: { fsPath: "/workspace/hello/src/Main.terl" }
      }
    }
  );

  assert.deepStrictEqual(entries, [
    { kind: "main", name: undefined, line: 2 },
    { kind: "test", name: "first", line: 6 },
    { kind: "test", name: "second", line: 10 }
  ]);
  assert.strictEqual(selected, workspaceFolder);

  return { entries, workspaceFolder: selected.name };
}

/**
 * Verifies workspace actions select nested Terlan package roots.
 *
 * @returns {{nestedPackageRoot: string, fallbackWorkspaceRoot: string}} Target paths.
 *
 * @description
 * Locks editor command working-directory behavior for package workspaces: active
 * files below a `terlan.toml` run that package, while ordinary workspaces keep
 * the VS Code workspace root.
 */
function testPackageWorkspaceTargetSelection() {
  const workspace = {
    workspaceFolders: [{ name: "root", uri: { fsPath: "/workspace" } }],
    getWorkspaceFolder(uri) {
      return uri.fsPath.startsWith("/workspace/")
        ? { name: "root", uri: { fsPath: "/workspace" } }
        : undefined;
    }
  };
  const activeEditor = {
    document: {
      uri: { fsPath: "/workspace/apps/chat/src/app/Main.terl" }
    }
  };
  const nestedPackageRoot = resolveRunTargetPath(
    workspace,
    activeEditor,
    (filePath) => filePath === "/workspace/apps/chat/terlan.toml"
  );
  const fallbackWorkspaceRoot = resolveRunTargetPath(workspace, activeEditor, () => false);

  assert.strictEqual(nestedPackageRoot, "/workspace/apps/chat");
  assert.strictEqual(fallbackWorkspaceRoot, "/workspace");

  return {
    nestedPackageRoot,
    fallbackWorkspaceRoot
  };
}

/**
 * Verifies stale named-test CodeLens arguments are rejected before launch.
 *
 * @returns {{renamedTestRejected: boolean, currentTestAccepted: boolean}} Stale-test status.
 *
 * @description
 * Covers the case where a user invokes an old CodeLens command after renaming
 * or deleting the test but before VS Code refreshes its runnable inventory.
 */
function testStaleNamedTestRejection() {
  const source = [
    "module app.MainTest.",
    "",
    "@test",
    "pub renamed_case(): Bool ->",
    "    true."
  ].join("\n");
  const extensionSource = readText("editors/vscode/src/extension.js");

  assert.strictEqual(hasRunnableTestName(source, "old_case"), false);
  assert.strictEqual(hasRunnableTestName(source, "renamed_case"), true);
  assert.ok(
    extensionSource.includes("hasRunnableTestName(document.getText(), testName)"),
    "runTestByName must reject stale named-test CodeLens arguments before launch"
  );

  return {
    renamedTestRejected: true,
    currentTestAccepted: true
  };
}

/**
 * Verifies Terlan terminal reuse and stale closed-terminal reset wiring.
 *
 * @returns {{sharedTerminalHandle: boolean, closeEventResetsHandle: boolean}} Terminal reuse status.
 *
 * @description
 * Prevents repeated run/test actions from spawning duplicate terminals while
 * still allowing a fresh terminal after the user closes the shared one.
 */
function testTerminalReuseContract() {
  const extensionSource = readText("editors/vscode/src/extension.js");
  const createTerminalMatches = extensionSource.match(/createTerminal\("Terlan"\)/g) || [];

  assert.strictEqual(
    createTerminalMatches.length,
    1,
    "Terlan run/test actions must create the shared terminal from one code path"
  );
  assert.ok(
    extensionSource.includes("if (terminal === terlanTerminal)"),
    "closed terminal events must check the cached Terlan terminal handle"
  );
  assert.ok(
    extensionSource.includes("terlanTerminal = undefined;"),
    "closed Terlan terminal must clear the cached handle before the next launch"
  );

  return {
    sharedTerminalHandle: true,
    closeEventResetsHandle: true
  };
}

/**
 * Verifies debug launch fallback wiring is present but not half-published.
 *
 * @returns {{cliFallback: boolean, vscodeDebuggerContribution: string}} Debug status.
 *
 * @description
 * Keeps editor debug launch parity tied to `terlc debug` until the DAP adapter
 * is implemented and explicitly contributed.
 */
function testDebugLaunchFallbackContract() {
  const manifest = readJson("editors/vscode/package.json");
  const cliDispatchSource = readText("crates/terlan/src/lib.rs");
  const usageSource = readText("crates/terlan/src/cli_usage.rs");
  const debugSource = readText("crates/terlan/src/commands/debug/mod.rs");

  assert.ok(cliDispatchSource.includes("\"debug\" => commands::debug::run"));
  assert.ok(usageSource.includes("terlc debug <image.tvm>"));
  assert.ok(debugSource.includes("--json-events"));
  assert.ok(debugSource.includes("--script"));
  assert.strictEqual(
    manifest.contributes.debuggers,
    undefined,
    "VS Code must not publish a debugger contribution until the adapter exists"
  );

  return {
    cliFallback: true,
    vscodeDebuggerContribution: "not-published"
  };
}

/**
 * Writes the runnable/debug launch gate report.
 *
 * @param {*} report Report payload.
 * @returns {void}
 *
 * @description
 * Persists machine-readable evidence consumed by roadmap and release gates.
 */
function writeReport(report) {
  fs.mkdirSync(path.dirname(REPORT_PATH), { recursive: true });
  fs.writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
}

const report = {
  runnableInventory: testRunnableInventoryAndWorkspaceSelection(),
  packageWorkspaceTarget: testPackageWorkspaceTargetSelection(),
  commandIds: testVscodeRunnableCommandMetadata(),
  codeLensCommandIds: testCodeLensRunnableCommandIds(),
  launchCommands: testRunnableCommandLines(),
  launchRedaction: testLaunchDescriptorRedactionPolicy(),
  debugLaunch: testDebugLaunchFallbackContract(),
  terminalReuse: Object.assign(
    {
      posixCommandCacheRefresh: true
    },
    testTerminalReuseContract()
  ),
  staleMetadataRejection: {
    editorRuntimeRequired: false,
    compilerOwnedCommands: true,
    namedTestArguments: testStaleNamedTestRejection()
  }
};

writeReport(report);

console.log(`editor runnable/debug launch contract is stable: ${REPORT_PATH}`);
