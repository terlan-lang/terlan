package org.terlan.intellij

/**
 * Static Terlan file type metadata for IntelliJ-family IDEs.
 *
 * Inputs:
 * - File names opened by the IDE.
 *
 * Outputs:
 * - Stable suffix, language id, and icon metadata consumed by plugin
 *   registration.
 *
 * Transformation:
 * - Keeps file identity declarative so smoke tests can validate editor
 *   registration without invoking the IDE runtime.
 */
object TerlanFileTypes {
    /**
     * Canonical shared Terlan editor icon path.
     *
     * Inputs:
     * - Plugin metadata or packaging tasks.
     *
     * Outputs:
     * - Plugin-resource icon path.
     *
     * Transformation:
     * - Points IntelliJ metadata at a package-local copy that smoke tests keep
     *   byte-identical to the shared canonical editor icon source.
     */
    const val ICON_PATH: String = "/icons/terlan-file.svg"

    /**
     * Terlan source, interface, and template suffixes.
     *
     * Inputs:
     * - File names opened by the IDE.
     *
     * Outputs:
     * - Suffixes that should be associated with Terlan editor behavior.
     *
     * Transformation:
     * - Covers the same suffix family as VS Code, Neovim, and Emacs.
     */
    val suffixes: List<String> = listOf(
        ".terl",
        ".terli",
        ".terl.html",
        ".terl.md",
        ".terl.json",
        ".terl.toml",
        ".terl.yaml",
        ".terl.yml",
        ".terl.txt",
    )
}
