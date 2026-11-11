# kanagawa-tools

High-performance native Rust micro-utilities and telemetry data providers for Waybar and desktop automation.

## Binaries
- `msi-fan-regulator`: EC RAM cooling manager for MSI Bravo 15 C7VE.
- `kanagawa-battery`: BAT1/ADP1 battery monitor & toggle (`--waybar`, `--toggle`, `--tooltip`).
- `kanagawa-clock`: Clock & interactive calendar (`--waybar`, `--toggle`, `--next-month`, `--prev-month`).
- `kanagawa-hwmon`: AMD `k10temp` CPU & RAM monitor (`--plain`, `--waybar`).
- `kanagawa-layout`: Hyprland keyboard layout tracker & switcher (`--waybar`, `--switch`).
- `kanagawa-network`: Direct kernel `getifaddrs()` network status (`--waybar`).
- `kanagawa-bluetooth`: Bluetooth status & toggle (`--waybar`).
- `kanagawa-hotspot`: Wi-Fi AP controller (`status`, `status-plain`, `toggle`).
- `kanagawa-clipboard`: Native Cliphist manager with ImageMagick thumbs.
- `kanagawa-dnd`: Mako DND & WirePlumber mute controller (`--waybar`, `--toggle`).
- `kanagawa-updates`: Arch & AUR update checker.
- `kanagawa-popup`: IPC client for `kanagawa-shell` daemon.
- `hypr-ipc-bridge`: Hyprland 0.56+ Lua workspace compatibility bridge.
- `kanagawa-osd-watcher`: SwayOSD layout & inotify webcam watcher.

## Build & Install
```bash
cargo build --release
cp target/release/hypr-ipc-bridge target/release/kanagawa-* ~/.local/bin/
sudo cp target/release/msi-fan-regulator /usr/local/bin/
sudo systemctl restart msi-fan-regulator.service
```
