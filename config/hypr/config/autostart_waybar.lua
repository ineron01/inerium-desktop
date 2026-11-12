-- ============================================================================
-- АВТОСТАРТ HYPRLAND (WAYBAR ПРОФИЛЬ ПОД УПРАВЛЕНИЕМ UWSM)
-- Все фоновые службы запускаются через systemd --user при активации target'а.
-- ============================================================================

hl.on("hyprland.start", function ()
    -- 1. Сигнал UWSM: композитор готов, экспорт переменных, старт graphical-session.target
    hl.exec_cmd("uwsm finalize HYPRLAND_INSTANCE_SIGNATURE")

    -- 2. Аппаратная синхронизация курсора в буфере Hyprland
    hl.exec_cmd("hyprctl setcursor Bibata-Modern-Ice 24")

    -- 3. Доступ учетной записи root к дисплею XWayland (GParted и утилиты разделов)
    hl.exec_cmd("xhost +SI:localuser:root")

    -- 4. Фоновая мультиязычная индексация приложений (oneshot)
    hl.exec_cmd("sudo -n /usr/local/bin/inerium-desktop-indexer")

    -- 5. Первичная настройка окружения и интеграций; если всё пройдено — выходит сразу
    hl.exec_cmd("inerium-setup --autostart")
end)
