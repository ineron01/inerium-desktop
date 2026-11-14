# inerium-desktop

Universal CachyOS / Arch Linux desktop environment built around **Hyprland (Lua API)**, **Waybar**, **Rofi**, **Mako**, **SwayOSD**, **Dolphin KIO**, and a native **Rust / GTK4** utility suite themed in the **Inerium Dragon** palette.

## Structure

- `Cargo.toml` — Rust workspace (`inerium-i18n`, `inerium-tools`, `inerium-shell`, `inerium-greeter`, `inerium-setup`, `dolphin-converter`)
- `standalone/inerium-polkit/` — Standalone Polkit agent (fork of [soteria](https://github.com/ImVaskel/soteria), Apache-2.0; `gtk4 0.9` / `relm4 0.9`, kept separate to avoid `links = "gtk-4"` conflict with `gtk4 0.11`)
- `config/` — Symlinkable user configurations (`hypr`, `waybar`, `rofi`, `mako`, `swayosd`, `fish`, `kitty`, `alacritty`, `uwsm`, `systemd/user`, `qt5ct`, `qt6ct`, `gtk-3.0`, `gtk-4.0`)
- `local/` — User scripts (`~/.local/bin/`) and Dolphin KIO service menus
- `system/` — System-wide `inerium-desktop-indexer`, `greetd` templates (`--with-greeter`), and `sudoers.d` rule
- `assets/` — Default wallpapers (`assets/wallpapers/`) and greeter background (`assets/backgrounds/`)
- `pkglist-base.txt` — Base Arch/CachyOS packages
- `bootstrap.sh` — One-command TTY bootstrapper (installs `git`, clones repo, runs `install.sh --with-greeter --fetch-model`)
- `install.sh` — Universal idempotent installer (`--dry-run`, `--skip-pkgs`, `--skip-build`, `--fetch-model`, `--with-greeter`)

## Installation Methods & Quick Start

### 1. One-Line Bootstrap (Recommended)

Works identically from a **fresh TTY** or **inside any existing desktop environment** (KDE Plasma, GNOME, etc.):

**Short URL (via GitHub Pages):**
```bash
curl -sL ineron01.github.io/inerium-desktop | bash
```

**Direct GitHub Raw URL (Alternative / Fallback):**
```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ineron01/inerium-desktop/main/bootstrap.sh)"
```

- **Interactive TUI:** Auto-detects hardware (GPU, laptop battery/chassis, existing Display Manager) and offers quick choices.
- **Fast prebuilt binaries (Default):** Downloads compact release archives (~6 MB) directly from GitHub Releases, skipping heavy Cargo compilation.
- **Smart Fallback:** Automatically falls back to local source build if dynamic shared libraries mismatch.
- **Preserves existing DE:** When installed on top of KDE/GNOME, it sets Inerium (`Hyprland (UWSM-Waybar)`) as the default session in `greetd` while keeping KDE Plasma untouched and available to launch.

### 2. Silent / Automated Run (for scripts or SSH)

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ineron01/inerium-desktop/main/bootstrap.sh)" -- -y
```

### 3. Developer / Source Compilation

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ineron01/inerium-desktop/main/bootstrap.sh)" -- --from-source
```

### 4. Local Run from Cloned Repo

```bash
./install.sh --dry-run
./install.sh --bin --with-greeter --fetch-model
```

## Documentation & User Guide

- **[Hotkeys, Gestures & Waybar Tutorial (Руководство по горячим клавишам)](docs/keybindings.md)** — Полное иллюстрированное руководство по управлению системой, тайлингу окон, шорткатам, жестам тачпада и скрытым возможностям панели Waybar.

## First-login setup

`inerium-first-login` runs one-time steps (account sign-ins and similar) from `~/.config/inerium/first-login.d/` in a floating terminal at the first Hyprland login, remembers completed steps, and re-offers postponed ones on the next login. This repository ships no steps; personal overlays drop executables there (each must exit 0 for `--check` once configured). Re-run anytime from Rofi ("Первичная настройка") or with `inerium-first-login --reset`.

Hardware-specific MSI features (`msi-fan-regulator`) are gated behind the `msi-ec` Cargo feature and disabled in the universal workspace build by default. All panels and utilities perform runtime capability detection (`/sys/devices/platform/msi-ec`, `/sys/class/backlight`, `/sys/class/power_supply/`, `/sys/class/net/`, `envycontrol`) and gracefully hide unavailable controls.
