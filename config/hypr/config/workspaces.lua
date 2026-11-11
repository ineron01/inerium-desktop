-- Universal workspace rules (skipped if host-specific config.monitors_local overrides them)

if KANAGAWA_WORKSPACES_OVERRIDDEN then
    return
end

local has_secondary = MONITOR2 and MONITOR2 ~= "" and MONITOR2 ~= MONITOR1 and hl.get_monitor(MONITOR2) ~= nil

if has_secondary then
    hl.workspace_rule({ workspace = "1", monitor = MONITOR1, default = true, persistent = true })
    hl.workspace_rule({ workspace = "2", monitor = MONITOR1, persistent = true })
    hl.workspace_rule({ workspace = "3", monitor = MONITOR1, persistent = true })

    hl.workspace_rule({ workspace = "4", monitor = MONITOR2, default = true, persistent = true })
    hl.workspace_rule({ workspace = "5", monitor = MONITOR2, persistent = true })
    hl.workspace_rule({ workspace = "6", monitor = MONITOR2, persistent = true })
    hl.workspace_rule({ workspace = "name:gaming", monitor = MONITOR2 })
else
    hl.workspace_rule({ workspace = "1", default = true, persistent = true })
    hl.workspace_rule({ workspace = "2", persistent = true })
    hl.workspace_rule({ workspace = "3", persistent = true })
    hl.workspace_rule({ workspace = "4", persistent = true })
    hl.workspace_rule({ workspace = "5", persistent = true })
    hl.workspace_rule({ workspace = "6", persistent = true })
end
