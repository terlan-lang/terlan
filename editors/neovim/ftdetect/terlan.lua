-- Terlan filetype detection for Neovim.
--
-- Inputs:
-- - Buffer names ending in Terlan source, interface, or template suffixes.
--
-- Outputs:
-- - Neovim filetype names used by the Terlan ftplugin and LSP setup.
--
-- Transformation:
-- - Maps filename suffixes to stable filetypes without parsing file contents.

local group = vim.api.nvim_create_augroup("terlan_filetypes", { clear = true })

vim.api.nvim_create_autocmd({ "BufRead", "BufNewFile" }, {
  group = group,
  pattern = {
    "*.terl",
    "*.terli",
    "*.terl.html",
    "*.terl.md",
    "*.terl.json",
    "*.terl.toml",
    "*.terl.yaml",
    "*.terl.yml",
    "*.terl.txt",
  },
  callback = function(event)
    local name = event.file
    if name:match("%.terl%.html$") then
      vim.bo[event.buf].filetype = "terlan-template-html"
    elseif name:match("%.terl%.md$") then
      vim.bo[event.buf].filetype = "terlan-template-markdown"
    elseif name:match("%.terl%.json$") then
      vim.bo[event.buf].filetype = "terlan-template-json"
    elseif name:match("%.terl%.toml$") then
      vim.bo[event.buf].filetype = "terlan-template-toml"
    elseif name:match("%.terl%.ya?ml$") then
      vim.bo[event.buf].filetype = "terlan-template-yaml"
    elseif name:match("%.terl%.txt$") then
      vim.bo[event.buf].filetype = "terlan-template-text"
    elseif name:match("%.terli$") then
      vim.bo[event.buf].filetype = "terlan-interface"
    else
      vim.bo[event.buf].filetype = "terlan"
    end
  end,
})
