#!/bin/bash
# Smart Network Toggle for Waybar RMB
# If any wired Ethernet cable is connected (carrier == 1), toggle that interface.
# If no cable, toggle Wi-Fi radio.

ETH_IF=""
for dev in /sys/class/net/en* /sys/class/net/eth*; do
    [ -e "$dev" ] || continue
    if [ -f "$dev/carrier" ] && [ "$(cat "$dev/carrier" 2>/dev/null)" = "1" ]; then
        ETH_IF=$(basename "$dev")
        break
    fi
done

if [ -n "$ETH_IF" ]; then
    STATE=$(nmcli -t -f DEVICE,STATE dev | grep "^${ETH_IF}:" | cut -d: -f2)
    if [ "$STATE" = "connected" ]; then
        nmcli dev disconnect "$ETH_IF"
        notify-send -a "Сеть" -i network-offline "Сеть Ethernet" "Кабель подключен, интерфейс $ETH_IF отключен"
    else
        nmcli dev connect "$ETH_IF"
        notify-send -a "Сеть" -i network-transmit-receive "Сеть Ethernet" "Интерфейс $ETH_IF подключен"
    fi
else
    WIFI_STATUS=$(nmcli radio wifi)
    if [ "$WIFI_STATUS" = "enabled" ]; then
        nmcli radio wifi off
        notify-send -a "Сеть" -i network-wireless-offline "Беспроводная сеть" "Wi-Fi модуль выключен"
    else
        nmcli radio wifi on
        notify-send -a "Сеть" -i network-wireless-signal-excellent "Беспроводная сеть" "Wi-Fi модуль включен"
    fi
fi
