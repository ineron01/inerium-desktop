#!/bin/bash
# Toggle Bluetooth power and notify with rfkill support
set -uo pipefail

STATUS=$(bluetoothctl show 2>/dev/null | grep "Powered: yes")
if [ -n "$STATUS" ]; then
    bluetoothctl power off >/dev/null 2>&1 || true
    notify-send -a "Bluetooth" -i bluetooth-disabled "Bluetooth" "Модуль Bluetooth выключен"
else
    rfkill unblock bluetooth >/dev/null 2>&1 || true
    for _ in {1..10}; do
        if bluetoothctl show 2>/dev/null | grep -q "Powered: yes"; then
            break
        fi
        bluetoothctl power on >/dev/null 2>&1 || true
        sleep 0.1
    done
    notify-send -a "Bluetooth" -i bluetooth-active "Bluetooth" "Модуль Bluetooth включен"
fi

pkill -SIGUSR1 kanagawa-bluetooth 2>/dev/null || true
