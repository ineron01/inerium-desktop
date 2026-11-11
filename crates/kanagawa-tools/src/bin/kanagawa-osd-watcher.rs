//! kanagawa-osd-watcher — Native Rust OSD daemon for Hyprland & SwayOSD
//!
//! Listens for:
//! 1. Keyboard layout changes from Hyprland event socket (.socket2.sock)
//! 2. MSI-EC webcam toggle via Fn+F6 using event-driven Linux inotify on /dev (0% CPU at idle)

use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use kanagawa_tools::ipc::hyprland::get_socket2;
use kanagawa_tools::sys::posix::*;

const WEBCAM_SYSFS: &str = "/sys/devices/platform/msi-ec/webcam";

#[repr(C)]
struct InotifyEventHeader {
    wd: i32,
    mask: u32,
    cookie: u32,
    len: u32,
}

fn osd(message: &str, icon: &str) {
    let _ = Command::new("swayosd-client")
        .args(["--custom-message", message, "--custom-icon", icon])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn is_physical_video_node(dev_name: &str) -> bool {
    let sysfs = std::path::Path::new("/sys/class/video4linux").join(dev_name);
    if let Ok(target) = fs::canonicalize(&sysfs)
        && target.to_string_lossy().contains("/devices/virtual/")
    {
        return false;
    }
    true
}

fn webcam_state() -> bool {
    if std::path::Path::new(WEBCAM_SYSFS).exists() {
        let ec_on = fs::read_to_string(WEBCAM_SYSFS)
            .map(|s| s.trim() == "on")
            .unwrap_or(false);
        if !ec_on {
            return false;
        }
    }

    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("video") && is_physical_video_node(&name) {
                return true;
            }
        }
    }

    false
}

fn webcam_watcher() {
    let mut prev = webcam_state();

    let fd = match posix_inotify_init1(IN_CLOEXEC) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[kanagawa-osd-watcher] Failed to init inotify: {e}");
            return;
        }
    };

    let dev_cstr = match std::ffi::CString::new("/dev") {
        Ok(c) => c,
        Err(_) => {
            let _ = posix_close(fd);
            return;
        }
    };

    if let Err(e) = posix_inotify_add_watch(fd, &dev_cstr, IN_CREATE | IN_DELETE) {
        eprintln!("[kanagawa-osd-watcher] Failed to add inotify watch on /dev: {e}");
        let _ = posix_close(fd);
        return;
    }

    let mut buf = [0u8; 4096];
    loop {
        let n = match posix_read(fd, &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };

        let mut offset = 0;
        let mut relevant_change = false;

        while offset + std::mem::size_of::<InotifyEventHeader>() <= n {
            let header = unsafe { &*(buf.as_ptr().add(offset) as *const InotifyEventHeader) };
            let name_len = header.len as usize;
            let name_start = offset + std::mem::size_of::<InotifyEventHeader>();
            let name_end = name_start + name_len;

            if name_end <= n && name_len > 0 {
                let name_bytes = &buf[name_start..name_end];
                if let Some(nul_pos) = name_bytes.iter().position(|&b| b == 0)
                    && let Ok(name_str) = std::str::from_utf8(&name_bytes[..nul_pos])
                        && name_str.starts_with("video") {
                            relevant_change = true;
                        }
            }
            offset += std::mem::size_of::<InotifyEventHeader>() + name_len;
        }

        if relevant_change {
            // Small pause for sysfs sync
            thread::sleep(Duration::from_millis(50));
            let cur = webcam_state();
            if cur != prev {
                prev = cur;
                if cur {
                    osd("Веб-камера включена", "camera-web-symbolic");
                } else {
                    osd("Веб-камера выключена", "camera-disabled-symbolic");
                }
            }
        }
    }

    let _ = posix_close(fd);
}

pub fn parse_layout_event(line: &str) -> Option<(&str, &str)> {
    let payload = line.strip_prefix("activelayout>>")?;
    let layout = payload.rsplit(',').next().unwrap_or(payload).trim();
    if layout.is_empty() {
        return None;
    }

    let key = layout.split_whitespace().next().unwrap_or("").to_lowercase();
    let res = match key.as_str() {
        "english" => ("EN · English", "input-keyboard-symbolic"),
        "russian" => ("RU · Русский", "input-keyboard-symbolic"),
        _ => (layout, "input-keyboard-symbolic"),
    };
    Some(res)
}

fn layout_watcher() {
    let mut last_layout = String::new();
    let mut last_time = Instant::now() - Duration::from_secs(10);

    loop {
        if let Some(s2_path) = get_socket2()
            && let Ok(stream) = UnixStream::connect(s2_path) {
                let reader = BufReader::new(stream);
                for line in reader.lines().map_while(Result::ok) {
                    if let Some((msg, icon)) = parse_layout_event(&line) {
                        let now = Instant::now();
                        // Debounce duplicate events from multiple keyboard devices (e.g. 2.4G dongle, laptop kb)
                        if msg == last_layout && now.duration_since(last_time) < Duration::from_millis(400) {
                            continue;
                        }
                        last_layout = msg.to_string();
                        last_time = now;

                        osd(msg, icon);
                    }
                }
            }
        thread::sleep(Duration::from_secs(2));
    }
}

fn main() {
    posix_signal_ign(SIGCHLD);

    thread::spawn(webcam_watcher);
    layout_watcher();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layout_event() {
        assert_eq!(
            parse_layout_event("activelayout>>at-translated-set-2-keyboard,English (US)"),
            Some(("EN · English", "input-keyboard-symbolic"))
        );
        assert_eq!(
            parse_layout_event("activelayout>>telink-vxe-v87-2.4g-dongle-keyboard,Russian"),
            Some(("RU · Русский", "input-keyboard-symbolic"))
        );
        assert_eq!(
            parse_layout_event("activelayout>>device,German"),
            Some(("German", "input-keyboard-symbolic"))
        );
        assert_eq!(parse_layout_event("workspace>>1"), None);
    }
}
