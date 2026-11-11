#!/usr/bin/env bash
# ============================================================================
# kanagawa-desktop/install.sh — Universal Arch/CachyOS Hyprland + Rust Installer
# ============================================================================
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY_RUN=0
SKIP_PKGS=0
SKIP_BUILD=0
FETCH_MODEL=0
WITH_GREETER=0
BACKUP_STAMP="$(date +%Y%m%d%H%M%S)"

SILUETA_URL="https://github.com/danielgatis/rembg/releases/download/v0.0.0/silueta.onnx"
SILUETA_PATH="${XDG_DATA_HOME:-$HOME/.local/share}/dolphin-converter/models/silueta.onnx"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --dry-run       Print planned actions without modifying the system
  --skip-pkgs     Skip pacman/paru package installation
  --skip-build    Skip Cargo workspace and kanagawa-polkit compilation
  --fetch-model   Download silueta.onnx (~43 MB) if missing
  --with-greeter  Install and configure greetd + kanagawa-greeter (/etc/greetd/)
  -h, --help      Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)      DRY_RUN=1; shift ;;
        --skip-pkgs)    SKIP_PKGS=1; shift ;;
        --skip-build)   SKIP_BUILD=1; shift ;;
        --fetch-model)  FETCH_MODEL=1; shift ;;
        --with-greeter) WITH_GREETER=1; shift ;;
        -h|--help)      usage; exit 0 ;;
        *)              echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
done

log() { printf '[kanagawa-desktop] %s\n' "$*"; }

run() {
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] %s\n' "$*"
    else
        eval "$@"
    fi
}

link_item() {
    local src="$1"
    local dst="$2"
    if [[ "$DRY_RUN" -eq 1 ]]; then
        if [[ -e "$dst" && ! -L "$dst" ]]; then
            printf '[DRY-RUN] mv %s -> %s.bak.%s\n' "$dst" "$dst" "$BACKUP_STAMP"
        fi
        printf '[DRY-RUN] ln -sfn %s -> %s\n' "$src" "$dst"
        return
    fi
    mkdir -p "$(dirname "$dst")"
    if [[ -L "$dst" ]]; then
        if [[ "$(readlink -f "$dst" 2>/dev/null || true)" == "$(readlink -f "$src" 2>/dev/null || true)" ]]; then
            return
        fi
    elif [[ -e "$dst" ]]; then
        local bak="${dst}.bak.${BACKUP_STAMP}"
        log "Backing up existing target: $dst -> $bak"
        mv "$dst" "$bak"
    fi
    ln -sfn "$src" "$dst"
}

render_template() {
    local tpl="$1"
    local dst="$2"
    local only_if_missing="${3:-0}"
    if [[ "$only_if_missing" -eq 1 && -e "$dst" ]]; then
        log "Keeping existing mutable file: $dst"
        return
    fi
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] render %s -> %s\n' "$tpl" "$dst"
        return
    fi
    mkdir -p "$(dirname "$dst")"
    sed -e "s|__HOME__|$HOME|g" -e "s|__USER__|$USER|g" "$tpl" > "${dst}.tmp"
    mv -f "${dst}.tmp" "$dst"
}

install_root_if_changed() {
    local mode="$1"
    local src="$2"
    local dst="$3"
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] sudo install -m %s %s -> %s\n' "$mode" "$src" "$dst"
        return
    fi
    if ! cmp -s "$src" "$dst" 2>/dev/null; then
        sudo install -d -m 755 "$(dirname "$dst")"
        sudo install -m "$mode" "$src" "$dst"
    fi
}

install_sudoers_template() {
    local tpl="$1"
    local dst="$2"
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] render sudoers %s -> %s (visudo -cf)\n' "$tpl" "$dst"
        return
    fi
    local tmp
    tmp="$(mktemp)"
    sed -e "s|__HOME__|$HOME|g" -e "s|__USER__|$USER|g" "$tpl" > "$tmp"
    if sudo cmp -s "$tmp" "$dst" 2>/dev/null; then
        rm -f "$tmp"
        return
    fi
    if sudo visudo -cf "$tmp" >/dev/null 2>&1; then
        sudo install -m 0440 "$tmp" "$dst"
    else
        log "WARNING: Invalid sudoers syntax in $tpl; skipped $dst"
    fi
    rm -f "$tmp"
}

# 1. Install base packages
if [[ "$SKIP_PKGS" -eq 0 ]]; then
    log "Installing base packages from pkglist-base.txt..."
    if ! command -v paru >/dev/null 2>&1 && ! command -v yay >/dev/null 2>&1; then
        if pacman -Si paru >/dev/null 2>&1; then
            run "sudo pacman -Sy --needed --noconfirm paru"
        fi
    fi
    PKGS=$(grep -vE '^\s*(#|$)' "$REPO_DIR/pkglist-base.txt" | tr '\n' ' ')
    if command -v paru >/dev/null 2>&1; then
        run "paru -Sy --needed --noconfirm $PKGS"
    elif command -v yay >/dev/null 2>&1; then
        run "yay -Sy --needed --noconfirm $PKGS"
    else
        run "sudo pacman -Sy --needed --noconfirm $PKGS"
    fi
    if [[ ! -d /usr/share/icons/Bibata-Modern-Ice && ! -d "$HOME/.local/share/icons/Bibata-Modern-Ice" ]]; then
        if command -v paru >/dev/null 2>&1; then
            run "paru -S --needed --noconfirm bibata-cursor-theme-bin || true"
        elif command -v yay >/dev/null 2>&1; then
            run "yay -S --needed --noconfirm bibata-cursor-theme-bin || true"
        else
            log "NOTE: Bibata-Modern-Ice cursor theme not found (AUR: bibata-cursor-theme-bin)."
        fi
    fi
    for svc in NetworkManager.service bluetooth.service power-profiles-daemon.service; do
        if systemctl list-unit-files "$svc" >/dev/null 2>&1; then
            run "sudo systemctl enable '$svc' >/dev/null 2>&1 || true"
        fi
    done
fi

# 2. Build Rust workspace + standalone kanagawa-polkit
if [[ "$SKIP_BUILD" -eq 0 ]]; then
    log "Building universal Rust workspace (kanagawa-tools, kanagawa-shell, kanagawa-greeter, dolphin-converter)..."
    run "cargo build --manifest-path '$REPO_DIR/Cargo.toml' --workspace --release"

    log "Building standalone kanagawa-polkit..."
    run "cargo build --manifest-path '$REPO_DIR/standalone/kanagawa-polkit/Cargo.toml' --release"

    log "Installing compiled binaries atomically..."
    run "mkdir -p '$HOME/.local/bin'"
    for bin in \
        hypr-ipc-bridge kanagawa-battery kanagawa-bluetooth kanagawa-clipboard \
        kanagawa-clock kanagawa-dnd kanagawa-hotspot kanagawa-hwmon \
        kanagawa-layout kanagawa-network kanagawa-osd-watcher kanagawa-popup \
        kanagawa-updates kanagawa-shell dolphin-convert; do
        run "install -m 755 '$REPO_DIR/target/release/$bin' '$HOME/.local/bin/$bin'"
    done
    run "install -m 755 '$REPO_DIR/standalone/kanagawa-polkit/target/release/kanagawa-polkit' '$HOME/.local/bin/kanagawa-polkit'"
    install_root_if_changed 755 "$REPO_DIR/target/release/kanagawa-greeter" /usr/local/bin/kanagawa-greeter
fi

# 3. Install user scripts & system indexer
log "Deploying user scripts & KIO service menus..."
run "mkdir -p '$HOME/.local/bin' '$HOME/.local/share/kio/servicemenus'"
for script in "$REPO_DIR"/local/bin/*; do
    name="$(basename "$script")"
    link_item "$script" "$HOME/.local/bin/$name"
done
for desktop in "$REPO_DIR"/local/share/kio/servicemenus/*.desktop; do
    name="$(basename "$desktop")"
    link_item "$desktop" "$HOME/.local/share/kio/servicemenus/$name"
done
install_root_if_changed 755 "$REPO_DIR/system/usr/local/bin/kanagawa-desktop-indexer" /usr/local/bin/kanagawa-desktop-indexer
install_sudoers_template "$REPO_DIR/system/etc/sudoers.d/kanagawa-desktop-indexer.template" /etc/sudoers.d/kanagawa-desktop-indexer

# 4. Symlink ~/.config directories and Hyprland files
log "Linking desktop configurations into ~/.config..."
for dir in alacritty environment.d gtk-3.0 gtk-4.0 kitty mako rofi uwsm waybar; do
    link_item "$REPO_DIR/config/$dir" "$HOME/.config/$dir"
done
link_item "$REPO_DIR/config/btop/btop.conf" "$HOME/.config/btop/btop.conf"

# Fish (preserve runtime fish_variables)
run "mkdir -p '$HOME/.config/fish/functions'"
link_item "$REPO_DIR/config/fish/config.fish" "$HOME/.config/fish/config.fish"
link_item "$REPO_DIR/config/fish/functions/code.fish" "$HOME/.config/fish/functions/code.fish"

# Qt5/Qt6 & SwayOSD
link_item "$REPO_DIR/config/qt5ct/colors" "$HOME/.config/qt5ct/colors"
link_item "$REPO_DIR/config/qt6ct/colors" "$HOME/.config/qt6ct/colors"
link_item "$REPO_DIR/config/swayosd/style.css" "$HOME/.config/swayosd/style.css"
render_template "$REPO_DIR/config/qt5ct/qt5ct.conf.template" "$HOME/.config/qt5ct/qt5ct.conf"
render_template "$REPO_DIR/config/qt6ct/qt6ct.conf.template" "$HOME/.config/qt6ct/qt6ct.conf"
render_template "$REPO_DIR/config/swayosd/config.toml.template" "$HOME/.config/swayosd/config.toml"

# Hyprland (file-level links so mutable hyprpaper.conf, profile, and monitors_local.lua coexist)
run "mkdir -p '$HOME/.config/hypr/config'"
for f in AGENTS.md hypridle.conf hyprland.lua hyprlock.conf hyprtoolkit.conf xdph.conf; do
    link_item "$REPO_DIR/config/hypr/$f" "$HOME/.config/hypr/$f"
done
for f in "$REPO_DIR"/config/hypr/config/*.lua "$REPO_DIR"/config/hypr/config/theme_settings.json; do
    base="$(basename "$f")"
    if [[ ( "$base" == "autostart_backup.lua" || "$base" == "binds_backup.lua" ) && -e "$HOME/.config/hypr/config/$base" ]]; then
        continue
    fi
    link_item "$f" "$HOME/.config/hypr/config/$base"
done
render_template "$REPO_DIR/config/hypr/hyprpaper.conf.template" "$HOME/.config/hypr/hyprpaper.conf" 1
render_template "$REPO_DIR/config/hypr/profile.default" "$HOME/.config/hypr/profile" 1

# Ensure XDG user dirs & default wallpapers exist (non-destructive)
if command -v xdg-user-dirs-update >/dev/null 2>&1 && [[ ! -d "$HOME/Pictures" ]]; then
    run "xdg-user-dirs-update"
fi
run "mkdir -p '$HOME/Pictures/Wallpapers'"
for wall in "$REPO_DIR"/assets/wallpapers/*; do
    [[ -f "$wall" ]] || continue
    wname="$(basename "$wall")"
    if [[ ! -e "$HOME/Pictures/Wallpapers/$wname" ]]; then
        run "install -m 644 '$wall' '$HOME/Pictures/Wallpapers/$wname'"
    fi
done

# Systemd user units
run "mkdir -p '$HOME/.config/systemd/user/hypridle.service.d' '$HOME/.config/systemd/user/mako.service.d' '$HOME/.config/systemd/user/waybar.service.d'"
for unit in "$REPO_DIR"/config/systemd/user/*.service "$REPO_DIR"/config/systemd/user/*.timer "$REPO_DIR"/config/systemd/user/*.path; do
    link_item "$unit" "$HOME/.config/systemd/user/$(basename "$unit")"
done
link_item "$REPO_DIR/config/systemd/user/hypridle.service.d/slice.conf" "$HOME/.config/systemd/user/hypridle.service.d/slice.conf"
link_item "$REPO_DIR/config/systemd/user/mako.service.d/slice.conf" "$HOME/.config/systemd/user/mako.service.d/slice.conf"
link_item "$REPO_DIR/config/systemd/user/waybar.service.d/bridge.conf" "$HOME/.config/systemd/user/waybar.service.d/bridge.conf"
link_item "$REPO_DIR/config/systemd/user/waybar.service.d/slice.conf" "$HOME/.config/systemd/user/waybar.service.d/slice.conf"

# 5. Check / fetch ONNX background removal model (silueta.onnx)
if [[ -s "$SILUETA_PATH" ]]; then
    log "Found silueta.onnx at $SILUETA_PATH"
elif [[ "$FETCH_MODEL" -eq 1 ]]; then
    log "Downloading silueta.onnx (~43 MB)..."
    run "mkdir -p '$(dirname "$SILUETA_PATH")'"
    run "curl -fsSL '$SILUETA_URL' -o '$SILUETA_PATH'"
else
    log "NOTE: silueta.onnx not found at $SILUETA_PATH."
    log "      Run with --fetch-model (or download from $SILUETA_URL) for Dolphin AI background removal."
fi

# 6. Wayland session entries & optional greetd + kanagawa-greeter setup
for sess in "$REPO_DIR"/system/usr/share/wayland-sessions/*.desktop; do
    [[ -f "$sess" ]] || continue
    install_root_if_changed 644 "$sess" "/usr/share/wayland-sessions/$(basename "$sess")"
done

if [[ "$WITH_GREETER" -eq 1 ]]; then
    log "Configuring greetd + kanagawa-greeter..."
    if [[ "$SKIP_PKGS" -eq 0 ]]; then
        if command -v paru >/dev/null 2>&1; then
            run "paru -S --needed --noconfirm greetd cage"
        elif command -v yay >/dev/null 2>&1; then
            run "yay -S --needed --noconfirm greetd cage"
        else
            run "sudo pacman -S --needed --noconfirm greetd cage"
        fi
    fi
    install_root_if_changed 644 "$REPO_DIR/assets/backgrounds/kanagawa-greeter.jpg" /usr/share/backgrounds/kanagawa-greeter.jpg
    install_root_if_changed 644 "$REPO_DIR/system/etc/greetd/config.toml" /etc/greetd/config.toml
    install_root_if_changed 644 "$REPO_DIR/system/etc/greetd/environments" /etc/greetd/environments
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] render %s -> /etc/greetd/kanagawa-greeter.json\n' "$REPO_DIR/system/etc/greetd/kanagawa-greeter.json.template"
    else
        greeter_tmp="$(mktemp)"
        sed -e "s|__HOME__|$HOME|g" -e "s|__USER__|$USER|g" "$REPO_DIR/system/etc/greetd/kanagawa-greeter.json.template" > "$greeter_tmp"
        install_root_if_changed 644 "$greeter_tmp" /etc/greetd/kanagawa-greeter.json
        rm -f "$greeter_tmp"
    fi
    run "sudo systemctl enable greetd.service"
fi

# 7. Enable systemd user units (live D-Bus or offline TTY symlink fallback) & rebuild KDE cache
USER_UNITS="cliphist.service cliphist-images.service hypr-ipc-bridge.service hyprpaper.service kanagawa-desktop-indexer.path kanagawa-osd-watcher.service kanagawa-polkit.service kanagawa-popups.service kanagawa-updates.timer swayosd-server.service"
MASK_UNITS="hyprpolkitagent.service at-spi2-registryd.service at-spi-dbus-bus.service"
if systemctl --user show-environment >/dev/null 2>&1; then
    log "Refreshing systemd user daemon & enabling user services..."
    run "systemctl --user daemon-reload"
    run "systemctl --user mask $MASK_UNITS >/dev/null 2>&1 || true"
    run "systemctl --user enable $USER_UNITS"
else
    log "No active systemd --user D-Bus session (TTY/chroot); creating offline .wants/mask links..."
    run "mkdir -p '$HOME/.config/systemd/user/default.target.wants' '$HOME/.config/systemd/user/timers.target.wants' '$HOME/.config/systemd/user/paths.target.wants'"
    for m in $MASK_UNITS; do
        run "ln -sfn /dev/null '$HOME/.config/systemd/user/$m'"
    done
    for u in $USER_UNITS; do
        case "$u" in
            *.timer) run "ln -sfn '$HOME/.config/systemd/user/$u' '$HOME/.config/systemd/user/timers.target.wants/$u'" ;;
            *.path)  run "ln -sfn '$HOME/.config/systemd/user/$u' '$HOME/.config/systemd/user/paths.target.wants/$u'" ;;
            *)       run "ln -sfn '$HOME/.config/systemd/user/$u' '$HOME/.config/systemd/user/default.target.wants/$u'" ;;
        esac
    done
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    run "kbuildsycoca6 --noincremental >/dev/null 2>&1 || true"
fi

log "Base kanagawa-desktop installation complete!"
