# Inerium Tools (Waybar Native Providers) Guidelines

## Architecture & Codebase
- Crate Type: Cargo workspace targeting lightweight native telemetry binaries.
- Core Library: `libinerium_tools` (shared telemetry, IPC, Waybar formatters).
- Target Binaries: `inerium-battery`, `inerium-clock`, `inerium-dnd`, `inerium-updates`, `inerium-hotspot`, `inerium-clipboard`.

## Engineering & Output Protocol
1. **Waybar JSON Schema:** Every `--waybar` output MUST be valid compact JSON adhering to:
   `{"text": "...", "tooltip": "...", "class": "...", "percentage": 0}`
2. **Non-blocking Execution:** Never perform unbounded I/O, heavy polling, or sleeping on the main execution thread. Use async tasks or event sockets.
3. **Quality & Lint Gate:** Always ensure clean compilation before deploy:
   ```bash
   cargo check --workspace
   cargo clippy --workspace -- -D warnings
   ```
4. **Deployment:** Atomically copy compiled release binaries into `~/.local/bin/`.
