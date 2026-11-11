-- Hyprland default apps
TERMINAL     = "kitty"
FILE_MANAGER = "dolphin"
BROWSER      = "firefox"
EDITOR       = "gnome-text-editor --new-window"
CALCULATOR   = "gnome-calculator"

-- Generic dynamic monitor discovery via DRM sysfs
local function detect_drm_monitors()
    local internal = nil
    local external = nil
    local connectors = { "eDP-2", "eDP-1", "LVDS-1", "HDMI-A-1", "HDMI-A-2", "DP-1", "DP-2", "DP-3" }
    for card = 0, 3 do
        for _, conn in ipairs(connectors) do
            local path = string.format("/sys/class/drm/card%d-%s/status", card, conn)
            local f = io.open(path, "r")
            if f then
                local status = f:read("*l")
                f:close()
                if status == "connected" then
                    if conn:match("^eDP") or conn:match("^LVDS") then
                        if not internal then internal = conn end
                    else
                        if not external then external = conn end
                    end
                end
            end
        end
    end
    return internal, external
end

local internal_mon, external_mon = detect_drm_monitors()

LAPTOP_MONITOR  = internal_mon or external_mon or "eDP-1"
MONITOR1        = LAPTOP_MONITOR
MONITOR2        = external_mon or MONITOR1
MONITOR3        = (LAPTOP_MONITOR == "eDP-2") and "eDP-1" or "eDP-2"
PRIMARY_MONITOR = external_mon or MONITOR1

-- Workspaces
NUM_WPM = 3
