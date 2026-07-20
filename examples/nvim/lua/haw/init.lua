-- haw.nvim — run haw (hawser) from inside Neovim.
--
-- This module just shells the `haw` binary on your PATH — there is no server
-- and no daemon. It parses `--format json` output where useful and otherwise
-- drops you into a terminal split. Dependency-free: only Neovim stdlib + jobs.
--
-- Commands (see plugin/haw.lua):
--   :HawSync     run `haw sync` and echo the result
--   :HawStatus   run `haw status` into a scratch buffer
--   :HawDash     open `haw dash` in a terminal split
--   :HawFleet    list the fleet (repo/branch/state) in a scratch buffer,
--                parsed from `haw status --format json` when available

local M = {}

M.config = {
  -- Path to the haw binary. Override via require("haw").setup({ bin = "..." }).
  bin = "haw",
  -- How to open terminal splits for :HawDash — "split" or "vsplit".
  dash_split = "split",
}

-- Merge user options over the defaults.
function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
end

-- Run `haw <args...>` synchronously, returning (code, stdout_lines, stderr_lines).
local function run(args)
  local cmd = { M.config.bin }
  vim.list_extend(cmd, args)
  local out = vim.fn.systemlist(cmd)
  local code = vim.v.shell_error
  return code, out
end

-- Is the haw binary on PATH?
local function has_haw()
  return vim.fn.executable(M.config.bin) == 1
end

-- Open a throwaway scratch buffer in a split and fill it with `lines`.
local function scratch(title, lines)
  vim.cmd("botright new")
  local buf = vim.api.nvim_get_current_buf()
  vim.bo[buf].buftype = "nofile"
  vim.bo[buf].bufhidden = "wipe"
  vim.bo[buf].swapfile = false
  vim.api.nvim_buf_set_name(buf, title)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].filetype = "haw"
  -- q closes the scratch buffer, like most Neovim helper panels.
  vim.keymap.set("n", "q", "<cmd>close<cr>", { buffer = buf, silent = true })
end

-- Guard: notify + bail if haw is not installed.
local function require_haw()
  if not has_haw() then
    vim.notify(
      "haw.nvim: `" .. M.config.bin .. "` not found on PATH",
      vim.log.levels.ERROR
    )
    return false
  end
  return true
end

-- :HawSync — run `haw sync`, echo a one-line result.
function M.sync()
  if not require_haw() then
    return
  end
  vim.notify("haw: syncing…", vim.log.levels.INFO)
  local code, out = run({ "sync" })
  local level = code == 0 and vim.log.levels.INFO or vim.log.levels.ERROR
  local last = out[#out] or (code == 0 and "sync complete" or "sync failed")
  vim.notify("haw sync: " .. last, level)
end

-- :HawStatus — dump `haw status` into a scratch buffer.
function M.status()
  if not require_haw() then
    return
  end
  local _, out = run({ "status" })
  if vim.tbl_isempty(out) then
    out = { "(no output from `haw status` — are you in a workspace?)" }
  end
  scratch("haw://status", out)
end

-- :HawDash — open `haw dash` in a terminal split.
function M.dash()
  if not require_haw() then
    return
  end
  local split = M.config.dash_split == "vsplit" and "vsplit" or "split"
  vim.cmd(split)
  vim.cmd("terminal " .. vim.fn.fnameescape(M.config.bin) .. " dash")
  vim.cmd("startinsert")
end

-- Parse the fleet out of `haw status --format json`.
-- Returns a list of formatted lines, or nil if the JSON wasn't usable.
local function parse_fleet_json(out)
  local text = table.concat(out, "\n")
  local ok, doc = pcall(vim.json.decode, text)
  if not ok or type(doc) ~= "table" then
    return nil
  end
  local repos = doc.repos
  if type(repos) ~= "table" then
    return nil
  end
  local lines = { string.format("%-20s %-20s %s", "REPO", "BRANCH", "STATE") }
  for _, r in ipairs(repos) do
    if type(r) == "table" then
      local name = r.name or "?"
      local branch = r.branch or r.rev or "-"
      local state = r.dirty and "dirty" or (r.state or "clean")
      lines[#lines + 1] = string.format("%-20s %-20s %s", name, branch, state)
    end
  end
  return lines
end

-- :HawFleet — list the fleet in a scratch buffer. Prefers JSON, falls back to
-- the plain `haw status` text so it still works if the JSON shape differs.
function M.fleet()
  if not require_haw() then
    return
  end
  local code, out = run({ "status", "--format", "json" })
  local lines
  if code == 0 then
    lines = parse_fleet_json(out)
  end
  if not lines then
    local _, plain = run({ "status" })
    lines = vim.tbl_isempty(plain)
        and { "(no fleet — run :HawSync or open a haw workspace)" }
      or plain
  end
  scratch("haw://fleet", lines)
end

return M
