# inerium-tools

High-performance native Rust micro-utilities and telemetry data providers for Waybar and desktop automation.

## Binaries
- `msi-fan-regulator`: EC RAM cooling manager for MSI Bravo 15 C7VE.
- `inerium-battery`: BAT1/ADP1 battery monitor & toggle (`--waybar`, `--toggle`, `--tooltip`).
- `inerium-clock`: Clock & interactive calendar (`--waybar`, `--toggle`, `--next-month`, `--prev-month`).
- `inerium-hwmon`: AMD `k10temp` CPU & RAM monitor (`--plain`, `--waybar`).
- `inerium-layout`: Hyprland keyboard layout tracker & switcher (`--waybar`, `--switch`).
- `inerium-network`: Direct kernel `getifaddrs()` network status (`--waybar`).
- `inerium-bluetooth`: Bluetooth status & toggle (`--waybar`).
- `inerium-hotspot`: Wi-Fi AP controller (`status`, `status-plain`, `toggle`).
- `inerium-clipboard`: Native Cliphist manager with ImageMagick thumbs.
- `inerium-dnd`: Mako DND & WirePlumber mute controller (`--waybar`, `--toggle`).
- `inerium-updates`: Arch & AUR update checker.
- `inerium-popup`: IPC client for `inerium-shell` daemon.
- `hypr-ipc-bridge`: Hyprland 0.56+ Lua workspace compatibility bridge.
- `inerium-osd-watcher`: SwayOSD layout & inotify webcam watcher.

## Build & Install
```bash
cargo build --release
cp target/release/hypr-ipc-bridge target/release/inerium-* ~/.local/bin/
sudo cp target/release/msi-fan-regulator /usr/local/bin/
sudo systemctl restart msi-fan-regulator.service
```
