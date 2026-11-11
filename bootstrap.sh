#!/usr/bin/env bash
# ============================================================================
# kanagawa-desktop/bootstrap.sh — One-command TTY bootstrap after fresh CachyOS
# ============================================================================
set -euo pipefail

REPO_URL="${KANAGAWA_DESKTOP_REPO:-}"
TARGET_DIR="${KANAGAWA_DESKTOP_DIR:-$HOME/Repos/kanagawa-desktop}"

if [[ -z "$REPO_URL" ]]; then
    echo "Error: KANAGAWA_DESKTOP_REPO URL is not set yet (publish to GitHub first)." >&2
    exit 1
fi

if ! command -v git >/dev/null 2>&1; then
    echo "[bootstrap] Installing git..."
    sudo pacman -Sy --needed --noconfirm git
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
