-- Universal workspace rules (skipped if host-specific config.monitors_local overrides them)

if INERIUM_WORKSPACES_OVERRIDDEN then
    return
end

local per_monitor = 3
local count_path = (os.getenv("HOME") or "") .. "/.config/hypr/config/workspaces_count"
local cf = io.open(count_path, "r")
if cf then
    local v = tonumber(cf:read("*l"))
    cf:close()
    if v and v >= 1 and v <= 9 then
        per_monitor = math.floor(v)
    end
end

local has_secondary = MONITOR2 and MONITOR2 ~= "" and MONITOR2 ~= MONITOR1 and hl.get_monitor(MONITOR2) ~= nil

if has_secondary then
    for i = 1, per_monitor do
        hl.workspace_rule({ workspace = tostring(i), monitor = MONITOR1, default = (i == 1), persistent = true })
    end
    for i = 1, per_monitor do
        hl.workspace_rule({ workspace = tostring(per_monitor + i), monitor = MONITOR2, default = (i == 1), persistent = true })
    end
    hl.workspace_rule({ workspace = "name:gaming", monitor = MONITOR2 })
else
    for i = 1, per_monitor do
        hl.workspace_rule({ workspace = tostring(i), default = (i == 1), persistent = true })
    end
end
