-- Terlan Neovim LSP startup helper.
--
-- Inputs:
-- - Current Neovim buffer and project files.
--
-- Outputs:
-- - Starts the Terlan LSP client for the buffer.
--
-- Transformation:
-- - Converts editor state into a `vim.lsp.start` request that launches the
--   installed compiler with `terlc lsp --stdio`.

local M = {}

M.command = { "terlc", "lsp", "--stdio" }
M.root_markers = { "terlan.toml", ".git" }
M.tree_sitter_language = "terlan"
M.filetypes = {
  "terlan",
  "terlan-interface",
  "terlan-template-html",
  "terlan-template-markdown",
  "terlan-template-json",
  "terlan-template-toml",
  "terlan-template-yaml",
  "terlan-template-text",
}

--- Returns the project root for a Terlan buffer.
---
--- Inputs:
--- - `buffer`: optional Neovim buffer handle.
---
--- Outputs:
--- - Project root path from `terlan.toml` / `.git`, or current working dir.
---
--- Transformation:
--- - Uses `vim.fs.root` when available and falls back to `vim.fn.getcwd`.
function M.root_dir(buffer)
  local root = nil
  if vim.fs and vim.fs.root then
    root = vim.fs.root(buffer or 0, M.root_markers)
  end
  return root or vim.fn.getcwd()
end

--- Registers Terlan filetypes with Neovim Tree-sitter when available.
---
--- Inputs:
--- - Current Neovim runtime with optional `vim.treesitter.language.register`.
---
--- Outputs:
--- - `true` when registration was attempted.
--- - `false` when the host does not expose the Tree-sitter registration API.
---
--- Transformation:
--- - Maps every Terlan source/interface/template filetype to the shared
---   `terlan` Tree-sitter language so editor highlighting can reuse the
---   checked-in `tree-sitter-terlan` package without changing LSP behavior.
function M.register_treesitter()
  if not (vim.treesitter and vim.treesitter.language and vim.treesitter.language.register) then
    return false
  end
  vim.treesitter.language.register(M.tree_sitter_language, M.filetypes)
  return true
end

--- Starts the Terlan language server for one buffer.
---
--- Inputs:
--- - `buffer`: optional Neovim buffer handle.
---
--- Outputs:
--- - Result returned by `vim.lsp.start`.
---
--- Transformation:
--- - Builds the LSP client configuration while leaving parsing, diagnostics,
---   symbols, and typechecking inside `terlc`.
function M.start(buffer)
  return vim.lsp.start({
    name = "terlan",
    cmd = M.command,
    root_dir = M.root_dir(buffer),
  }, { bufnr = buffer or 0 })
end

--- Sets up the current Terlan buffer.
---
--- Inputs:
--- - Current Neovim buffer.
---
--- Outputs:
--- - Starts or attaches the Terlan LSP client.
---
--- Transformation:
--- - Keeps ftplugin behavior small and delegates to `M.start`.
function M.setup_buffer()
  M.register_treesitter()
  return M.start(0)
end

return M
