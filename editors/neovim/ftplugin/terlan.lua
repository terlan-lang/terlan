-- Buffer-local Terlan setup for Neovim.
--
-- Inputs:
-- - Current Terlan buffer.
--
-- Outputs:
-- - Starts or attaches the Terlan LSP client for the buffer.
--
-- Transformation:
-- - Delegates all language behavior to the compiler-owned `terlc lsp --stdio`
--   endpoint through the shared Lua helper.

require("terlan_lsp").setup_buffer()
