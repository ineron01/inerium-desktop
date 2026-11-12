-- ============================================================================
-- РЕЗЕРВНАЯ СЕССИЯ HYPRLAND (BACKUP SESSION) — АВТОЗАПУСК СЛУЖБ
-- Этот файл используется при запуске резервной сессии "Hyprland (Резервная)".
-- Он содержит автономную конфигурацию запуска с резервной копией Waybar.
-- ============================================================================

hl.on("hyprland.start", function ()
    local home = os.getenv("HOME") or ""

    -- Установка курсора темы и размера для Hyprland
    hl.exec_cmd("hyprctl setcursor Bibata-Modern-Ice 24")

    -- Окружение в systemd, затем фоновые демоны: попапы, OSD, IPC-мост и буфер обмена
    hl.exec_cmd("sh -c 'dbus-update-activation-environment --systemd --all; systemctl --user start hypr-ipc-bridge.service inerium-osd-watcher.service cliphist.service cliphist-images.service; systemctl --user restart inerium-popups.service'")
    hl.exec_cmd("xhost +SI:localuser:root")

    -- Аутентификация Polkit
    hl.exec_cmd("sh -c '" .. home .. "/.local/bin/inerium-polkit || hyprpolkitagent || /usr/lib/polkit-kde-authentication-agent-1'")

    -- Демон обоев
    hl.exec_cmd(home .. "/.local/bin/hyprpaper")

    -- Демон энергосбережения и блокировки hypridle
    hl.exec_cmd("systemctl --user start hypridle.service")

    -- Уведомления
    hl.exec_cmd("mako")

    -- OSD Индикатор
    hl.exec_cmd("swayosd-server")

    -- Панель Waybar: запуск с резервной конфигурацией и стилями
    hl.exec_cmd("waybar -c " .. home .. "/.config/waybar/config_backup.jsonc -s " .. home .. "/.config/waybar/style_backup.css")

    -- VPN клиент AmneziaVPN (если установлен и не запущен)
    hl.exec_cmd("pgrep -x AmneziaVPN >/dev/null || /opt/AmneziaVPN/bin/AmneziaVPN --autostart")

    -- Индексация приложений для поиска в лаунчере на любых раскладках (QWERTY <-> JCUKEN)
    hl.exec_cmd("sudo -n /usr/local/bin/inerium-desktop-indexer")
end)
