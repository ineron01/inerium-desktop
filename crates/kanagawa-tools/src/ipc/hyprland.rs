//! Unified and fault-tolerant resolution of Hyprland IPC sockets.

use std::env;
use std::fs;
use std::path::PathBuf;
use crate::sys::posix::posix_getuid;

/// Returns the base Hyprland runtime directory: `$XDG_RUNTIME_DIR/hypr`
pub fn get_hyprland_base_dir() -> PathBuf {
    let rt = env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", posix_getuid()));
    PathBuf::from(rt).join("hypr")
}

/// Resolves the directory for the active Hyprland instance.
/// Checks `HYPRLAND_INSTANCE_SIGNATURE` first, falling back to newest modified directory.
pub fn get_socket_dir() -> Option<PathBuf> {
    let hypr_dir = get_hyprland_base_dir();

    let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .filter(|s| {
            let p = hypr_dir.join(s);
            p.join(".socket.sock").exists() || p.join(".socket.real.sock").exists() || p.join(".socket2.sock").exists()
        })
        .or_else(|| {
            let entries = fs::read_dir(&hypr_dir).ok()?;
            let mut subdirs: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .collect();
            subdirs.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            subdirs.last().map(|e| e.file_name().to_string_lossy().into_owned())
        })?;

    let target = hypr_dir.join(sig);
    if target.is_dir() {
        Some(target)
    } else {
        None
    }
}

/// Returns path to Hyprland's `.socket.sock` (or proxied socket)
pub fn get_socket1() -> Option<PathBuf> {
    let dir = get_socket_dir()?;
    let p = dir.join(".socket.sock");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// Returns path to Hyprland's real backend socket (`.socket.real.sock` if bridge is active, else `.socket.sock`)
pub fn get_real_socket1() -> Option<PathBuf> {
    let dir = get_socket_dir()?;
    let real = dir.join(".socket.real.sock");
    if real.exists() {
        Some(real)
    } else {
        let orig = dir.join(".socket.sock");
        if orig.exists() {
            Some(orig)
        } else {
            None
        }
    }
}

/// Returns path to Hyprland's event stream socket `.socket2.sock`
pub fn get_socket2() -> Option<PathBuf> {
    let dir = get_socket_dir()?;
    let p = dir.join(".socket2.sock");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_dir() {
        let dir = get_socket_dir();
        assert!(dir.is_some(), "Hyprland socket directory should be found in active session");
    }
}
