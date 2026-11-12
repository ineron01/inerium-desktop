-- Hyprland default apps
TERMINAL     = "kitty"
FILE_MANAGER = "dolphin"
BROWSER      = "google-chrome-stable"
EDITOR       = "gnome-text-editor --new-window"
CALCULATOR   = "gnome-calculator"

-- Generic dynamic monitor discovery via DRM sysfs
local function detect_drm_monitors()
    local internal = nil
    local external = nil
    local p = io.popen("grep -l '^connected$' /sys/class/drm/card*-*/status 2>/dev/null")
    if p then
        for line in p:lines() do
            local conn = line:match("/sys/class/drm/card%d+%-(.+)/status$")
            if conn then
                if conn:match("^eDP") or conn:match("^LVDS") or conn:match("^DSI") then
                    if not internal then internal = conn end
                else
                    if not external then external = conn end
                end
            end
        end
        p:close()
    end
    return internal, external
end

local internal_mon, external_mon = detect_drm_monitors()

LAPTOP_MONITOR  = internal_mon or external_mon or "eDP-1"
MONITOR1        = LAPTOP_MONITOR
MONITOR2        = external_mon or MONITOR1
MONITOR3        = external_mon or MONITOR2
PRIMARY_MONITOR = external_mon or MONITOR1

-- Workspaces
NUM_WPM = 3
