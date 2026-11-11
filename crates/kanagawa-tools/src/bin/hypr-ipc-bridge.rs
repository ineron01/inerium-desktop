//! hypr-ipc-bridge — Native Rust Unix socket bridge for Hyprland 0.56+ Lua API compatibility.
//!
//! Intercepts commands sent to Hyprland's IPC socket (.socket.sock) by Waybar
//! and translates legacy dispatch syntax into valid Lua dispatch calls.

use std::ffi::CString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::thread;

use kanagawa_tools::ipc::hyprland::{get_socket_dir, get_socket2};
use kanagawa_tools::sys::posix::*;

static PUB_SOCK_PTR: AtomicPtr<std::ffi::c_char> = AtomicPtr::new(std::ptr::null_mut());
static REAL_SOCK_PTR: AtomicPtr<std::ffi::c_char> = AtomicPtr::new(std::ptr::null_mut());
static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_signal(_: i32) {
    restore_sockets();
    std::process::exit(0);
}

fn restore_sockets() {
    let pub_ptr = PUB_SOCK_PTR.load(Ordering::SeqCst);
    let real_ptr = REAL_SOCK_PTR.load(Ordering::SeqCst);
    if !pub_ptr.is_null() && !real_ptr.is_null() {
        let pub_cstr = unsafe { std::ffi::CStr::from_ptr(pub_ptr) };
        let real_cstr = unsafe { std::ffi::CStr::from_ptr(real_ptr) };
        let _ = posix_unlink(pub_cstr);
        let _ = posix_rename(real_cstr, pub_cstr);
    }
}

pub fn translate(cmd: &str) -> Option<String> {
    let s = cmd.trim_matches(|c| c == '\0' || c == '\r' || c == '\n' || c == ' ');
    let s = s.strip_prefix('/').unwrap_or(s);
    let s = s.strip_prefix("dispatch")?;
    let rest = s.trim_start();

    // 1. Toggle special workspace
    if let Some(arg) = rest.strip_prefix("togglespecialworkspace") {
        let name = arg.trim();
        if name.is_empty() {
            return Some("dispatch hl.dsp.workspace.toggle_special({})".to_string());
        } else {
            return Some(format!("dispatch hl.dsp.workspace.toggle_special({{ name = \"{}\" }})", name));
        }
    }

    // 2. Move cursor
    if let Some(coords) = rest.strip_prefix("movecursor").map(|c| c.trim_start()) {
        let mut parts = coords.split_whitespace();
        if let (Some(x), Some(y)) = (parts.next(), parts.next())
            && x.parse::<i32>().is_ok() && y.parse::<i32>().is_ok() {
                return Some(format!("dispatch hl.dsp.cursor.move({{ x = {}, y = {} }})", x, y));
            }
    }

    // 2b. Focus monitor
    if let Some(arg) = rest.strip_prefix("focusmonitor").map(|c| c.trim_start()) {
        let first_word = arg.split_whitespace().next().unwrap_or("");
        if !first_word.is_empty() {
            return Some(format!("dispatch hl.dsp.focus({{ monitor = \"{}\" }})", first_word));
        }
    }

    // 3. Focus workspace (regular, relative, special or on current monitor)
    let (ws_arg, on_current_monitor) = if let Some(arg) = rest.strip_prefix("focusworkspaceoncurrentmonitor").map(|c| c.trim_start()) {
        (Some(arg), true)
    } else if let Some(arg) = rest.strip_prefix("workspace").map(|c| c.trim_start()) {
        (Some(arg), false)
    } else {
        (None, false)
    };

    if let Some(arg) = ws_arg {
        let first_word = arg.split_whitespace().next().unwrap_or("");
        if !first_word.is_empty() {
            let monitor_opt = if on_current_monitor { ", on_current_monitor = true" } else { "" };

            // Relative prefixes: e+N, e-N, r+N, r-N, m+N, m-N
            if first_word.starts_with("e+")
                || first_word.starts_with("r+")
                || first_word.starts_with("m+")
                || first_word.starts_with("e-")
                || first_word.starts_with("r-")
                || first_word.starts_with("m-")
            {
                return Some(format!("dispatch hl.dsp.focus({{ workspace = \"{}\"{} }})", first_word, monitor_opt));
            }

            // Direct relative offsets: +N, -N
            if first_word.starts_with('+') || first_word.starts_with('-') {
                return Some(format!("dispatch hl.dsp.focus({{ workspace = \"{}\"{} }})", first_word, monitor_opt));
            }

            // Named special targets: previous, empty, etc.
            if matches!(first_word, "previous" | "previous_per_monitor" | "empty" | "empty_per_monitor" | "current") {
                return Some(format!("dispatch hl.dsp.focus({{ workspace = \"{}\"{} }})", first_word, monitor_opt));
            }

            // Absolute numbered workspace
            if first_word.chars().all(|c| c.is_ascii_digit()) {
                return Some(format!("dispatch hl.dsp.focus({{ workspace = {}{} }})", first_word, monitor_opt));
            } else if first_word.starts_with("name:") {
                return Some(format!("dispatch hl.dsp.focus({{ workspace = \"{}\"{} }})", first_word, monitor_opt));
            } else {
                // Named workspace without explicit name: prefix
                return Some(format!("dispatch hl.dsp.focus({{ workspace = \"name:{}\"{} }})", first_word, monitor_opt));
            }
        }
    }

    None
}

fn handle_client(mut client: UnixStream, real_sock: &Path) {
    let mut buf = [0u8; 4096];
    let n = match client.read(&mut buf) {
        Ok(0) | Err(_) => return,
        Ok(n) => n,
    };

    let mut upstream = match UnixStream::connect(real_sock) {
        Ok(u) => u,
        Err(_) => return,
    };

    let data_to_send: Vec<u8> = if let Ok(cmd_str) = std::str::from_utf8(&buf[..n]) {
        if let Some(translated) = translate(cmd_str) {
            translated.into_bytes()
        } else {
            buf[..n].to_vec()
        }
    } else {
        buf[..n].to_vec()
    };

    if upstream.write_all(&data_to_send).is_err() {
        return;
    }
    let _ = upstream.shutdown(std::net::Shutdown::Write);

    let mut resp_buf = [0u8; 65536];
    loop {
        match upstream.read(&mut resp_buf) {
            Ok(0) | Err(_) => break,
            Ok(rn) => {
                if client.write_all(&resp_buf[..rn]).is_err() {
                    break;
                }
            }
        }
    }
}

fn watch_hyprland_lifecycle(s2_path: PathBuf) {
    if let Ok(stream) = UnixStream::connect(&s2_path) {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if line.is_err() {
                break;
            }
        }
    }
    // Socket2 closed = Hyprland terminated
    eprintln!("[hypr-ipc-bridge] Hyprland socket2 closed, exiting");
    RUNNING.store(false, Ordering::SeqCst);
    restore_sockets();
    std::process::exit(0);
}

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_sockets();
        default_hook(info);
    }));

    let sock_dir = match get_socket_dir() {
        Some(d) => d,
        None => {
            eprintln!("[hypr-ipc-bridge] HYPRLAND_INSTANCE_SIGNATURE could not be determined");
            std::process::exit(1);
        }
    };

    let pub_sock = sock_dir.join(".socket.sock");
    let real_sock = sock_dir.join(".socket.real.sock");

    // Register C strings for signal handler
    let pub_bytes = pub_sock.to_string_lossy().into_owned().into_bytes();
    let real_bytes = real_sock.to_string_lossy().into_owned().into_bytes();
    if let (Ok(pub_c), Ok(real_c)) = (CString::new(pub_bytes), CString::new(real_bytes)) {
        PUB_SOCK_PTR.store(pub_c.into_raw(), Ordering::SeqCst);
        REAL_SOCK_PTR.store(real_c.into_raw(), Ordering::SeqCst);
    }

    posix_signal(SIGHUP, handle_signal);
    posix_signal(SIGINT, handle_signal);
    posix_signal(SIGQUIT, handle_signal);
    posix_signal(SIGTERM, handle_signal);

    if !real_sock.exists() {
        if !pub_sock.exists() {
            eprintln!("[hypr-ipc-bridge] Error: {} does not exist", pub_sock.display());
            restore_sockets();
            std::process::exit(1);
        }
        if let Err(e) = fs::rename(&pub_sock, &real_sock) {
            eprintln!("[hypr-ipc-bridge] Failed to rename socket: {e}");
            restore_sockets();
            std::process::exit(1);
        }
    } else if pub_sock.exists() {
        let _ = fs::remove_file(&pub_sock);
    }

    let listener = match UnixListener::bind(&pub_sock) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[hypr-ipc-bridge] Failed to bind {}: {e}", pub_sock.display());
            restore_sockets();
            std::process::exit(1);
        }
    };

    // Monitor Hyprland socket2 to cleanly exit when compositor restarts
    if let Some(s2) = get_socket2() {
        thread::spawn(move || {
            watch_hyprland_lifecycle(s2);
        });
    }

    eprintln!(
        "[hypr-ipc-bridge] Listening on {}, forwarding to {}",
        pub_sock.display(),
        real_sock.display()
    );

    for stream in listener.incoming() {
        if !RUNNING.load(Ordering::SeqCst) {
            break;
        }
        match stream {
            Ok(client) => {
                let real_path = real_sock.clone();
                thread::spawn(move || {
                    handle_client(client, &real_path);
                });
            }
            Err(e) => {
                if RUNNING.load(Ordering::SeqCst) {
                    eprintln!("[hypr-ipc-bridge] Accept error: {e}");
                }
            }
        }
    }

    restore_sockets();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translations() {
        assert_eq!(
            translate("dispatch workspace e+1"),
            Some("dispatch hl.dsp.focus({ workspace = \"e+1\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace +1"),
            Some("dispatch hl.dsp.focus({ workspace = \"+1\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace -1"),
            Some("dispatch hl.dsp.focus({ workspace = \"-1\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace m+1"),
            Some("dispatch hl.dsp.focus({ workspace = \"m+1\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace previous"),
            Some("dispatch hl.dsp.focus({ workspace = \"previous\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace empty"),
            Some("dispatch hl.dsp.focus({ workspace = \"empty\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace r-1"),
            Some("dispatch hl.dsp.focus({ workspace = \"r-1\" })".to_string())
        );
        assert_eq!(
            translate("/dispatch workspace 2"),
            Some("dispatch hl.dsp.focus({ workspace = 2 })".to_string())
        );
        assert_eq!(
            translate("dispatch focusworkspaceoncurrentmonitor 3"),
            Some("dispatch hl.dsp.focus({ workspace = 3, on_current_monitor = true })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace name:gaming"),
            Some("dispatch hl.dsp.focus({ workspace = \"name:gaming\" })".to_string())
        );
        assert_eq!(
            translate("dispatch workspace gaming"),
            Some("dispatch hl.dsp.focus({ workspace = \"name:gaming\" })".to_string())
        );
        assert_eq!(
            translate("dispatch focusworkspaceoncurrentmonitor name:special"),
            Some("dispatch hl.dsp.focus({ workspace = \"name:special\", on_current_monitor = true })".to_string())
        );
        assert_eq!(
            translate("dispatch focusworkspaceoncurrentmonitor gaming"),
            Some("dispatch hl.dsp.focus({ workspace = \"name:gaming\", on_current_monitor = true })".to_string())
        );
        assert_eq!(
            translate("dispatch togglespecialworkspace"),
            Some("dispatch hl.dsp.workspace.toggle_special({})".to_string())
        );
        assert_eq!(
            translate("dispatch togglespecialworkspace special:magic"),
            Some("dispatch hl.dsp.workspace.toggle_special({ name = \"special:magic\" })".to_string())
        );
        assert_eq!(
            translate("dispatch movecursor 100 200"),
            Some("dispatch hl.dsp.cursor.move({ x = 100, y = 200 })".to_string())
        );
        assert_eq!(
            translate("dispatch movecursor -15 -30"),
            Some("dispatch hl.dsp.cursor.move({ x = -15, y = -30 })".to_string())
        );
        assert_eq!(
            translate("dispatch focusmonitor HDMI-A-1"),
            Some("dispatch hl.dsp.focus({ monitor = \"HDMI-A-1\" })".to_string())
        );
        assert_eq!(translate("monitors"), None);
        assert_eq!(translate("dispatch exec kitty"), None);
    }
}
