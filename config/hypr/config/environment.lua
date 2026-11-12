-- Environmental variables (for reference https://wiki.hypr.land/Configuring/Advanced-and-Cool/Environment-variables/)

hl.env("QT_QPA_PLATFORM", "wayland;xcb")
hl.env("QT_QPA_PLATFORMTHEME", "qt6ct")
hl.env("QT_STYLE_OVERRIDE", "Fusion")
hl.env("GTK_THEME", "adw-gtk3-dark")
hl.env("GTK_USE_PORTAL", "1")
hl.env("HYPRCURSOR_THEME", "Bibata-Modern-Ice")
hl.env("HYPRCURSOR_SIZE", "24")
hl.env("XCURSOR_THEME", "Bibata-Modern-Ice")
hl.env("XCURSOR_SIZE", "24")

-- Отключение мостов специальных возможностей (экранные дикторы не используются)
hl.env("NO_AT_BRIDGE", "1")

-- if you have an NVIDIA GPU uncomment the following lines:

-- hl.env("GBM_BACKEND", "nvidia-drm") -- force GBM as a backend
-- hl.env("__GLX_VENDOR_LIBRARY_NAME", "nvidia") -- force GBM as a backend
-- hl.env("LIBVA_DRIVER_NAME", "nvidia") -- Hardware acceleration on NVIDIA GPUs
-- hl.env("__GL_GSYNC_ALLOWED", "1") -- Controls if G-Sync capable monitors should use Variable Refresh Rate (VRR)

pcall(require, "config.environment_local")
