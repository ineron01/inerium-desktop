local mainMod = "SUPER"
local home = os.getenv("HOME") or ""

local function digitCode(d)
    return "code:" .. (d == 0 and 19 or (9 + d))
end

--------------------------------
---- WAYBAR / ROFI ACTIONS -----
--------------------------------

-- Меню приложений и окон (с поддержкой локального оверлея при конфликте с Super+Space)
local binds_local_ok, binds_local = pcall(require, "config.binds_local")
if not (binds_local_ok and type(binds_local) == "table" and binds_local.launcher_bound) then
    hl.bind(mainMod .. " + Space",   hl.dsp.exec_cmd("uwsm app -- rofi -show drun"))
end

-- Центр управления
hl.bind(mainMod .. " + A",       hl.dsp.exec_cmd(home .. "/.local/bin/inerium-popup control"))

-- Переключение окон
hl.bind(mainMod .. " + Tab",     hl.dsp.exec_cmd("uwsm app -- rofi -show window"))

-- Буфер обмена
hl.bind(mainMod .. " + V",       hl.dsp.exec_cmd("uwsm app -- " .. home .. "/.local/bin/inerium-clipboard"))

-- Блокировка и меню управления питанием
hl.bind(mainMod .. " + L",       hl.dsp.exec_cmd("hyprlock || loginctl lock-session"))
hl.bind(mainMod .. " + ALT + C", hl.dsp.exec_cmd(home .. "/.local/bin/inerium-popup power"))
hl.bind("XF86PowerOff",          hl.dsp.exec_cmd(home .. "/.local/bin/inerium-popup power"))

-------------------------------
---- WORKSPACES NAVIGATION ----
-------------------------------

-- Прямое переключение на воркспейсы 1-6 (1-3 на ноутбуке, 4-6 на внешнем мониторе)
for i = 1, 6 do
    local digit = digitCode(i)
    hl.bind(mainMod .. " + " .. digit, hl.dsp.focus({ workspace = i }))
    hl.bind(mainMod .. " + SHIFT + " .. digit, hl.dsp.window.move({ workspace = i }))
    hl.bind(mainMod .. " + ALT + " .. digit, hl.dsp.window.move({ workspace = i, follow = false }))
end

---------------------------
---- HARDWARE CONTROLS ----
---------------------------

-- Звук
hl.bind("XF86AudioRaiseVolume", hl.dsp.exec_cmd("swayosd-client --output-volume raise"), { locked = true, repeating = true })
hl.bind("XF86AudioLowerVolume", hl.dsp.exec_cmd("swayosd-client --output-volume lower"), { locked = true, repeating = true })
hl.bind("XF86AudioMute",        hl.dsp.exec_cmd("swayosd-client --output-volume mute-toggle"), { locked = true })
hl.bind("XF86AudioMicMute",     hl.dsp.exec_cmd("swayosd-client --input-volume mute-toggle"),  { locked = true })

-- Медиа
hl.bind("XF86AudioPlay",  hl.dsp.exec_cmd("playerctl play-pause"), { locked = true })
hl.bind("XF86AudioPause", hl.dsp.exec_cmd("playerctl play-pause"), { locked = true })
hl.bind("XF86AudioNext",  hl.dsp.exec_cmd("playerctl next"),       { locked = true })
hl.bind("XF86AudioPrev",  hl.dsp.exec_cmd("playerctl previous"),   { locked = true })

-- Яркость
hl.bind("XF86MonBrightnessUp",   hl.dsp.exec_cmd("swayosd-client --brightness raise"), { locked = true, repeating = true })
hl.bind("XF86MonBrightnessDown", hl.dsp.exec_cmd("swayosd-client --brightness lower"), { locked = true, repeating = true })

-------------------
---- UTILITIES ----
-------------------

-- Пипетка цвета
hl.bind(mainMod .. " + P", hl.dsp.exec_cmd("hyprpicker -a -n"))

-- Скриншоты
hl.bind("Print",           hl.dsp.exec_cmd("uwsm app -- " .. home .. "/.local/bin/hypr-screenshot area"))
hl.bind("SHIFT + Print",   hl.dsp.exec_cmd("uwsm app -- " .. home .. "/.local/bin/hypr-screenshot fullscreen"))
