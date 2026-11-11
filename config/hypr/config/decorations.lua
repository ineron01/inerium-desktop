-- Look and feel configuration - Kanagawa Dragon aesthetic

hl.config({
    general = {
        gaps_in = 4,
        gaps_out = 4,
        border_size = 1,
        extend_border_grab_area = 10,
        resize_on_border = true,
        col = {
            active_border = {
                colors = { KANAGAWA_DRAGON_GRAY, KANAGAWA_DRAGON_BLUE },
                angle = 45,
            },
            inactive_border = KANAGAWA_BORDER,
        },
    },
    group = {
        col = {
            border_active = KANAGAWA_BLUE,
            border_inactive = KANAGAWA_BORDER,
            border_locked_active = KANAGAWA_PURPLE,
            border_locked_inactive = KANAGAWA_BORDER,
        },
        groupbar = {
            col = {
                active = KANAGAWA_DRAGON_BLUE,
                inactive = KANAGAWA_BORDER,
                locked_active = KANAGAWA_PURPLE,
                locked_inactive = KANAGAWA_BORDER,
            },
        },
    },
    decoration = {
        dim_special = 0.3,
        rounding = 10,
        active_opacity = 0.96,
        inactive_opacity = 0.88,
        fullscreen_opacity = 1.0,
        blur = {
            enabled = true,
            size = 6,
            passes = 3,
            new_optimizations = true,
            xray = false,
            special = true,
        },
        shadow = {
            enabled = true,
            range = 14,
            render_power = 3,
            color = "rgba(18, 16, 16, 0.65)",
        },
    },
})
