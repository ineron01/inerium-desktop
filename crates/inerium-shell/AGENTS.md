# Inerium Shell (GTK4 Layer-Shell Daemon) Guidelines

## Architecture
- GTK4 Layer-Shell daemon managing popups (Control Center, Audio, Network, BT, Media, Power).
- IPC Client: `inerium-popup` sends commands via Unix domain socket.
- Service: Managed via `inerium-shell.service` (`systemctl --user`).

## Engineering Guidelines
1. **SafeZone & Dismissal:** Retain pointer motion SafeZone tracking to ensure smooth dismissal without flickering.
2. **Dimension Constraints:** Popups must enforce maximum heights with `GtkScrolledWindow` to prevent stretching outside display bounds on high device counts.
3. **Quality & Validation:**
   ```bash
   cargo check --workspace
   cargo clippy --workspace -- -D warnings
   ```
4. **Reload Procedure:** Deploy binary to `~/.local/bin/inerium-shell` and run `systemctl --user restart inerium-shell`.
