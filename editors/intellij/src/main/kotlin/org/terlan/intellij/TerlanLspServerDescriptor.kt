package org.terlan.intellij

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor

/** Starts the compiler-owned language server when a Terlan document opens. */
internal class TerlanLspServerSupportProvider : LspServerSupportProvider {
    override fun fileOpened(
        project: Project,
        file: VirtualFile,
        serverStarter: LspServerSupportProvider.LspServerStarter,
    ) {
        if (TerlanFileTypes.isSupported(file.name)) {
            serverStarter.ensureServerStarted(
                TerlanProjectLspServerDescriptor(
                    project,
                    TerlanLspServerDescriptor.workingDirectory(project, file),
                ),
            )
        }
    }
}

/** One compiler-owned LSP process shared by all Terlan files in a project. */
private class TerlanProjectLspServerDescriptor(
    project: Project,
    private val workingDirectory: String?,
) :
    ProjectWideLspServerDescriptor(project, "Terlan") {
    override fun isSupportedFile(file: VirtualFile): Boolean =
        TerlanFileTypes.isSupported(file.name)

    override fun createCommandLine(): GeneralCommandLine {
        val commandLine = GeneralCommandLine(TerlanLspServerDescriptor.command)
        workingDirectory?.let(commandLine::withWorkDirectory)
        return commandLine
    }
}

/**
 * Terlan language-server deployment constants for IntelliJ-family IDEs.
 *
 * Inputs:
 * - IntelliJ project roots and opened Terlan files.
 *
 * Outputs:
 * - Compiler-owned stdio LSP command and root marker metadata.
 *
 * Transformation:
 * - Keeps the executable contract independently testable while the registered
 *   support provider performs the actual JetBrains LSP startup.
 */
object TerlanLspServerDescriptor {
    /**
     * Default language-server command.
     *
     * Inputs:
     * - No user input by default.
     *
     * Outputs:
     * - Program and argument list used to start Terlan LSP.
     *
     * Transformation:
     * - Standardizes IntelliJ on the same LSP process as other editors.
     */
    val command: List<String> = listOf("terlc", "lsp", "--stdio")

    /**
     * Project root markers for Terlan workspaces.
     *
     * Inputs:
     * - Candidate parent directories for an opened Terlan file.
     *
     * Outputs:
     * - Ordered marker names used for root discovery.
     *
     * Transformation:
     * - Prefers `terlan.toml` and falls back to `.git`.
     */
    val rootMarkers: List<String> = listOf("terlan.toml", ".git")

    /** Finds the nearest package or repository root without leaving the IDE project. */
    fun workingDirectory(project: Project, file: VirtualFile): String? {
        val projectPath = project.basePath ?: return file.parent?.path
        var directory = if (file.isDirectory) file else file.parent
        while (directory != null && directory.path.startsWith(projectPath)) {
            if (rootMarkers.any { marker -> directory.findChild(marker) != null }) {
                return directory.path
            }
            if (directory.path == projectPath) {
                break
            }
            directory = directory.parent
        }
        return projectPath
    }
}
