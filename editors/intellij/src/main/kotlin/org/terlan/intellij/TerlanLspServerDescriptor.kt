package org.terlan.intellij

/**
 * Terlan language-server deployment contract for IntelliJ-family IDEs.
 *
 * Inputs:
 * - IntelliJ project roots and opened Terlan files.
 *
 * Outputs:
 * - Compiler-owned stdio LSP command and root marker metadata.
 *
 * Transformation:
 * - Converts IDE language-service startup into `terlc lsp --stdio`, leaving
 *   parsing, diagnostics, symbols, and typechecking inside the compiler.
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
}
