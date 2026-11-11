# kanagawa-polkit

Native Rust/GTK4/Relm4 Polkit authentication agent in Kanagawa Dragon aesthetic.

## Architecture
- Service: `kanagawa-polkit.service` in `session.slice` (`systemd --user`).
- D-Bus: Implements `org.freedesktop.PolicyKit1.AuthenticationAgent` via `zbus`.
- Secure memory: zeroizes password buffers with `zeroize`.
- Hyprland: Floating, centered, non-exclusive input grab.

## Build & Deploy
```bash
cargo build --release
cp target/release/kanagawa-polkit ~/.local/bin/
systemctl --user restart kanagawa-polkit.service
```
