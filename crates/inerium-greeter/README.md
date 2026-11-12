# inerium-greeter

Native Rust/GTK4 login screen greeter for `greetd` and `cage` Wayland kiosk.

## Installation
- Binary: `/usr/local/bin/inerium-greeter`
- Config: `/etc/greetd/config.toml`

## Build
```bash
cargo build --release
sudo cp target/release/inerium-greeter /usr/local/bin/
sudo chmod 755 /usr/local/bin/inerium-greeter
```
