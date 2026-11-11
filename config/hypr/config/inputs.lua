-- Input configuration

hl.config({
    input = {
        accel_profile = "flat",
        kb_layout     = "us,ru",
        kb_options    = "grp:alt_shift_toggle",
    },
})

hl.gesture({ fingers = 4, direction = "horizontal", action = "workspace" })
hl.gesture({ fingers = 3, direction = "down",       action = "close" })
hl.gesture({ fingers = 3, direction = "up",         action = "fullscreen" })
hl.gesture({ fingers = 3, direction = "left",       action = "float" })

pcall(require, "config.inputs_local")
