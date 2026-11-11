# kanagawa-desktop

Universal CachyOS / Arch Linux desktop environment built around **Hyprland (Lua API)**, **Waybar**, **Rofi**, **Mako**, **SwayOSD**, **Dolphin KIO**, and a native **Rust / GTK4** utility suite themed in the **Kanagawa Dragon** palette.

## Structure

- `Cargo.toml` — Rust workspace (`kanagawa-tools`, `kanagawa-shell`, `kanagawa-greeter`, `dolphin-converter`)
- `standalone/kanagawa-polkit/` — Standalone Polkit agent (`gtk4 0.9` / `relm4 0.9`, kept separate to avoid `links = "gtk-4"` conflict with `gtk4 0.11`)
- `config/` — Symlinkable user configurations (`hypr`, `waybar`, `rofi`, `mako`, `swayosd`, `fish`, `kitty`, `alacritty`, `uwsm`, `systemd/user`, `qt5ct`, `qt6ct`, `gtk-3.0`, `gtk-4.0`)
- `local/` — User scripts (`~/.local/bin/`) and Dolphin KIO service menus
- `system/` — System-wide `kanagawa-desktop-indexer`, `greetd` templates (`--with-greeter`), and `sudoers.d` rule
- `assets/` — Default wallpapers (`assets/wallpapers/`) and greeter background (`assets/backgrounds/`)
- `pkglist-base.txt` — Base Arch/CachyOS packages
- `bootstrap.sh` — One-command TTY bootstrapper (installs `git`, clones repo, runs `install.sh --with-greeter --fetch-model`)
- `install.sh` — Universal idempotent installer (`--dry-run`, `--skip-pkgs`, `--skip-build`, `--fetch-model`, `--with-greeter`)

## Quick Start

**Fresh TTY one-line bootstrap (Arch / CachyOS; works in fish and bash):**

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ineron01/kanagawa-desktop/main/bootstrap.sh)"
```

**Local / manual run:**

```bash
./install.sh --dry-run
./install.sh --with-greeter --fetch-model
```

Hardware-specific MSI features (`msi-fan-regulator`) are gated behind the `msi-ec` Cargo feature and disabled in the universal workspace build by default. All panels and utilities perform runtime capability detection (`/sys/devices/platform/msi-ec`, `/sys/class/backlight`, `/sys/class/power_supply/`, `/sys/class/net/`, `envycontrol`) and gracefully hide unavailable controls.
