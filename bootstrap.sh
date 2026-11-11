#!/usr/bin/env bash
# ============================================================================
# kanagawa-desktop/bootstrap.sh — One-command TTY bootstrap after fresh CachyOS
# ============================================================================
set -euo pipefail

REPO_URL="${KANAGAWA_DESKTOP_REPO:-https://github.com/ineron01/kanagawa-desktop.git}"
TARGET_DIR="${KANAGAWA_DESKTOP_DIR:-$HOME/Repos/kanagawa-desktop}"

# Full upgrade first: installing on top of "pacman -Sy" alone is an unsupported partial upgrade
# (a fresh install from an older ISO would mix new packages with stale libraries).
if [[ " $* " != *" --skip-pkgs "* ]]; then
    echo "[bootstrap] Upgrading the system and installing git/base-devel..."
    sudo pacman -Syu --needed --noconfirm git base-devel
elif ! command -v git >/dev/null 2>&1; then
    sudo pacman -S --needed --noconfirm git
fi

if [[ " $* " != *" --skip-pkgs "* ]] && ! command -v paru >/dev/null 2>&1; then
    echo "[bootstrap] Installing paru..."
    if pacman -Si paru >/dev/null 2>&1; then
        sudo pacman -S --needed --noconfirm paru
    else
        TMP_DIR="$(mktemp -d)"
        trap 'rm -rf "$TMP_DIR"' EXIT
        git clone --depth 1 https://aur.archlinux.org/paru-bin.git "$TMP_DIR/paru-bin"
        (cd "$TMP_DIR/paru-bin" && makepkg -si --noconfirm)
        rm -rf "$TMP_DIR"
        trap - EXIT
    fi
fi

mkdir -p "$(dirname "$TARGET_DIR")"
if [[ -d "$TARGET_DIR/.git" ]]; then
    echo "[bootstrap] Updating existing clone at $TARGET_DIR..."
    git -C "$TARGET_DIR" pull --ff-only
else
    echo "[bootstrap] Cloning $REPO_URL -> $TARGET_DIR..."
    git clone "$REPO_URL" "$TARGET_DIR"
fi

exec "$TARGET_DIR/install.sh" --with-greeter --fetch-model "$@"
