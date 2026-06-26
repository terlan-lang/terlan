;;; terlan-mode.el --- Terlan editing support -*- lexical-binding: t; -*-

;; Inputs:
;; - Terlan source, interface, and template buffers.
;;
;; Outputs:
;; - A conservative major mode and LSP client registration.
;;
;; Transformation:
;; - Registers file suffixes and delegates language behavior to the installed
;;   compiler through `terlc lsp --stdio`.

;;; Code:

(defgroup terlan nil
  "Editing support for Terlan source files."
  :group 'languages)

(defcustom terlan-lsp-command '("terlc" "lsp" "--stdio")
  "Command used to start the Terlan language server.

Inputs:
- No direct user input beyond customization.

Output:
- Program/argument list consumed by `eglot` or `lsp-mode`.

Transformation:
- Keeps Emacs integration on the compiler-owned stdio LSP endpoint."
  :type '(repeat string)
  :group 'terlan)

(defcustom terlan-enable-treesit t
  "Whether to prefer Tree-sitter-backed Terlan buffers when available.

Inputs:
- User customization and the installed Emacs Tree-sitter grammar set.

Output:
- Non-nil enables remapping `terlan-mode` to `terlan-ts-mode` when the
  `terlan` Tree-sitter grammar is installed.

Transformation:
- Keeps Tree-sitter highlighting optional while preserving the compiler-owned
  `terlc lsp --stdio` diagnostics path."
  :type 'boolean
  :group 'terlan)

(defconst terlan-root-markers '("terlan.toml" ".git")
  "Project root markers for Terlan buffers.

Inputs:
- Current buffer file path.

Output:
- Ordered marker names used by LSP clients for project discovery.

Transformation:
- Prefers `terlan.toml` and falls back to `.git`, matching other editor
integrations.")

(defconst terlan-file-extensions
  '("\\.terl\\'"
    "\\.terli\\'"
    "\\.terl\\.html\\'"
    "\\.terl\\.md\\'"
    "\\.terl\\.json\\'"
    "\\.terl\\.toml\\'"
    "\\.terl\\.ya?ml\\'"
    "\\.terl\\.txt\\'")
  "File suffix patterns handled by `terlan-mode`.

Inputs:
- Buffer file name.

Output:
- Regex patterns added to `auto-mode-alist`.

Transformation:
- Covers Terlan source, interface, and template suffixes without inspecting
file contents.")

(defconst terlan-treesit-language 'terlan
  "Tree-sitter language symbol for Terlan buffers.

Inputs:
- Installed Emacs Tree-sitter language grammars.

Output:
- Language symbol used for optional Tree-sitter parser creation.

Transformation:
- Points Emacs integration at the shared `tree-sitter-terlan` grammar without
embedding a second parser or compiler in the editor package.")

;;;###autoload
(define-derived-mode terlan-mode prog-mode "Terlan"
  "Major mode for Terlan source and template files.

Inputs:
- Current Emacs buffer.

Output:
- Buffer configured for conservative Terlan editing.

Transformation:
- Sets comment syntax and leaves semantic language behavior to the LSP client."
  (setq-local comment-start "// ")
  (setq-local comment-end ""))

;;;###autoload
(define-derived-mode terlan-ts-mode terlan-mode "Terlan[TS]"
  "Tree-sitter-backed major mode for Terlan files.

Inputs:
- Current Emacs buffer and an installed `terlan` Tree-sitter grammar.

Output:
- Buffer configured with the Tree-sitter parser when available.

Transformation:
- Creates a Tree-sitter parser for highlighting/editing support only; parsing,
  diagnostics, symbols, and typechecking still come from `terlc lsp --stdio`."
  (when (and (fboundp 'treesit-parser-create)
             (fboundp 'treesit-language-available-p)
             (treesit-language-available-p terlan-treesit-language))
    (treesit-parser-create terlan-treesit-language)))

;;;###autoload
(defun terlan-register-treesit-remap ()
  "Register optional Tree-sitter remapping for Terlan buffers.

Inputs:
- Current Emacs runtime and installed Tree-sitter grammar set.

Output:
- Non-nil when the remap is registered, nil otherwise.

Transformation:
- Adds `terlan-mode -> terlan-ts-mode` to `major-mode-remap-alist` only when
  Tree-sitter is enabled and the `terlan` language grammar is available."
  (when (and terlan-enable-treesit
             (boundp 'major-mode-remap-alist)
             (fboundp 'treesit-language-available-p)
             (treesit-language-available-p terlan-treesit-language))
    (add-to-list 'major-mode-remap-alist '(terlan-mode . terlan-ts-mode))
    t))

;;;###autoload
(dolist (pattern terlan-file-extensions)
  (add-to-list 'auto-mode-alist `(,pattern . terlan-mode)))

(terlan-register-treesit-remap)

(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs `(terlan-mode . ,terlan-lsp-command)))

(with-eval-after-load 'lsp-mode
  (add-to-list 'lsp-language-id-configuration '(terlan-mode . "terlan"))
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection terlan-lsp-command)
    :major-modes '(terlan-mode)
    :server-id 'terlan)))

(provide 'terlan-mode)

;;; terlan-mode.el ends here
