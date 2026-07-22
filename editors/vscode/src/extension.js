"use strict";

const fs = require("fs");
const vscode = require("vscode");
const { createClientOptions, createServerOptions } = require("./client_config");
const {
  RUN_COMMAND_IDS,
  buildBuildCommandLine,
  buildCheckCommandLine,
  buildCleanCommandLine,
  buildDebugBreakpointCommandLine,
  buildDebugCommandLine,
  buildDoctorCommandLine,
  buildInspectCommandLine,
  buildRunCommandLine,
  buildServeCommandLine,
  buildTerminalLaunchDescriptor,
  buildTestFileCommandLine,
  buildTestNameCommandLine,
  buildWatchCommandLine,
  discoverRunnableEntries,
  findQualifiedModuleReferenceAtLine,
  findModuleReferencePrefixAtPosition,
  findTestNameAtLine,
  hasRunnableTestName,
  hasModuleImport,
  importInsertionLine,
  isProjectScriptFilePath,
  isTerlanTestFilePath,
  moduleLeafName,
  parseModuleDeclaration,
  resolveRunTargetPath,
  resolveRunWorkspaceFolder
} = require("./run_command");
const {
  findTemplateComponentTagLinks,
  parseTemplateDeclarations
} = require("./template_links");

let client;
let terlanTerminal;

/**
 * Activates the Terlan VS Code extension.
 *
 * Inputs:
 * - `context`: VS Code extension context used to register disposables.
 *
 * Output:
 * - Registers run/test commands, CodeLens controls, and, when available,
 *   starts the configured Terlan language server.
 *
 * Transformation:
 * - Installs editor actions before LSP startup so missing optional runtime
 *   dependencies cannot prevent runnable source controls from registering.
 */
function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runMain, runMain)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runCheck, runCheck)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runBuild, runBuild)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runClean, runClean)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runServe, runServe)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runWatch, runWatch)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runDoctor, runDoctor)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runDebug, runDebug)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runDebugAtCursor, runDebugAtCursor)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.inspectRuntime, inspectRuntime)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.addMissingImport, () =>
      vscode.commands.executeCommand("editor.action.quickFix"))
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.formatDocument, () =>
      vscode.commands.executeCommand("editor.action.formatDocument"))
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.showDiagnostics, () =>
      vscode.commands.executeCommand("workbench.actions.view.problems"))
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runTestFile, runTestFile)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runTestAtCursor, runTestAtCursor)
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(RUN_COMMAND_IDS.runTestByName, runTestByName)
  );
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(
      [
        { language: "terlan", scheme: "file" },
        { language: "terlan-test", scheme: "file" },
        { pattern: "**/*.terl", scheme: "file" }
      ],
      createTerlanCodeLensProvider()
    )
  );
  context.subscriptions.push(
    vscode.languages.registerCodeActionsProvider(
      [
        { language: "terlan", scheme: "file" },
        { language: "terlan-test", scheme: "file" },
        { pattern: "**/*.terl", scheme: "file" }
      ],
      createTerlanImportCodeActionProvider(),
      {
        providedCodeActionKinds: [vscode.CodeActionKind.QuickFix]
      }
    )
  );
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      [
        { language: "terlan", scheme: "file" },
        { language: "terlan-test", scheme: "file" },
        { pattern: "**/*.terl", scheme: "file" }
      ],
      createTerlanAutoImportCompletionProvider(),
      "."
    )
  );
  context.subscriptions.push(
    vscode.languages.registerDocumentLinkProvider(
      [
        { language: "terlan-template-html", scheme: "file" },
        { pattern: "**/*.terl.html", scheme: "file" }
      ],
      createTerlanTemplateHtmlDocumentLinkProvider()
    )
  );
  context.subscriptions.push(
    vscode.languages.registerDefinitionProvider(
      [
        { language: "terlan-template-html", scheme: "file" },
        { pattern: "**/*.terl.html", scheme: "file" }
      ],
      createTerlanTemplateHtmlDefinitionProvider()
    )
  );
  context.subscriptions.push(
    vscode.window.onDidCloseTerminal((terminal) => {
      if (terminal === terlanTerminal) {
        terlanTerminal = undefined;
      }
    })
  );
  startLanguageClient(context);
}

/**
 * Creates document links from HTML component tags to Terlan template modules.
 *
 * Inputs:
 * - No explicit input; scans workspace `.terl` files when links are requested.
 *
 * Output:
 * - VS Code document-link provider for `.terl.html` template buffers.
 *
 * Transformation:
 * - Maps template component tags such as `<page-shell>` to the Terlan
 *   `template PageShell from ".../page_shell.terl.html"` declaration that
 *   defines the component.
 */
function createTerlanTemplateHtmlDocumentLinkProvider() {
  return {
    async provideDocumentLinks(document) {
      const declarationsByTag = await findTemplateDeclarationsByTag(document.uri);
      const links = findTemplateComponentTagLinks(document.getText(), declarationsByTag);
      return links.map((link) => {
        const range = new vscode.Range(
          document.positionAt(link.start),
          document.positionAt(link.end)
        );
        const target = templateDeclarationUri(link.declaration);
        const documentLink = new vscode.DocumentLink(range, target);
        documentLink.tooltip = `Open template ${link.declaration.name}`;
        return documentLink;
      });
    }
  };
}

/**
 * Creates Go to Definition support for HTML component tags.
 *
 * Inputs:
 * - No explicit input; scans workspace `.terl` files when definition is
 *   requested.
 *
 * Output:
 * - VS Code definition provider for component tag names in `.terl.html`.
 *
 * Transformation:
 * - Reuses the document-link declaration index and returns a precise source
 *   location in the owning Terlan module.
 */
function createTerlanTemplateHtmlDefinitionProvider() {
  return {
    async provideDefinition(document, position) {
      const tag = templateTagAtPosition(document.getText(), document.offsetAt(position));
      if (!tag) {
        return undefined;
      }
      const declarationsByTag = await findTemplateDeclarationsByTag(document.uri);
      const declaration = declarationsByTag.get(tag);
      if (!declaration) {
        return undefined;
      }
      return new vscode.Location(
        vscode.Uri.file(declaration.filePath),
        new vscode.Position(declaration.line, declaration.character)
      );
    }
  };
}

/**
 * Finds workspace template declarations indexed by component tag name.
 *
 * Inputs:
 * - `activeUri`: URI of the template document requesting links/definitions.
 *
 * Output:
 * - Map from component tag names to declaration metadata.
 *
 * Transformation:
 * - Scans project `.terl` files outside generated/build folders and keeps the
 *   first declaration for each tag in deterministic path order.
 */
async function findTemplateDeclarationsByTag(activeUri) {
  const folder = vscode.workspace.getWorkspaceFolder(activeUri)
    || vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    return new Map();
  }
  const files = await vscode.workspace.findFiles(
    new vscode.RelativePattern(folder, "**/*.terl"),
    "{**/_build/**,**/.terlan/**,**/gen/**,**/node_modules/**}",
    500
  );
  const declarationsByTag = new Map();
  const sortedFiles = [...files].sort((left, right) => left.fsPath.localeCompare(right.fsPath));

  for (const uri of sortedFiles) {
    const bytes = await vscode.workspace.fs.readFile(uri);
    const declarations = parseTemplateDeclarations(
      Buffer.from(bytes).toString("utf8"),
      uri.fsPath
    );
    for (const declaration of declarations) {
      if (!declarationsByTag.has(declaration.tag)) {
        declarationsByTag.set(declaration.tag, declaration);
      }
    }
  }

  return declarationsByTag;
}

/**
 * Builds a URI that opens a Terlan template declaration line.
 *
 * Inputs:
 * - `declaration`: parsed template declaration metadata.
 *
 * Output:
 * - File URI with a line/column fragment.
 *
 * Transformation:
 * - Converts zero-based editor positions into VS Code's line fragment format
 *   so document links land near the owning Terlan declaration.
 */
function templateDeclarationUri(declaration) {
  return vscode.Uri.file(declaration.filePath).with({
    fragment: `L${declaration.line + 1},${declaration.character + 1}`
  });
}

/**
 * Returns the component tag under a byte/UTF-16 offset in an HTML template.
 *
 * Inputs:
 * - `text`: template HTML source.
 * - `offset`: VS Code document offset for the cursor.
 *
 * Output:
 * - Component tag at the offset, or `undefined`.
 *
 * Transformation:
 * - Scans opening tag names and checks whether the cursor is inside the tag
 *   name range.
 */
function templateTagAtPosition(text, offset) {
  const source = String(text);
  const tagPattern = /<\s*([a-z][a-z0-9-]*)\b/g;
  let match;
  while ((match = tagPattern.exec(source)) !== null) {
    const start = match.index + match[0].indexOf(match[1]);
    const end = start + match[1].length;
    if (offset >= start && offset <= end) {
      return match[1];
    }
  }
  return undefined;
}

/**
 * Creates completion items that insert missing module imports.
 *
 * Inputs:
 * - No explicit input; reads workspace Terlan modules when completion is
 *   requested.
 *
 * Output:
 * - VS Code completion provider for uppercase module references.
 *
 * Transformation:
 * - Matches the partially typed module leaf before the cursor against module
 *   declarations in workspace `.terl` files. Accepting a completion inserts
 *   the module leaf at the cursor and adds `import <module>.` to the header.
 */
function createTerlanAutoImportCompletionProvider() {
  return {
    /**
     * Provides auto-import module completions for one cursor position.
     *
     * Inputs:
     * - `document`: active Terlan source document.
     * - `position`: cursor position where completion was requested.
     *
     * Output:
     * - Promise resolving to completion items.
     *
     * Transformation:
     * - Builds one completion per matching unimported module. The completion's
     *   additional text edit inserts the import while the main edit completes
     *   the module leaf reference.
     */
    async provideCompletionItems(document, position) {
      const prefix = findModuleReferencePrefixAtPosition(
        document.getText(),
        position.line,
        position.character
      );
      if (!prefix) {
        return [];
      }

      const candidates = await findImportCandidatesForModulePrefix(
        prefix.name,
        document.uri
      );
      const sourceText = document.getText();
      const replaceRange = new vscode.Range(
        position.line,
        prefix.start,
        position.line,
        prefix.end
      );
      return candidates
        .filter((moduleName) => !hasModuleImport(sourceText, moduleName))
        .map((moduleName) => createAutoImportCompletionItem(document, moduleName, replaceRange));
    }
  };
}

/**
 * Creates the Terlan import quick-fix provider.
 *
 * Inputs:
 * - No explicit input; reads active workspace files through VS Code APIs when
 *   code actions are requested.
 *
 * Output:
 * - VS Code CodeAction provider for missing module imports.
 *
 * Transformation:
 * - Maps uppercase qualified references such as `Other.test()` to workspace
 *   modules whose final segment is `Other`, then prepares import insertion
 *   edits.
 */
function createTerlanImportCodeActionProvider() {
  return {
    /**
     * Provides import quick fixes for one Terlan document range.
     *
     * Inputs:
     * - `document`: Terlan source document.
     * - `range`: editor range where quick fixes were requested.
     *
     * Output:
     * - Promise resolving to import CodeActions.
     *
     * Transformation:
     * - Discovers the qualified module reference on the requested line, scans
     *   workspace Terlan modules, and emits one quick fix per matching module.
     */
    async provideCodeActions(document, range) {
      const reference = findQualifiedModuleReferenceAtLine(
        document.getText(),
        range.start.line
      );
      if (!reference) {
        return [];
      }

      const candidates = await findImportCandidatesForModule(reference.name, document.uri);
      return candidates
        .filter((moduleName) => !hasModuleImport(document.getText(), moduleName))
        .map((moduleName) => createImportModuleCodeAction(document, moduleName));
    }
  };
}

/**
 * Finds workspace modules whose leaf name matches a source reference.
 *
 * Inputs:
 * - `moduleLeaf`: source-visible module reference such as `Other`.
 * - `activeUri`: URI of the document requesting the quick fix.
 *
 * Output:
 * - Promise resolving to sorted full module names.
 *
 * Transformation:
 * - Scans workspace `.terl` files outside generated/build folders, parses
 *   their module declarations, and matches on final dotted segment.
 */
async function findImportCandidatesForModule(moduleLeaf, activeUri) {
  const candidates = await findImportCandidatesForModulePrefix(moduleLeaf, activeUri);
  return candidates.filter((moduleName) => moduleLeafName(moduleName) === moduleLeaf);
}

/**
 * Finds workspace modules whose leaf name starts with a source prefix.
 *
 * Inputs:
 * - `modulePrefix`: source-visible module reference prefix such as `Vec`.
 * - `activeUri`: URI of the document requesting completion.
 *
 * Output:
 * - Promise resolving to sorted full module names.
 *
 * Transformation:
 * - Scans workspace `.terl` files outside generated/build folders, parses
 *   their module declarations, and matches on final dotted segment prefix.
 */
async function findImportCandidatesForModulePrefix(modulePrefix, activeUri) {
  const folder = vscode.workspace.getWorkspaceFolder(activeUri);
  const include = new vscode.RelativePattern(folder || vscode.workspace.workspaceFolders?.[0], "**/*.terl");
  const files = await vscode.workspace.findFiles(
    include,
    "{**/_build/**,**/.terlan/**,**/gen/**,**/node_modules/**}",
    500
  );
  const candidates = new Set();
  for (const uri of files) {
    if (uri.fsPath === activeUri.fsPath) {
      continue;
    }
    const bytes = await vscode.workspace.fs.readFile(uri);
    const moduleName = parseModuleDeclaration(Buffer.from(bytes).toString("utf8"));
    if (moduleName && moduleLeafName(moduleName).startsWith(modulePrefix)) {
      candidates.add(moduleName);
    }
  }
  return Array.from(candidates).sort();
}

/**
 * Creates a completion item that also inserts a missing import.
 *
 * Inputs:
 * - `document`: Terlan source document to edit.
 * - `moduleName`: full module name to import.
 * - `replaceRange`: source range containing the partially typed module leaf.
 *
 * Output:
 * - VS Code completion item with a main text edit and import insertion edit.
 *
 * Transformation:
 * - Completes the visible module leaf and attaches a non-overlapping header
 *   edit so accepting the completion automatically adds the import.
 */
function createAutoImportCompletionItem(document, moduleName, replaceRange) {
  const leaf = moduleLeafName(moduleName);
  const importText = `import ${moduleName}.`;
  const item = new vscode.CompletionItem(leaf, vscode.CompletionItemKind.Module);
  item.detail = moduleName;
  item.filterText = leaf;
  item.sortText = `0_${leaf}_${moduleName}`;
  item.textEdit = vscode.TextEdit.replace(replaceRange, leaf);
  item.commitCharacters = ["."];
  const line = importInsertionLine(document.getText(), importText);
  item.additionalTextEdits = [
    vscode.TextEdit.insert(
      new vscode.Position(line, 0),
      `${importText}\n`
    )
  ];
  return item;
}

/**
 * Creates a single import insertion code action.
 *
 * Inputs:
 * - `document`: Terlan source document to edit.
 * - `moduleName`: full module name to import.
 *
 * Output:
 * - VS Code quick-fix action with a workspace edit.
 *
 * Transformation:
 * - Inserts `import <module>.` after the module/import header block.
 */
function createImportModuleCodeAction(document, moduleName) {
  const importText = `import ${moduleName}.`;
  const action = new vscode.CodeAction(
    `Import module ${moduleName}`,
    vscode.CodeActionKind.QuickFix
  );
  const edit = new vscode.WorkspaceEdit();
  const line = importInsertionLine(document.getText(), importText);
  edit.insert(document.uri, new vscode.Position(line, 0), `${importText}\n`);
  action.edit = edit;
  action.isPreferred = true;
  return action;
}

/**
 * Starts the Terlan language client when its package dependency is available.
 *
 * Inputs:
 * - `context`: VS Code extension context used to register the LSP disposable.
 *
 * Output:
 * - Starts the language client and registers it for disposal, or logs a warning
 *   when `vscode-languageclient` is not present in the installed extension.
 *
 * Transformation:
 * - Lazily loads the optional LSP dependency after run/test commands are
 *   registered, preserving the editor runner even for manually synced extension
 *   installs that do not include Node dependencies.
 */
function startLanguageClient(context) {
  let LanguageClient;
  try {
    ({ LanguageClient } = require("vscode-languageclient/node"));
  } catch (error) {
    console.warn(`Terlan LSP disabled: ${error.message}`);
    return;
  }

  const config = vscode.workspace.getConfiguration("terlan");
  const serverOptions = createServerOptions(config, vscode.workspace);
  const clientOptions = createClientOptions();
  client = new LanguageClient("terlan", "Terlan", serverOptions, clientOptions);
  context.subscriptions.push(client.start());
}

/**
 * Runs a Terlan file or the active Terlan workspace through `terlc run`.
 *
 * Inputs:
 * - Optional `filePath` supplied by CodeLens; otherwise reads the active
 *   editor, workspace folders, and `terlan.run.command` configuration from VS
 *   Code.
 *
 * Output:
 * - Sends `terlc run <file-or-workspace>` to the shared Terlan terminal, or
 *   shows a warning when no runnable target is available.
 *
 * Transformation:
 * - Lets CodeLens launches carry the exact source file that declared `main`
 *   while command-palette launches continue to use active workspace resolution.
 */
async function runMain(filePath = undefined) {
  if (filePath) {
    if (!String(filePath).endsWith(".terl")) {
      vscode.window.showWarningMessage("Select a Terlan source file before running main.");
      return;
    }
    await saveDocumentByPath(filePath);
    sendTerlanTerminalCommand(buildRunCommandLine(terlanCommand(), filePath));
    return;
  }

  const projectPath = activeRunTargetPath();
  if (!projectPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before running.");
    return;
  }

  await saveActiveTerlanDocument();
  const editor = vscode.window.activeTextEditor;
  const activeFilePath = editor && editor.document && editor.document.uri.fsPath;
  const targetPath = isProjectScriptFilePath(activeFilePath, projectPath)
    ? activeFilePath
    : projectPath;
  sendTerlanTerminalCommand(
    buildRunCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Runs compiler-owned package checks for the active workspace.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends `terlc check <workspace>` to the shared Terlan terminal, or shows a
 *   warning when no workspace folder is available.
 *
 * Transformation:
 * - Reuses the compiler's package/check path instead of maintaining
 *   editor-owned validation.
 */
async function runCheck() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before checking.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildCheckCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Builds the active Terlan workspace through the compiler.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends `terlc build <workspace>` to the shared Terlan terminal, or shows a
 *   warning when no workspace folder is available.
 *
 * Transformation:
 * - Keeps artifact generation compiler-owned while exposing a stable editor
 *   command for package workflows.
 */
async function runBuild() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before building.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildBuildCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Cleans compiler-owned artifacts for the active Terlan workspace.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends `terlc clean <workspace>` to the shared Terlan terminal, or shows a
 *   warning when no workspace folder is available.
 *
 * Transformation:
 * - Delegates cleanup to the compiler command instead of deleting build
 *   directories in the editor extension.
 */
async function runClean() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before cleaning.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildCleanCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Starts the active Terlan web package through `terlc serve`.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends `terlc serve <workspace>` to the shared Terlan terminal, or shows a
 *   warning when no workspace folder is available.
 *
 * Transformation:
 * - Keeps HTTP serving compiler-owned while exposing a stable editor command
 *   for local web application workflows.
 */
async function runServe() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before serving.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildServeCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Starts the active Terlan web package in live-reload watch mode.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends compiler-owned `terlc serve <workspace> --poll-ms 250` to the shared
 *   Terlan terminal, or shows a warning when no workspace folder is available.
 *
 * Transformation:
 * - Exposes the existing serve watcher without adding editor-side file watching
 *   or duplicate HTTP process management.
 */
async function runWatch() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before watching.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildWatchCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Runs compiler-owned Terlan project diagnostics for the active workspace.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends `terlc doctor <workspace>` to the shared Terlan terminal, or shows a
 *   warning when no workspace folder is available.
 *
 * Transformation:
 * - Exposes the CLI doctor/support surface without duplicating VM-pivot,
 *   generated-output, or support-bundle diagnostics in the editor extension.
 */
async function runDoctor() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before running doctor.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildDoctorCommandLine(terlanCommand(), targetPath)
  );
}

/** Runs a deterministic VM/runtime snapshot for the active workspace. */
async function inspectRuntime() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before inspecting the runtime.");
    return;
  }
  sendTerlanTerminalCommand(buildInspectCommandLine(terlanCommand(), targetPath));
}

/**
 * Launches the compiler-owned Terlan debugger event stream for a workspace.
 *
 * Inputs:
 * - No explicit input; reads the active editor, workspace folders, and
 *   `terlan.run.command` configuration from VS Code.
 *
 * Output:
 * - Sends `terlc debug <workspace> --json-events` to the shared Terlan
 *   terminal, or shows a warning when no workspace folder is available.
 *
 * Transformation:
 * - Provides a stable editor debugger launch target while keeping the VS Code
 *   DAP contribution unpublished until a real adapter exists.
 */
async function runDebug() {
  const targetPath = activeRunTargetPath();
  if (!targetPath) {
    vscode.window.showWarningMessage("Open a Terlan workspace before debugging.");
    return;
  }

  await saveActiveTerlanDocument();
  sendTerlanTerminalCommand(
    buildDebugCommandLine(terlanCommand(), targetPath)
  );
}

/**
 * Launches the Terlan debugger with a breakpoint at the active cursor line.
 *
 * Inputs:
 * - Active Terlan editor and cursor position.
 *
 * Output:
 * - Sends `terlc debug <file> --break <file:line> --json-events` to the shared
 *   Terlan terminal, or shows a warning when no Terlan source file is active.
 *
 * Transformation:
 * - Converts VS Code's zero-based cursor line to the one-based compiler
 *   breakpoint format while leaving breakpoint validation to `terlc debug`.
 */
async function runDebugAtCursor() {
  const editor = vscode.window.activeTextEditor;
  const filePath = editor && editor.document && editor.document.uri.fsPath;
  if (!filePath || !String(filePath).endsWith(".terl")) {
    vscode.window.showWarningMessage("Open a Terlan source file before debugging at cursor.");
    return;
  }

  await saveDocumentByPath(filePath);
  sendTerlanTerminalCommand(
    buildDebugBreakpointCommandLine(
      terlanCommand(),
      filePath,
      editor.selection.active.line + 1
    )
  );
}

/**
 * Resolves the workspace folder used by run/serve/watch editor actions.
 *
 * Inputs:
 * - Active VS Code editor and workspace folders.
 *
 * Output:
 * - Workspace folder selected for compiler-owned runnable commands.
 *
 * Transformation:
 * - Centralizes active-document workspace resolution so all workspace-level
 *   commands use identical multi-root behavior.
 */
function activeRunWorkspaceFolder() {
  return resolveRunWorkspaceFolder(vscode.workspace, vscode.window.activeTextEditor);
}

/**
 * Resolves the package/workspace path used by workspace-level run actions.
 *
 * Inputs:
 * - Active VS Code editor, workspace folders, and local filesystem metadata.
 *
 * Output:
 * - Nearest active Terlan package root containing `terlan.toml`, or the owning
 *   VS Code workspace root when no nested package manifest is present.
 *
 * Transformation:
 * - Keeps package-root selection in the editor aligned with compiler-owned
 *   package commands while preserving deterministic multi-root fallback.
 */
function activeRunTargetPath() {
  return resolveRunTargetPath(
    vscode.workspace,
    vscode.window.activeTextEditor,
    (filePath) => fs.existsSync(filePath)
  );
}

/**
 * Runs every Terlan test in the active test file.
 *
 * Inputs:
 * - No explicit input; reads the active editor and `terlan.run.command` from
 *   VS Code.
 *
 * Output:
 * - Sends `terlc test <active-file>` to the shared Terlan terminal, or shows a
 *   warning when the active document is not a Terlan test file.
 *
 * Transformation:
 * - Validates the active file shape, builds a shell-safe compiler command, and
 *   delegates execution to the shared integrated terminal.
 */
async function runTestFile() {
  const editor = vscode.window.activeTextEditor;
  const filePath = editor && editor.document && editor.document.uri.fsPath;
  if (!filePath || !isTerlanTestFilePath(filePath)) {
    vscode.window.showWarningMessage("Open a Terlan *Test.terl file before running tests.");
    return;
  }

  await saveDocumentByPath(filePath);
  sendTerlanTerminalCommand(buildTestFileCommandLine(terlanCommand(), filePath));
}

/**
 * Runs the Terlan test function under the active cursor.
 *
 * Inputs:
 * - No explicit input; reads the active editor text, cursor line, and
 *   `terlan.run.command` from VS Code.
 *
 * Output:
 * - Sends `terlc test <active-file> --name <test>` to the shared Terlan
 *   terminal, or shows a warning when no surrounding `@test` function is found.
 *
 * Transformation:
 * - Maps the cursor line to the nearest containing `@test` function and
 *   delegates exact test selection to the compiler's `--name` option.
 */
async function runTestAtCursor() {
  const editor = vscode.window.activeTextEditor;
  const filePath = editor && editor.document && editor.document.uri.fsPath;
  if (!filePath || !isTerlanTestFilePath(filePath)) {
    vscode.window.showWarningMessage("Open a Terlan *Test.terl file before running a test.");
    return;
  }

  const testName = findTestNameAtLine(
    editor.document.getText(),
    editor.selection.active.line
  );
  if (!testName) {
    vscode.window.showWarningMessage("Place the cursor inside a Terlan @test function.");
    return;
  }

  await saveDocumentByPath(filePath);
  sendTerlanTerminalCommand(
    buildTestNameCommandLine(terlanCommand(), filePath, testName)
  );
}

/**
 * Runs one named Terlan test from editor integrations.
 *
 * Inputs:
 * - `filePath`: active or CodeLens-provided Terlan `*Test.terl` path.
 * - `testName`: exact `@test` function name.
 *
 * Output:
 * - Sends `terlc test <file> --name <test>` to the shared Terlan terminal, or
 *   shows a warning when the arguments are not a runnable Terlan test.
 *
 * Transformation:
 * - Validates the file and test name supplied by CodeLens before delegating
 *   execution to the compiler-owned test selector.
 */
async function runTestByName(filePath, testName) {
  if (!filePath || !isTerlanTestFilePath(filePath) || !testName) {
    vscode.window.showWarningMessage("Select a Terlan @test function to run.");
    return;
  }

  await saveDocumentByPath(filePath);
  const document = openDocumentByPath(filePath);
  if (document && !hasRunnableTestName(document.getText(), testName)) {
    vscode.window.showWarningMessage(
      `Terlan test '${testName}' no longer exists in this file.`
    );
    return;
  }
  sendTerlanTerminalCommand(
    buildTestNameCommandLine(terlanCommand(), filePath, testName)
  );
}

/**
 * Saves the active Terlan source document when it has pending edits.
 *
 * Inputs:
 * - Active VS Code text editor, when present.
 *
 * Output:
 * - Promise resolving after save completes or immediately when no save is
 *   needed.
 *
 * Transformation:
 * - Flushes editor changes to disk before compiler-owned run/test commands
 *   read source files from the filesystem.
 */
async function saveActiveTerlanDocument() {
  const editor = vscode.window.activeTextEditor;
  const document = editor && editor.document;
  if (!document || !String(document.uri.fsPath).endsWith(".terl")) {
    return;
  }
  await saveDocument(document);
}

/**
 * Saves an open Terlan document by filesystem path.
 *
 * Inputs:
 * - `filePath`: source path that will be passed to `terlc`.
 *
 * Output:
 * - Promise resolving after the matching open document is saved, if present.
 *
 * Transformation:
 * - Finds the open VS Code document for the compiler input path and saves it
 *   so CodeLens and command-palette test runs use current editor contents.
 */
async function saveDocumentByPath(filePath) {
  const document = openDocumentByPath(filePath);
  if (document) {
    await saveDocument(document);
  }
}

/**
 * Finds an already-open VS Code document by filesystem path.
 *
 * Inputs:
 * - `filePath`: source path provided by a command or CodeLens argument.
 *
 * Output:
 * - Matching open document, when VS Code currently has one.
 *
 * Transformation:
 * - Keeps stale CodeLens validation scoped to open buffers and leaves closed
 *   files to the compiler-owned `terlc test --name` selector.
 */
function openDocumentByPath(filePath) {
  return vscode.workspace.textDocuments.find(
    (candidate) => candidate.uri && candidate.uri.fsPath === filePath
  );
}

/**
 * Saves one dirty VS Code document.
 *
 * Inputs:
 * - `document`: VS Code text document.
 *
 * Output:
 * - Promise resolving after the document is saved or skipped.
 *
 * Transformation:
 * - Calls VS Code's document save API only when the buffer is dirty, avoiding
 *   unnecessary filesystem writes for already-current files.
 */
async function saveDocument(document) {
  if (document.isDirty) {
    await document.save();
  }
}

/**
 * Creates the Terlan CodeLens provider for runnable source declarations.
 *
 * Inputs:
 * - No explicit input; returns a VS Code provider object.
 *
 * Output:
 * - Provider that renders CodeLens run controls above `pub main()` and
 *   `@test` functions in Terlan source files.
 *
 * Transformation:
 * - Converts lightweight source discovery results into VS Code CodeLens
 *   command objects that delegate to `terlc run` or `terlc test --name`.
 */
function createTerlanCodeLensProvider() {
  return {
    /**
     * Provides runnable CodeLens controls for one Terlan document.
     *
     * Inputs:
     * - `document`: VS Code text document to inspect.
     *
     * Output:
     * - Array of VS Code `CodeLens` objects.
     *
     * Transformation:
     * - Discovers runnable entries from source text and maps them to command
     *   lenses with codicon-backed titles.
     */
    provideCodeLenses(document) {
      const entries = discoverRunnableEntries(
        document.getText(),
        isTerlanTestFilePath(document.uri.fsPath)
      );
      return entries.map((entry) => {
        const range = new vscode.Range(entry.line, 0, entry.line, 0);
        if (entry.kind === "test") {
          return new vscode.CodeLens(range, {
            title: "$(play) Run Test",
            command: RUN_COMMAND_IDS.runTestByName,
            arguments: [document.uri.fsPath, entry.name]
          });
        }
        return new vscode.CodeLens(range, {
          title: "$(play) Run",
          command: RUN_COMMAND_IDS.runMain,
          arguments: [document.uri.fsPath]
        });
      });
    }
  };
}

/**
 * Returns the configured compiler command for run/test terminal actions.
 *
 * Inputs:
 * - No explicit input; reads `terlan.run.command` from VS Code settings.
 *
 * Output:
 * - Compiler command string, defaulting to `terlc`.
 *
 * Transformation:
 * - Centralizes command lookup so run and test editor actions use the same
 *   configured compiler executable.
 */
function terlanCommand() {
  return vscode.workspace.getConfiguration("terlan").get("run.command", "terlc");
}

/**
 * Sends one command line to the shared Terlan terminal.
 *
 * Inputs:
 * - `commandLine`: shell command assembled by editor command helpers.
 *
 * Output:
 * - Visible integrated terminal running the requested command.
 *
 * Transformation:
 * - Reuses one Terlan terminal when available, creating it only once per
 *   editor session so repeated run/test actions do not spawn a fresh shell.
 */
function sendTerlanTerminalCommand(commandLine) {
  const terminal = sharedTerlanTerminal();
  const launch = buildTerminalLaunchDescriptor("command", undefined, commandLine);
  terminal.show();
  terminal.sendText(launch.terminalCommandLine);
}

/**
 * Returns the shared Terlan integrated terminal.
 *
 * Inputs:
 * - Module-level cached terminal handle.
 *
 * Output:
 * - Live VS Code terminal used by Terlan run/test commands.
 *
 * Transformation:
 * - Lazily creates one terminal and reuses it until VS Code reports it closed,
 *   preserving shell state and avoiding per-click terminal startup cost.
 */
function sharedTerlanTerminal() {
  if (!terlanTerminal) {
    terlanTerminal = vscode.window.createTerminal("Terlan");
  }
  return terlanTerminal;
}

/**
 * Deactivates the Terlan VS Code extension.
 *
 * Inputs:
 * - No explicit input; uses the module-level language client if active.
 *
 * Output:
 * - Promise returned by `client.stop()`, or `undefined` when no client exists.
 *
 * Transformation:
 * - Stops the language client so the spawned Terlan LSP process exits cleanly.
 */
function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

module.exports = {
  activate,
  startLanguageClient,
  deactivate,
  createTerlanCodeLensProvider,
  createTerlanTemplateHtmlDefinitionProvider,
  createTerlanTemplateHtmlDocumentLinkProvider,
  runBuild,
  runCheck,
  runClean,
  runDebug,
  runDebugAtCursor,
  runDoctor,
  inspectRuntime,
  runMain,
  runServe,
  activeRunTargetPath,
  runTestAtCursor,
  runTestByName,
  runTestFile,
  runWatch,
  saveActiveTerlanDocument,
  saveDocumentByPath,
  sendTerlanTerminalCommand,
  sharedTerlanTerminal,
  templateTagAtPosition
};
