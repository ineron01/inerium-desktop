#!/usr/bin/env bash
# ============================================================================
# inerium-desktop/install.sh — Universal Arch/CachyOS Hyprland + Rust Installer
# ============================================================================
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY_RUN=0
SKIP_PKGS=0
SKIP_BUILD=0
FETCH_MODEL=0
WITH_GREETER=0
NON_INTERACTIVE="${INERIUM_NON_INTERACTIVE:-0}"
INSTALL_MODE="bin"
MODE_EXPLICIT=0
WORKSPACES_PER_MON=""
BACKUP_STAMP="$(date +%Y%m%d%H%M%S)"
LOG_FILE="/tmp/inerium-install.log"

RELEASE_BASE_URL="${INERIUM_RELEASE_URL:-https://github.com/ineron01/inerium-desktop/releases/latest/download}"
BIN_ARCHIVE="inerium-binaries-linux-x86_64.tar.gz"
BIN_SHA="inerium-binaries-linux-x86_64.tar.gz.sha256"

SILUETA_URL="https://github.com/danielgatis/rembg/releases/download/v0.0.0/silueta.onnx"
SILUETA_PATH="${XDG_DATA_HOME:-$HOME/.local/share}/dolphin-converter/models/silueta.onnx"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --bin              Install prebuilt binaries from GitHub Releases (default)
  --from-source      Compile Rust workspace and inerium-polkit locally via Cargo
  --dry-run          Print planned actions without modifying the system
  --skip-pkgs        Skip paru package installation
  --skip-build       Skip binary download and Cargo compilation
  --fetch-model      Download silueta.onnx (~43 MB) if missing
  --with-greeter     Install and configure greetd + inerium-greeter (/etc/greetd/)
  --workspaces N     Workspaces per monitor (1-9, default: 3)
  --non-interactive  Run without interactive TUI prompts (also -y)
  -h, --help         Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin)
            if [[ "$MODE_EXPLICIT" -eq 1 && "$INSTALL_MODE" == "source" ]]; then
                echo "Cannot combine --bin and --from-source." >&2; exit 1
            fi
            INSTALL_MODE="bin"; MODE_EXPLICIT=1; shift ;;
        --from-source)
            if [[ "$MODE_EXPLICIT" -eq 1 && "$INSTALL_MODE" == "bin" ]]; then
                echo "Cannot combine --bin and --from-source." >&2; exit 1
            fi
            INSTALL_MODE="source"; MODE_EXPLICIT=1; shift ;;
        --dry-run)         DRY_RUN=1; NON_INTERACTIVE=1; shift ;;
        --skip-pkgs)       SKIP_PKGS=1; shift ;;
        --skip-build)      SKIP_BUILD=1; shift ;;
        --fetch-model)     FETCH_MODEL=1; shift ;;
        --with-greeter)    WITH_GREETER=1; shift ;;
        --workspaces)      WORKSPACES_PER_MON="${2:-}"; shift 2 ;;
        --non-interactive|-y) NON_INTERACTIVE=1; shift ;;
        -h|--help)         usage; exit 0 ;;
        *)                 echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
done

if [[ -n "$WORKSPACES_PER_MON" && ! "$WORKSPACES_PER_MON" =~ ^[1-9]$ ]]; then
    echo "Invalid --workspaces value '$WORKSPACES_PER_MON' (expected 1-9)." >&2
    exit 1
fi

log()  { printf '[inerium-desktop] %s\n' "$*"; }
warn() { printf '[inerium-desktop] WARNING: %s\n' "$*" >&2; }

has_gum() {
    command -v gum >/dev/null 2>&1 && [[ -t 0 && -t 1 ]]
}

use_tui() {
    [[ "$DRY_RUN" -eq 0 && "$NON_INTERACTIVE" -eq 0 ]] && has_gum
}

if [[ "$EUID" -eq 0 ]]; then
    echo "Run install.sh as your regular user (not root/sudo); it calls sudo itself." >&2
    exit 1
fi

if [[ "$DRY_RUN" -eq 0 ]]; then
    : > "$LOG_FILE"
fi

# Ask for the sudo password once and keep the timestamp fresh, so long AUR/Cargo builds
# never stall on a second prompt (a nested installer reuses the parent's keepalive).
if [[ "$DRY_RUN" -eq 0 && -z "${INERIUM_SUDO_KEEPALIVE:-}" ]]; then
    if ! sudo -n true 2>/dev/null; then
        sudo -v
    fi
    ( while kill -0 "$$" 2>/dev/null; do sudo -n -v 2>/dev/null; sleep 50; done ) &
    export INERIUM_SUDO_KEEPALIVE=$!
    trap 'kill "$INERIUM_SUDO_KEEPALIVE" 2>/dev/null || true' EXIT
fi

run() {
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] %s\n' "$*"
    else
        eval "$@"
    fi
}

run_step() {
    local title="$1"
    shift
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] %s: %s\n' "$title" "$*"
        return 0
    fi
    if use_tui; then
        printf '\n=== %s (%s) ===\n' "$title" "$(date '+%H:%M:%S')" >> "$LOG_FILE"
        if ! gum spin --spinner dot --title "$title" -- bash -c '"$@" >> "$0" 2>&1' "$LOG_FILE" "$@"; then
            printf '\n[inerium-desktop] ERROR during: %s\n' "$title" >&2
            printf '[inerium-desktop] Last 25 lines of %s:\n' "$LOG_FILE" >&2
            tail -n 25 "$LOG_FILE" >&2 || true
            return 1
        fi
    else
        log "$title"
        "$@"
    fi
}

# ============================================================================
# Hardware, Display Manager & Session Detection
# ============================================================================
detect_gpu_label() {
    local has_nv=0 has_amd=0 has_intel=0 v vf
    for vf in /sys/class/drm/card*/device/vendor; do
        [[ -r "$vf" ]] || continue
        v="$(cat "$vf" 2>/dev/null || true)"
        case "$v" in
            0x10de) has_nv=1 ;;
            0x1002) has_amd=1 ;;
            0x8086) has_intel=1 ;;
        esac
    done
    if [[ "$has_amd" -eq 1 && "$has_nv" -eq 1 ]]; then
        echo "Hybrid (AMD + NVIDIA)"
    elif [[ "$has_intel" -eq 1 && "$has_nv" -eq 1 ]]; then
        echo "Hybrid (Intel + NVIDIA)"
    elif [[ "$has_nv" -eq 1 ]]; then
        echo "NVIDIA"
    elif [[ "$has_amd" -eq 1 ]]; then
        echo "AMD"
    elif [[ "$has_intel" -eq 1 ]]; then
        echo "Intel"
    else
        echo "Unknown"
    fi
}

detect_chassis_label() {
    if compgen -G "/sys/class/power_supply/BAT*" >/dev/null 2>&1; then
        echo "Laptop (battery detected)"
    else
        echo "Desktop"
    fi
}

detect_current_dm() {
    local dm
    for dm in sddm gdm lightdm greetd; do
        if systemctl is-enabled "${dm}.service" >/dev/null 2>&1 || systemctl is-active "${dm}.service" >/dev/null 2>&1; then
            echo "$dm"
            return 0
        fi
    done
    echo "none"
}

detect_current_session() {
    local de="${XDG_CURRENT_DESKTOP:-${DESKTOP_SESSION:-}}"
    if [[ -n "$de" ]]; then
        if [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
            echo "$de (Wayland)"
        elif [[ -n "${DISPLAY:-}" ]]; then
            echo "$de (X11)"
        else
            echo "$de"
        fi
    elif [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
        echo "Wayland"
    elif [[ -n "${DISPLAY:-}" ]]; then
        echo "X11"
    else
        echo "TTY"
    fi
}

# Ensure gum is available for interactive TUI if possible
if [[ "$DRY_RUN" -eq 0 && "$NON_INTERACTIVE" -eq 0 && -t 0 && -t 1 ]] && ! command -v gum >/dev/null 2>&1; then
    if [[ "$SKIP_PKGS" -eq 0 ]]; then
        sudo pacman -S --needed --noconfirm gum >> "$LOG_FILE" 2>&1 || true
    fi
fi

# Interactive TUI prompt phase
run_tui_prompts() {
    local gpu_info chassis_info dm_info sess_info
    gpu_info="$(detect_gpu_label)"
    chassis_info="$(detect_chassis_label)"
    dm_info="$(detect_current_dm)"
    sess_info="$(detect_current_session)"

    gum style \
        --border rounded \
        --border-foreground 212 \
        --padding "1 3" \
        --margin "1 0" \
        --bold \
        "INERIUM DESKTOP" \
        "Hyprland Lua + Rust Desktop Environment"

    gum style \
        --border normal \
        --border-foreground 99 \
        --padding "0 2" \
        --margin "0 0 1 0" \
        "Detected System:" \
        "  GPU:             $gpu_info" \
        "  Chassis:         $chassis_info" \
        "  Display Manager: $dm_info" \
        "  Session:         $sess_info"

    if [[ "$SKIP_BUILD" -eq 0 ]]; then
        local mode_fast="Fast (prebuilt binaries, ~1-2 min)"
        local mode_src="From source (local Cargo build, ~15 min)"
        local default_mode="$mode_fast"
        [[ "$INSTALL_MODE" == "source" ]] && default_mode="$mode_src"
        local chosen_mode
        chosen_mode="$(gum choose --header "Installation mode:" --selected "$default_mode" "$mode_fast" "$mode_src")"
        if [[ "$chosen_mode" == "$mode_src" ]]; then
            INSTALL_MODE="source"
        else
            INSTALL_MODE="bin"
        fi
    fi

    local dm_greeter="Enable inerium-greeter (greetd)"
    local dm_keep="Keep current DM untouched"
    local default_dm="$dm_greeter"
    [[ "$WITH_GREETER" -eq 0 ]] && default_dm="$dm_keep"
    local chosen_dm
    chosen_dm="$(gum choose --header "Login screen (Display Manager):" --selected "$default_dm" "$dm_greeter" "$dm_keep")"
    if [[ "$chosen_dm" == "$dm_greeter" ]]; then
        WITH_GREETER=1
    else
        WITH_GREETER=0
    fi

    if gum confirm "Download silueta.onnx (~43 MB for Dolphin AI background removal)?" \
        --default="$([[ "$FETCH_MODEL" -eq 1 ]] && echo true || echo false)"; then
        FETCH_MODEL=1
    else
        FETCH_MODEL=0
    fi
}

if use_tui; then
    run_tui_prompts
fi

# ============================================================================
# File & Package Helpers
# ============================================================================
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
        if ! cmp -s "$src" "$dst" 2>/dev/null; then
            printf '[DRY-RUN] sudo install -m %s %s -> %s\n' "$mode" "$src" "$dst"
        fi
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
        warn "Invalid sudoers syntax in $tpl; skipped $dst"
    fi
    rm -f "$tmp"
}

ensure_paru() {
    if command -v paru >/dev/null 2>&1; then
        return 0
    fi
    sudo pacman -S --needed --noconfirm base-devel git
    if pacman -Si paru >/dev/null 2>&1; then
        sudo pacman -S --needed --noconfirm paru
    else
        local tmp_dir
        tmp_dir="$(mktemp -d)"
        git clone --depth 1 https://aur.archlinux.org/paru-bin.git "$tmp_dir/paru-bin"
        (cd "$tmp_dir/paru-bin" && makepkg -si --noconfirm) || { rm -rf "$tmp_dir"; return 1; }
        rm -rf "$tmp_dir"
    fi
}

install_pkgs() {
    local -a missing
    # pacman -T lists only deps not satisfied by installed packages or their provides,
    # so e.g. "rust" is skipped when rustup is installed instead of hitting a conflict prompt.
    mapfile -t missing < <(pacman -T "$@" 2>/dev/null || true)
    if [[ ${#missing[@]} -eq 0 ]]; then
        return 0
    fi
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] paru -S --needed --noconfirm %s\n' "${missing[*]}"
    else
        paru -S --needed --noconfirm "${missing[@]}"
    fi
}

install_pkglist() {
    local -a wanted filtered
    mapfile -t wanted < <(grep -vE '^\s*(#|$)' "$1" | awk '{print $1}')
    if [[ "$INSTALL_MODE" == "bin" && "$SKIP_BUILD" -eq 0 ]]; then
        local pkg
        for pkg in "${wanted[@]}"; do
            case "$pkg" in
                rust|clang|mold) continue ;;
                *) filtered+=("$pkg") ;;
            esac
        done
        install_pkgs "${filtered[@]}"
    else
        install_pkgs "${wanted[@]}"
    fi
}

user_unit_path() {
    local d
    for d in "$HOME/.config/systemd/user" /etc/systemd/user /usr/lib/systemd/user; do
        if [[ -e "$d/$1" ]]; then
            printf '%s\n' "$d/$1"
            return 0
        fi
    done
    return 1
}

export -f ensure_paru install_pkgs install_pkglist
export DRY_RUN INSTALL_MODE SKIP_BUILD

# ============================================================================
# 1. Install base packages
# ============================================================================
if [[ "$SKIP_PKGS" -eq 0 ]]; then
    run_step "Upgrading system (pacman -Syu)..." sudo pacman -Syu --noconfirm
    if ! command -v paru >/dev/null 2>&1; then
        if [[ "$DRY_RUN" -eq 1 ]]; then
            printf '[DRY-RUN] ensure_paru\n'
        else
            run_step "Installing AUR helper (paru)..." ensure_paru
        fi
    fi
    run_step "Installing base packages from pkglist-base.txt..." install_pkglist "$REPO_DIR/pkglist-base.txt"
    if [[ ! -d /usr/share/icons/Bibata-Modern-Ice && ! -d "$HOME/.local/share/icons/Bibata-Modern-Ice" ]]; then
        run_step "Installing Bibata cursor theme..." bash -c 'paru -S --needed --noconfirm bibata-cursor-theme-bin || true'
    fi
    for svc in NetworkManager.service bluetooth.service power-profiles-daemon.service; do
        if ! systemctl is-enabled --quiet "$svc" 2>/dev/null && systemctl list-unit-files "$svc" >/dev/null 2>&1; then
            run "sudo systemctl enable '$svc' >/dev/null 2>&1 || true"
        fi
    done
fi

# ============================================================================
# 2. Deliver Rust binaries (--bin with ldd smoke-test & fallback, or --from-source)
# ============================================================================
USER_BINS_STR="hypr-ipc-bridge inerium-battery inerium-bluetooth inerium-clipboard inerium-clock inerium-dnd inerium-hotspot inerium-hwmon inerium-layout inerium-network inerium-osd-watcher inerium-popup inerium-updates inerium-shell inerium-setup dolphin-convert"

install_from_binaries() {
    local tmp_dir bin missing_libs
    local -a user_bins
    read -ra user_bins <<< "$USER_BINS_STR"
    tmp_dir="$(mktemp -d)"

    if ! curl -fsSL "${RELEASE_BASE_URL}/${BIN_ARCHIVE}" -o "${tmp_dir}/${BIN_ARCHIVE}"; then
        warn "Failed to download ${RELEASE_BASE_URL}/${BIN_ARCHIVE}"
        rm -rf "$tmp_dir"
        return 1
    fi
    if ! curl -fsSL "${RELEASE_BASE_URL}/${BIN_SHA}" -o "${tmp_dir}/${BIN_SHA}"; then
        warn "Failed to download ${RELEASE_BASE_URL}/${BIN_SHA}"
        rm -rf "$tmp_dir"
        return 1
    fi
    if ! (cd "$tmp_dir" && sha256sum -c "$BIN_SHA" >/dev/null 2>&1); then
        warn "SHA256 verification failed for ${BIN_ARCHIVE}"
        rm -rf "$tmp_dir"
        return 1
    fi

    mkdir -p "${tmp_dir}/dist"
    tar -xzf "${tmp_dir}/${BIN_ARCHIVE}" -C "${tmp_dir}/dist"

    # Smoke-test shared library dependencies via ldd
    for bin in "${user_bins[@]}" inerium-polkit inerium-greeter; do
        if [[ ! -x "${tmp_dir}/dist/${bin}" ]]; then
            warn "Missing binary in release archive: ${bin}"
            rm -rf "$tmp_dir"
            return 1
        fi
        missing_libs="$(ldd "${tmp_dir}/dist/${bin}" 2>/dev/null | grep "not found" || true)"
        if [[ -n "$missing_libs" ]]; then
            warn "Shared library mismatch in ${bin}:"
            printf '%s\n' "$missing_libs" >&2
            rm -rf "$tmp_dir"
            return 1
        fi
    done

    mkdir -p "$HOME/.local/bin"
    for bin in "${user_bins[@]}" inerium-polkit; do
        install -m 755 "${tmp_dir}/dist/${bin}" "$HOME/.local/bin/${bin}"
    done
    install_root_if_changed 755 "${tmp_dir}/dist/inerium-greeter" /usr/local/bin/inerium-greeter

    rm -rf "$tmp_dir"
    return 0
}

build_from_source() {
    local -a user_bins
    read -ra user_bins <<< "$USER_BINS_STR"
    if command -v rustup >/dev/null 2>&1 && ! rustc --version >/dev/null 2>&1; then
        run_step "Initializing default Rust toolchain..." rustup default stable
    fi
    run_step "Compiling universal Rust workspace (this may take 10-15 min)..." \
        cargo build --manifest-path "$REPO_DIR/Cargo.toml" --workspace --release
    run_step "Compiling standalone inerium-polkit..." \
        cargo build --manifest-path "$REPO_DIR/standalone/inerium-polkit/Cargo.toml" --release

    log "Installing compiled binaries atomically..."
    mkdir -p "$HOME/.local/bin"
    for bin in "${user_bins[@]}"; do
        install -m 755 "$REPO_DIR/target/release/$bin" "$HOME/.local/bin/$bin"
    done
    install -m 755 "$REPO_DIR/standalone/inerium-polkit/target/release/inerium-polkit" "$HOME/.local/bin/inerium-polkit"
    install_root_if_changed 755 "$REPO_DIR/target/release/inerium-greeter" /usr/local/bin/inerium-greeter
}

export -f install_from_binaries install_root_if_changed warn log
export RELEASE_BASE_URL BIN_ARCHIVE BIN_SHA USER_BINS_STR

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    if [[ "$DRY_RUN" -eq 1 ]]; then
        if [[ "$INSTALL_MODE" == "bin" ]]; then
            printf '[DRY-RUN] Download %s/%s + %s, verify sha256, run ldd smoke-test, install to ~/.local/bin & /usr/local/bin\n' \
                "$RELEASE_BASE_URL" "$BIN_ARCHIVE" "$BIN_SHA"
        else
            printf '[DRY-RUN] cargo build --manifest-path %s/Cargo.toml --workspace --release\n' "$REPO_DIR"
            printf '[DRY-RUN] cargo build --manifest-path %s/standalone/inerium-polkit/Cargo.toml --release\n' "$REPO_DIR"
            printf '[DRY-RUN] install 17 binaries -> ~/.local/bin/ and inerium-greeter -> /usr/local/bin/\n'
        fi
    elif [[ "$INSTALL_MODE" == "bin" ]]; then
        if ! run_step "Downloading & verifying prebuilt binaries..." install_from_binaries; then
            warn "Prebuilt binary installation failed or shared libraries did not match."
            do_fallback=1
            if use_tui; then
                if ! gum confirm "Fall back to local Cargo compilation (--from-source)?" --default=true; then
                    do_fallback=0
                fi
            fi
            if [[ "$do_fallback" -eq 1 ]]; then
                INSTALL_MODE="source"
                if [[ "$SKIP_PKGS" -eq 0 ]]; then
                    run_step "Installing Rust build toolchain..." install_pkgs rust base-devel pkgconf clang mold
                fi
                build_from_source
            else
                exit 1
            fi
        fi
    else
        build_from_source
    fi
fi

# ============================================================================
# 3. Install user scripts & system indexer
# ============================================================================
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
for desktop in "$REPO_DIR"/local/share/applications/*.desktop; do
    link_item "$desktop" "$HOME/.local/share/applications/$(basename "$desktop")"
done
run "mkdir -p '${XDG_CONFIG_HOME:-$HOME/.config}/inerium/first-login.d'"
if [[ -d "$REPO_DIR/first-login.d" ]]; then
    for step in "$REPO_DIR"/first-login.d/*; do
        [[ -e "$step" ]] || continue
        link_item "$step" "${XDG_CONFIG_HOME:-$HOME/.config}/inerium/first-login.d/$(basename "$step")"
    done
fi
install_root_if_changed 755 "$REPO_DIR/system/usr/local/bin/inerium-desktop-indexer" /usr/local/bin/inerium-desktop-indexer
install_sudoers_template "$REPO_DIR/system/etc/sudoers.d/inerium-desktop-indexer.template" /etc/sudoers.d/inerium-desktop-indexer

# ============================================================================
# 4. Symlink ~/.config directories and Hyprland files
# ============================================================================
log "Linking desktop configurations into ~/.config..."
for dir in alacritty environment.d gtk-3.0 gtk-4.0 kitty mako rofi uwsm waybar; do
    link_item "$REPO_DIR/config/$dir" "$HOME/.config/$dir"
done
link_item "$REPO_DIR/config/btop/btop.conf" "$HOME/.config/btop/btop.conf"

run "mkdir -p '$HOME/.config/rclone'"
link_item "$REPO_DIR/config/rclone/filter.txt" "$HOME/.config/rclone/filter.txt"

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
    if [[ "$base" == "autostart_backup.lua" || "$base" == "binds_backup.lua" ]]; then
        # Fallback session files are copied once and never symlinked, so a broken repo
        # checkout cannot take down the emergency session; existing copies are left alone.
        if [[ ! -e "$HOME/.config/hypr/config/$base" ]]; then
            run "install -m 644 '$f' '$HOME/.config/hypr/config/$base'"
        fi
        continue
    fi
    link_item "$f" "$HOME/.config/hypr/config/$base"
done
render_template "$REPO_DIR/config/hypr/hyprpaper.conf.template" "$HOME/.config/hypr/hyprpaper.conf" 1
render_template "$REPO_DIR/config/hypr/profile.default" "$HOME/.config/hypr/profile" 1

WS_COUNT_FILE="$HOME/.config/hypr/config/workspaces_count"
if [[ -n "$WORKSPACES_PER_MON" && "$WORKSPACES_PER_MON" =~ ^[1-9]$ ]]; then
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] write %s -> %s\n' "$WORKSPACES_PER_MON" "$WS_COUNT_FILE"
    else
        printf '%s\n' "$WORKSPACES_PER_MON" > "$WS_COUNT_FILE"
    fi
fi

# 4b. Hardware auto-detection -> local gitignored overlays (*_local.lua, hypridle_local.conf)
detect_and_generate_local_overlays() {
    local env_local="$HOME/.config/hypr/config/environment_local.lua"
    local idle_local="$HOME/.config/hypr/hypridle_local.conf"
    local idle_dropin="$HOME/.config/systemd/user/hypridle.service.d/local.conf"
    local inputs_local="$HOME/.config/hypr/config/inputs_local.lua"
    local mons_local="$HOME/.config/hypr/config/monitors_local.lua"

    # 1. GPU detection (/sys/class/drm/card*/device/vendor)
    if [[ ! -e "$env_local" ]]; then
        local has_nv=0 has_amd=0 has_intel=0 v
        for vf in /sys/class/drm/card*/device/vendor; do
            [[ -r "$vf" ]] || continue
            v="$(cat "$vf" 2>/dev/null || true)"
            case "$v" in
                0x10de) has_nv=1 ;;
                0x1002) has_amd=1 ;;
                0x8086) has_intel=1 ;;
            esac
        done
        if [[ "$DRY_RUN" -eq 1 ]]; then
            printf '[DRY-RUN] generate GPU overlay %s (nv=%d amd=%d intel=%d)\n' "$env_local" "$has_nv" "$has_amd" "$has_intel"
        else
            mkdir -p "$(dirname "$env_local")"
            {
                echo "-- Auto-generated by inerium-desktop hardware detection (gitignored)"
                if [[ "$has_nv" -eq 1 && "$has_amd" -eq 0 && "$has_intel" -eq 0 ]]; then
                    echo 'hl.env("LIBVA_DRIVER_NAME", "nvidia")'
                    echo 'hl.env("GBM_BACKEND", "nvidia-drm")'
                    echo 'hl.env("__GLX_VENDOR_LIBRARY_NAME", "nvidia")'
                    echo 'hl.env("NVD_BACKEND", "direct")'
                elif [[ "$has_amd" -eq 1 ]]; then
                    echo 'hl.env("LIBVA_DRIVER_NAME", "radeonsi")'
                    echo 'hl.env("VDPAU_DRIVER", "radeonsi")'
                    if [[ "$has_nv" -eq 1 ]]; then
                        echo '-- Hybrid AMD+NVIDIA detected; iGPU handles Wayland display, use prime-run for dGPU offload.'
                    fi
                elif [[ "$has_intel" -eq 1 ]]; then
                    echo 'hl.env("LIBVA_DRIVER_NAME", "iHD")'
                    if [[ "$has_nv" -eq 1 ]]; then
                        echo '-- Hybrid Intel+NVIDIA detected; iGPU handles Wayland display, use prime-run for dGPU offload.'
                    fi
                fi
            } > "${env_local}.tmp" && mv -f "${env_local}.tmp" "$env_local"
        fi
    fi

    # 2. Chassis detection (laptop battery vs desktop) -> hypridle_local.conf
    if [[ ! -e "$idle_local" ]]; then
        local is_laptop=0
        if compgen -G "/sys/class/power_supply/BAT*" >/dev/null 2>&1; then
            is_laptop=1
        fi
        if [[ "$DRY_RUN" -eq 1 ]]; then
            printf '[DRY-RUN] generate idle overlay %s (laptop=%d)\n' "$idle_local" "$is_laptop"
        else
            mkdir -p "$(dirname "$idle_local")"
            if [[ "$is_laptop" -eq 1 ]]; then
                cat > "${idle_local}.tmp" <<'EOF'
# Auto-generated by inerium-desktop (laptop chassis, gitignored)
general {
    lock_cmd = pidof hyprlock || hyprlock
    before_sleep_cmd = loginctl lock-session
    after_sleep_cmd = hyprctl dispatch dpms on
}

listener {
    timeout = 150
    on-timeout = brightnessctl -s set 20%
    on-resume = brightnessctl -r
}

listener {
    timeout = 300
    on-timeout = loginctl lock-session
}

listener {
    timeout = 330
    on-timeout = hyprctl dispatch dpms off
    on-resume = hyprctl dispatch dpms on
}

listener {
    timeout = 600
    on-timeout = systemctl suspend
}
EOF
            else
                cat > "${idle_local}.tmp" <<'EOF'
# Auto-generated by inerium-desktop (desktop chassis, gitignored)
general {
    lock_cmd = pidof hyprlock || hyprlock
    before_sleep_cmd = loginctl lock-session
    after_sleep_cmd = hyprctl dispatch dpms on
}

listener {
    timeout = 600
    on-timeout = loginctl lock-session
}

listener {
    timeout = 900
    on-timeout = hyprctl dispatch dpms off
    on-resume = hyprctl dispatch dpms on
}
EOF
            fi
            mv -f "${idle_local}.tmp" "$idle_local"
            mkdir -p "$(dirname "$idle_dropin")"
            printf '[Service]\nExecStart=\nExecStart=/usr/bin/hypridle -c %s\n' "$idle_local" > "${idle_dropin}.tmp"
            mv -f "${idle_dropin}.tmp" "$idle_dropin"
        fi
    fi

    # 3. Keyboard layout detection (localectl / vconsole.conf, default us,ru + Super+Space)
    if [[ ! -e "$inputs_local" ]]; then
        local kb_layout="us,ru"
        if command -v localectl >/dev/null 2>&1; then
            local x11_layout
            x11_layout="$(localectl status 2>/dev/null | awk -F: '/X11 Layout/ {gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}' || true)"
            if [[ -n "$x11_layout" && "$x11_layout" != "n/a" && "$x11_layout" != "us" ]]; then
                if [[ "$x11_layout" == *","* ]]; then
                    kb_layout="$x11_layout"
                else
                    kb_layout="us,$x11_layout"
                fi
            fi
        fi
        if [[ "$DRY_RUN" -eq 1 ]]; then
            printf '[DRY-RUN] generate inputs overlay %s (kb_layout=%s)\n' "$inputs_local" "$kb_layout"
        else
            mkdir -p "$(dirname "$inputs_local")"
            cat > "${inputs_local}.tmp" <<EOF
-- Auto-generated by inerium-desktop (gitignored)
hl.config({
    input = {
        kb_layout  = "${kb_layout}",
        kb_options = "grp:win_space_toggle",
    },
})
EOF
            mv -f "${inputs_local}.tmp" "$inputs_local"
        fi
    fi

    # 4. HiDPI monitor scale detection (when running inside Hyprland)
    if [[ ! -e "$mons_local" ]] && command -v hyprctl >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
        local mon_json
        mon_json="$(hyprctl monitors -j 2>/dev/null || true)"
        if [[ -n "$mon_json" && "$mon_json" == "["* ]]; then
            if [[ "$DRY_RUN" -eq 1 ]]; then
                printf '[DRY-RUN] generate monitors overlay %s from hyprctl monitors -j\n' "$mons_local"
            else
                python3 - "$mon_json" "$mons_local" <<'PYEOF' || true
import json, math, os, sys
mons = json.loads(sys.argv[1])
lines = ["-- Auto-generated by inerium-desktop HiDPI detection (gitignored)\n"]
for m in mons:
    name = m.get("name", "")
    if not name:
        continue
    w = int(m.get("width", 1920) or 1920)
    h = int(m.get("height", 1080) or 1080)
    pw = int(m.get("physicalWidth", 0) or 0)
    ph = int(m.get("physicalHeight", 0) or 0)
    scale = 1.0
    diag_in = math.hypot(pw, ph) / 25.4 if pw > 0 and ph > 0 else 0.0
    if w >= 3840:
        scale = 2.0 if (diag_in > 0 and diag_in <= 16.5) else 1.75
    elif w >= 2560 and diag_in > 0 and diag_in <= 16.5:
        scale = 1.5 if w >= 2880 else 1.25
    lines.append(
        f'hl.monitor({{\n    output   = "{name}",\n    mode     = "preferred",\n    position = "auto",\n    scale    = {scale},\n}})\n'
    )
os.makedirs(os.path.dirname(sys.argv[2]), exist_ok=True)
tmp = sys.argv[2] + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    f.writelines(lines)
os.replace(tmp, sys.argv[2])
PYEOF
            fi
        fi
    fi

    # 5. Vendor EC status log
    if [[ -d /sys/devices/platform/msi-ec ]]; then
        log "Detected MSI EC (/sys/devices/platform/msi-ec); EC fan/battery controls enabled."
    else
        log "Standard chassis detected; using power-profiles-daemon."
    fi
}
detect_and_generate_local_overlays

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

# ============================================================================
# 5. Check / fetch ONNX background removal model (silueta.onnx)
# ============================================================================
if [[ -s "$SILUETA_PATH" ]]; then
    log "Found silueta.onnx at $SILUETA_PATH"
elif [[ "$FETCH_MODEL" -eq 1 ]]; then
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] mkdir -p %s && curl -fsSL %s -o %s\n' "$(dirname "$SILUETA_PATH")" "$SILUETA_URL" "$SILUETA_PATH"
    else
        mkdir -p "$(dirname "$SILUETA_PATH")"
        run_step "Downloading silueta.onnx (~43 MB)..." curl -fsSL "$SILUETA_URL" -o "$SILUETA_PATH"
    fi
else
    log "NOTE: silueta.onnx not found at $SILUETA_PATH."
    log "      Run with --fetch-model (or download from $SILUETA_URL) for Dolphin AI background removal."
fi

# ============================================================================
# 6. Wayland session entries & optional greetd + inerium-greeter setup
#    Note: Never stop an active DM (e.g. sddm/gdm) so KDE/GNOME sessions stay intact.
# ============================================================================
for sess in "$REPO_DIR"/system/usr/share/wayland-sessions/*.desktop; do
    [[ -f "$sess" ]] || continue
    install_root_if_changed 644 "$sess" "/usr/share/wayland-sessions/$(basename "$sess")"
done

if [[ "$WITH_GREETER" -eq 1 ]]; then
    log "Configuring greetd + inerium-greeter..."
    if [[ "$SKIP_PKGS" -eq 0 ]]; then
        if [[ "$DRY_RUN" -eq 1 ]]; then
            printf '[DRY-RUN] install_pkgs greetd cage\n'
        else
            run_step "Installing greetd & cage..." install_pkgs greetd cage
        fi
    fi
    install_root_if_changed 644 "$REPO_DIR/assets/backgrounds/inerium-greeter.jpg" /usr/share/backgrounds/inerium-greeter.jpg
    if [[ -f /etc/greetd/config.toml ]] && ! cmp -s "$REPO_DIR/system/etc/greetd/config.toml" /etc/greetd/config.toml 2>/dev/null; then
        log "Backing up existing /etc/greetd/config.toml -> /etc/greetd/config.toml.bak.${BACKUP_STAMP}"
        run "sudo cp -a /etc/greetd/config.toml '/etc/greetd/config.toml.bak.${BACKUP_STAMP}'"
    fi
    install_root_if_changed 644 "$REPO_DIR/system/etc/greetd/config.toml" /etc/greetd/config.toml
    install_root_if_changed 644 "$REPO_DIR/system/etc/greetd/environments" /etc/greetd/environments
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] render %s -> /etc/greetd/inerium-greeter.json\n' "$REPO_DIR/system/etc/greetd/inerium-greeter.json.template"
    else
        greeter_tmp="$(mktemp)"
        sed -e "s|__HOME__|$HOME|g" -e "s|__USER__|$USER|g" "$REPO_DIR/system/etc/greetd/inerium-greeter.json.template" > "$greeter_tmp"
        install_root_if_changed 644 "$greeter_tmp" /etc/greetd/inerium-greeter.json
        rm -f "$greeter_tmp"
    fi
    for dm in sddm.service gdm.service lightdm.service; do
        if systemctl is-enabled "$dm" >/dev/null 2>&1; then
            log "Disabling display manager autostart (keeping current session running): $dm"
            run "sudo systemctl disable '$dm'"
        fi
    done
    if ! systemctl is-enabled --quiet greetd.service 2>/dev/null; then
        log "Enabling greetd.service..."
        run "sudo systemctl enable --force greetd.service"
    fi
fi

# ============================================================================
# 7. Enable systemd user units (live D-Bus or offline TTY symlink fallback) & rebuild KDE cache
# ============================================================================
USER_UNITS="cliphist.service cliphist-images.service hypr-ipc-bridge.service hyprpaper.service hypridle.service inerium-desktop-indexer.path inerium-osd-watcher.service inerium-polkit.service inerium-popups.service inerium-updates.timer mako.service swayosd-server.service waybar.service"
MASK_UNITS="hyprpolkitagent.service at-spi2-registryd.service at-spi-dbus-bus.service"
if systemctl --user show-environment >/dev/null 2>&1; then
    log "Refreshing systemd user daemon & enabling user services..."
    run "systemctl --user daemon-reload"
    run "systemctl --user mask $MASK_UNITS >/dev/null 2>&1 || true"
    run "systemctl --user enable $USER_UNITS"
else
    log "No active systemd --user D-Bus session (TTY/chroot); creating offline .wants/mask links..."
    for m in $MASK_UNITS; do
        run "ln -sfn /dev/null '$HOME/.config/systemd/user/$m'"
    done
    for u in $USER_UNITS; do
        if ! unit_src="$(user_unit_path "$u")"; then
            log "WARNING: user unit $u not found; skipping offline enable"
            continue
        fi
        # Mirror 'systemctl --user enable': link into every [Install] WantedBy= target
        for target in $(sed -n 's/^WantedBy=//p' "$unit_src"); do
            run "mkdir -p '$HOME/.config/systemd/user/$target.wants'"
            run "ln -sfn '$unit_src' '$HOME/.config/systemd/user/$target.wants/$u'"
        done
    done
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    run "kbuildsycoca6 --noincremental >/dev/null 2>&1 || true"
fi

# ============================================================================
# 8. Final Summary & Next Steps
# ============================================================================
print_summary() {
    local msg1 msg2
    msg1="Inerium Desktop installation complete!"
    if [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
        msg2="Your current desktop session was kept intact. Reboot (sudo reboot) or log out and select 'Hyprland (Waybar)' at the login screen."
    else
        msg2="Run 'sudo reboot' to start inerium-greeter and enter Hyprland."
    fi

    if use_tui; then
        gum style \
            --border rounded \
            --border-foreground 82 \
            --padding "1 2" \
            --margin "1 0" \
            "$msg1" \
            "$msg2" \
            "Install log: $LOG_FILE"
    else
        log "$msg1"
        log "$msg2"
    fi
}

if [[ "$DRY_RUN" -eq 0 ]]; then
    print_summary
else
    log "Dry-run complete."
fi
