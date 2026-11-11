# kanagawa-shell

Native Rust GTK4 Layer-Shell popup daemon and Control Center for Hyprland.

## Architecture
- **Daemon:** `~/.local/bin/kanagawa-shell --daemon`
- **Socket:** `/run/user/<uid>/kanagawa-popups.sock`
- **Service:** `kanagawa-popups.service` (`systemd --user`)
- **Key Modules:**
  - `hw.rs`: Async DDC/CI worker, MSI EC sysfs, PipeWire volume, EnvyControl.
  - `views/`: `control_center`, `audio`, `network`, `bluetooth`, `media`, `power`, `wallpaper`.
  - Cursor SafeZone (80 ms timer, 65 px margin).
  - Batch IPC querying cursor position and monitor geometries.

## Build & Deploy
```bash
cargo build --release
cp target/release/kanagawa-shell ~/.local/bin/
systemctl --user restart kanagawa-popups.service
```
