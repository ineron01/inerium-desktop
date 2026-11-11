# Hyprland Lua Configuration Guidelines

## Architecture & Layout
- Entry point: `hyprland.lua`
- Modules: `config/variables.lua`, `config/windowrules.lua`, `config/binds_common.lua`, `config/binds_waybar.lua`
- Theme: `config/theme_settings.json` (Kanagawa Dragon palette)
- Immutable Fallback: NEVER modify or delete the fallback session files `config/autostart_backup.lua` and `config/binds_backup.lua`.

## Mandatory Engineering Rules
1. **Pre-flight Syntax Validation:** Always run `luac -p <file>` before reloading.
2. **Reload Command:** Reload using `hyprctl reload`.
3. **Application Launcher Prefix:** Prefix all graphical app bindings with `uwsm app --` for correct systemd session cgroups.
4. **Monitor Detection:** Never hardcode primary laptop monitor; rely on `detect_laptop_monitor()`.
