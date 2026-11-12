# inerium-desktop

Universal CachyOS / Arch Linux desktop environment built around **Hyprland (Lua API)**, **Waybar**, **Rofi**, **Mako**, **SwayOSD**, **Dolphin KIO**, and a native **Rust / GTK4** utility suite themed in the **Inerium Dragon** palette.

## Structure

- `Cargo.toml` — Rust workspace (`inerium-tools`, `inerium-shell`, `inerium-greeter`, `dolphin-converter`)
- `standalone/inerium-polkit/` — Standalone Polkit agent (`gtk4 0.9` / `relm4 0.9`, kept separate to avoid `links = "gtk-4"` conflict with `gtk4 0.11`)
- `config/` — Symlinkable user configurations (`hypr`, `waybar`, `rofi`, `mako`, `swayosd`, `fish`, `kitty`, `alacritty`, `uwsm`, `systemd/user`, `qt5ct`, `qt6ct`, `gtk-3.0`, `gtk-4.0`)
- `local/` — User scripts (`~/.local/bin/`) and Dolphin KIO service menus
- `system/` — System-wide `inerium-desktop-indexer`, `greetd` templates (`--with-greeter`), and `sudoers.d` rule
- `assets/` — Default wallpapers (`assets/wallpapers/`) and greeter background (`assets/backgrounds/`)
- `pkglist-base.txt` — Base Arch/CachyOS packages
- `bootstrap.sh` — One-command TTY bootstrapper (installs `git`, clones repo, runs `install.sh --with-greeter --fetch-model`)
- `install.sh` — Universal idempotent installer (`--dry-run`, `--skip-pkgs`, `--skip-build`, `--fetch-model`, `--with-greeter`)

## Quick Start

**Fresh TTY one-line bootstrap (Arch / CachyOS; works in fish and bash):**

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ineron01/inerium-desktop/main/bootstrap.sh)"
```

**Local / manual run:**

```bash
./install.sh --dry-run
./install.sh --with-greeter --fetch-model
```

## First-login setup

`inerium-first-login` runs one-time steps (account sign-ins and similar) from `~/.config/inerium/first-login.d/` in a floating terminal at the first Hyprland login, remembers completed steps, and re-offers postponed ones on the next login. This repository ships no steps; personal overlays drop executables there (each must exit 0 for `--check` once configured). Re-run anytime from Rofi ("Первичная настройка") or with `inerium-first-login --reset`.

Hardware-specific MSI features (`msi-fan-regulator`) are gated behind the `msi-ec` Cargo feature and disabled in the universal workspace build by default. All panels and utilities perform runtime capability detection (`/sys/devices/platform/msi-ec`, `/sys/class/backlight`, `/sys/class/power_supply/`, `/sys/class/net/`, `envycontrol`) and gracefully hide unavailable controls.
