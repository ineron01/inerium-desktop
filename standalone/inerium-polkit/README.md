# inerium-polkit

Native Rust/GTK4/Relm4 Polkit authentication agent in Inerium Dragon aesthetic.

Based on [soteria](https://github.com/ImVaskel/soteria) by ImVaskel, licensed under the
Apache License 2.0 (see `LICENSE` and `NOTICE`). Modified for Inerium: theming, Hyprland
integration and systemd user service.

## Architecture
- Service: `inerium-polkit.service` in `session.slice` (`systemd --user`).
- D-Bus: Implements `org.freedesktop.PolicyKit1.AuthenticationAgent` via `zbus`.
- Secure memory: zeroizes password buffers with `zeroize`.
- Hyprland: Floating, centered, non-exclusive input grab.

## Build & Deploy
```bash
cargo build --release
cp target/release/inerium-polkit ~/.local/bin/
systemctl --user restart inerium-polkit.service
```
