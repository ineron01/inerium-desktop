-- CachyOS Hyprland Configuration with Profile Support

local function get_profile()
    local env_p = os.getenv("HYPR_PROFILE")
    if env_p and env_p ~= "" then
        return env_p
    end
    local home = os.getenv("HOME") or ""
    local f = io.open(home .. "/.config/hypr/profile", "r")
    if f then
        local p = f:read("*l")
        f:close()
        if p and p:match("%S") then
            return p:gsub("%s+", "")
        end
    end
    return "waybar"
end

local profile = get_profile()

local function safe_require(mod)
    local ok, err = pcall(require, mod)
    if not ok then
        io.stderr:write("[Hyprland Config Error] Failed loading " .. mod .. ": " .. tostring(err) .. "\n")
    end
    return ok
end

-- Common configs (безопасная загрузка: ошибка в одном модуле не роняет сессию)
safe_require("config.animations")
safe_require("config.colors")
safe_require("config.decorations")
safe_require("config.variables")
safe_require("config.environment")
safe_require("config.inputs")
safe_require("config.misc")
safe_require("config.monitors")
safe_require("config.windowrules")
safe_require("config.workspaces")
safe_require("config.binds_common")

-- Profile-dependent configs: 'backup' (резервная сессия) или 'waybar' (основная)
if profile == "backup" then
    safe_require("config.autostart_backup")
    safe_require("config.binds_backup")
else
    safe_require("config.autostart_waybar")
    safe_require("config.binds_waybar")
end
