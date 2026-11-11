-- Monitor configuration
-- Loads host-specific config.monitors_local if present; otherwise falls back to universal auto-detection.

local ok = pcall(require, "config.monitors_local")
if not ok then
    hl.monitor({
        output   = "",
        mode     = "preferred",
        position = "auto",
        scale    = 1.0,
    })
end
