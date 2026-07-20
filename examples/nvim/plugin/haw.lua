-- haw.nvim — command definitions. Loaded automatically by Neovim on startup
-- (or via :packadd for opt plugins). The real logic lives in lua/haw/init.lua.

if vim.g.loaded_haw_nvim then
  return
end
vim.g.loaded_haw_nvim = true

local function haw()
  return require("haw")
end

vim.api.nvim_create_user_command("HawSync", function()
  haw().sync()
end, { desc = "haw: sync the workspace" })

vim.api.nvim_create_user_command("HawStatus", function()
  haw().status()
end, { desc = "haw: workspace status in a scratch buffer" })

vim.api.nvim_create_user_command("HawDash", function()
  haw().dash()
end, { desc = "haw: open the dashboard in a terminal split" })

vim.api.nvim_create_user_command("HawFleet", function()
  haw().fleet()
end, { desc = "haw: list the fleet in a scratch buffer" })
