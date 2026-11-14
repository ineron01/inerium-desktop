#!/usr/bin/env bash
# ============================================================================
# inerium-shell/bootstrap.sh — One-command TTY/DE bootstrap for Arch/CachyOS
# ============================================================================
set -euo pipefail

REPO_URL="${INERIUM_SHELL_REPO:-${INERIUM_DESKTOP_REPO:-https://github.com/ineron01/inerium-shell.git}}"
TARGET_DIR="${INERIUM_SHELL_DIR:-${INERIUM_DESKTOP_DIR:-$HOME/Repos/inerium-shell}}"
LOG_FILE="/tmp/inerium-install.log"
DRY_RUN=0
SKIP_PKGS=0

for arg in "$@"; do
    case "$arg" in
        --dry-run)   DRY_RUN=1 ;;
        --skip-pkgs) SKIP_PKGS=1 ;;
    esac
done

has_gum() {
    command -v gum >/dev/null 2>&1 && [[ -t 1 ]]
}

print_banner() {
    if has_gum; then
        gum style \
            --border rounded \
            --border-foreground 212 \
            --padding "1 3" \
            --margin "1 0" \
            --bold \
            "INERIUM SHELL" \
            "Universal Arch / CachyOS Hyprland + Rust Installer"
    else
        printf '\n=== INERIUM SHELL — Bootstrap ===\n\n'
    fi
}

run_quiet_step() {
    local title="$1"
    shift
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '[DRY-RUN] %s: %s\n' "$title" "$*"
        return 0
    fi
    if has_gum; then
        printf '\n--- %s (%s) ---\n' "$title" "$(date '+%H:%M:%S')" >> "$LOG_FILE"
        if ! gum spin --spinner dot --title "$title" -- bash -c '"$@" >> "$0" 2>&1' "$LOG_FILE" "$@"; then
            printf '\n[bootstrap] ERROR during: %s\n' "$title" >&2
            printf '[bootstrap] Last 25 lines of %s:\n' "$LOG_FILE" >&2
            tail -n 25 "$LOG_FILE" >&2 || true
            exit 1
        fi
    else
        printf '[bootstrap] %s\n' "$title"
        "$@"
    fi
}

if [[ "$DRY_RUN" -eq 0 ]]; then
    : > "$LOG_FILE"
    if ! sudo -n true 2>/dev/null; then
        sudo -v
    fi
fi

# Full upgrade first: installing on top of "pacman -Sy" alone is an unsupported partial upgrade.
if [[ "$SKIP_PKGS" -eq 0 ]]; then
    echo "[bootstrap] Upgrading system and installing git, base-devel, gum..."
    if [[ "$DRY_RUN" -eq 1 ]]; then
        echo "[DRY-RUN] sudo pacman -Syu --needed --noconfirm git base-devel"
        echo "[DRY-RUN] sudo pacman -S --needed --noconfirm gum || true"
    else
        sudo pacman -Syu --needed --noconfirm git base-devel >> "$LOG_FILE" 2>&1 || {
            tail -n 25 "$LOG_FILE" >&2
            exit 1
        }
        sudo pacman -S --needed --noconfirm gum >> "$LOG_FILE" 2>&1 || true
    fi
elif ! command -v git >/dev/null 2>&1; then
    if [[ "$DRY_RUN" -eq 1 ]]; then
        echo "[DRY-RUN] sudo pacman -S --needed --noconfirm git"
    else
        sudo pacman -S --needed --noconfirm git
    fi
fi

print_banner

if [[ "$SKIP_PKGS" -eq 0 ]] && ! command -v paru >/dev/null 2>&1; then
    install_paru() {
        if pacman -Si paru >/dev/null 2>&1; then
            sudo pacman -S --needed --noconfirm paru
        else
            local tmp_dir
            tmp_dir="$(mktemp -d)"
            git clone --depth 1 https://aur.archlinux.org/paru-bin.git "$tmp_dir/paru-bin"
            (cd "$tmp_dir/paru-bin" && makepkg -si --noconfirm)
            rm -rf "$tmp_dir"
        fi
    }
    export -f install_paru
    run_quiet_step "Installing AUR helper (paru)..." install_paru
fi

mkdir -p "$(dirname "$TARGET_DIR")"
if [[ -d "$TARGET_DIR/.git" ]]; then
    run_quiet_step "Updating repository at $TARGET_DIR..." git -C "$TARGET_DIR" pull --ff-only
else
    run_quiet_step "Cloning inerium-shell repository..." git clone "$REPO_URL" "$TARGET_DIR"
fi

if [[ ! -t 0 && -t 1 && -r /dev/tty ]]; then
    exec "$TARGET_DIR/install.sh" --with-greeter --fetch-model "$@" < /dev/tty
else
    exec "$TARGET_DIR/install.sh" --with-greeter --fetch-model "$@"
fi
