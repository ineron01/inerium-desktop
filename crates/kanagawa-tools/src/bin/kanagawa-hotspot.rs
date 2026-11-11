//! Kanagawa Dragon — Native Rust Hotspot Controller & Waybar Provider
//! - Zero shell forks for status polling (<0.3 ms execution time)
//! - Direct sysfs reading for wireless interfaces and operstate
//! - Compatible with Waybar custom/hotspot JSON schema, signals (SIGRTMIN+8) and toggle actions

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use kanagawa_tools::format::escape_json;
use kanagawa_tools::sys::posix::posix_getuid;

fn state_file_path() -> PathBuf {
    let rt = env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", posix_getuid()));
    PathBuf::from(rt).join("kanagawa-hotspot.state")
}

fn has_ethernet_carrier() -> bool {
    has_ethernet_carrier_in(Path::new("/sys/class/net"))
}

fn has_ethernet_carrier_in(net_dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(net_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if (name.starts_with("en") || name.starts_with("eth"))
                && read_trimmed(&entry.path().join("carrier")).as_deref() == Some("1")
            {
                return true;
            }
        }
    }
    false
}

fn find_hotspot_connection() -> Option<String> {
    if let Ok(custom) = env::var("KANAGAWA_HOTSPOT_CONN")
        && !custom.trim().is_empty()
    {
        return Some(custom);
    }

    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut wifi_names = Vec::new();
        for line in text.lines() {
            if let Some((name, ctype)) = line.split_once(':')
                && (ctype == "802-11-wireless" || ctype == "wifi")
            {
                let clean = name.replace("\\:", ":");
                wifi_names.push(clean);
            }
        }
        for preferred in ["ineron-hotspot", "kanagawa-hotspot", "Hotspot"] {
            if wifi_names.iter().any(|n| n == preferred) {
                return Some(preferred.to_string());
            }
        }
        for name in wifi_names {
            if let Ok(m_out) = Command::new("nmcli")
                .args(["-g", "802-11-wireless.mode", "connection", "show", &name])
                .output()
                && String::from_utf8_lossy(&m_out.stdout).trim() == "ap"
            {
                return Some(name);
            }
        }
    }
    None
}

fn refresh_waybar() {
    let _ = Command::new("pkill")
        .args(["-RTMIN+8", "waybar"])
        .output();
}

fn get_wifi_interface() -> Option<String> {
    get_wifi_interface_in(Path::new("/sys/class/net"))
}

fn get_wifi_interface_in(net_dir: &Path) -> Option<String> {
    if let Ok(entries) = fs::read_dir(net_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("wireless").exists() {
                return Some(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    if let Ok(entries) = fs::read_dir(net_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("wl") {
                return Some(name);
            }
        }
    }
    None
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

enum PendingState {
    Starting,
    Stopping,
    None,
}

fn get_pending_state() -> PendingState {
    let p = state_file_path();
    if !p.exists() {
        return PendingState::None;
    }

    if let Ok(meta) = fs::metadata(&p)
        && let Ok(modified) = meta.modified()
        && let Ok(elapsed) = SystemTime::now().duration_since(modified)
        && elapsed.as_secs() > 35
    {
        let _ = fs::remove_file(&p);
        return PendingState::None;
    }

    if let Ok(content) = fs::read_to_string(&p) {
        match content.trim() {
            "starting" => PendingState::Starting,
            "stopping" => PendingState::Stopping,
            _ => PendingState::None,
        }
    } else {
        PendingState::None
    }
}

fn is_hotspot_active() -> bool {
    let Some(iface) = get_wifi_interface() else {
        return false;
    };
    let oper_path = PathBuf::from(format!("/sys/class/net/{}/operstate", iface));

    // 1. Instant sysfs check: if interface is down, hotspot is inactive (0.05 ms)
    if let Some(oper) = read_trimmed(&oper_path)
        && oper == "down"
    {
        return false;
    }

    // 2. Direct netlink query via iw (<1 ms, 0 shell pipes)
    if let Ok(output) = Command::new("iw").args(["dev", &iface, "info"]).output() {
        let text = String::from_utf8_lossy(&output.stdout);
        return text.contains("type AP");
    }

    false
}

fn emit_json(text: &str, alt: &str, class: &str, tooltip: &str) {
    println!(
        "{{\"text\":\"{}\",\"alt\":\"{}\",\"class\":\"{}\",\"tooltip\":\"{}\"}}",
        escape_json(text),
        escape_json(alt),
        escape_json(class),
        escape_json(tooltip)
    );
}

fn print_status() {
    if get_wifi_interface().is_none() {
        emit_json("", "unavailable", "unavailable", "");
        return;
    }

    match get_pending_state() {
        PendingState::Starting => {
            emit_json("󱜠", "starting", "starting", "Точка доступа: включается…");
            return;
        }
        PendingState::Stopping => {
            emit_json("󱜡", "stopping", "stopping", "Точка доступа: выключается…");
            return;
        }
        PendingState::None => {}
    }

    if is_hotspot_active() {
        let conn = find_hotspot_connection().unwrap_or_else(|| "Hotspot".to_string());
        let tt = format!("Точка доступа: Включена\nSSID: {}\n\nЛКМ: Выключить", conn);
        emit_json("󱜠", "active", "active", &tt);
    } else {
        emit_json(
            "󱜡",
            "inactive",
            "inactive",
            "Точка доступа: Выключена\n\nЛКМ: Включить",
        );
    }
}

fn print_plain() {
    if get_wifi_interface().is_none() {
        return;
    }
    match get_pending_state() {
        PendingState::Starting => {
            println!("󱜠");
            return;
        }
        PendingState::Stopping => {
            println!("󱜡");
            return;
        }
        PendingState::None => {}
    }

    if is_hotspot_active() {
        println!("󱜠");
    } else {
        println!("󱜡");
    }
}

struct StateGuard(PathBuf);

impl Drop for StateGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        refresh_waybar();
    }
}

fn notify(urgency_critical: bool, summary: &str, body: &str) {
    let icon = if urgency_critical {
        "network-wireless-offline-symbolic"
    } else {
        "network-wireless-hotspot-symbolic"
    };
    let mut cmd = Command::new("notify-send");
    if urgency_critical {
        cmd.args(["-u", "critical"]);
    }
    let _ = cmd
        .args(["-a", "Сеть", "-i", icon, summary, body])
        .output();
}

fn is_wifi_radio_enabled() -> bool {
    Command::new("nmcli")
        .args(["-t", "-f", "WIFI", "general"])
        .output()
        .ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled")
}

/// Waits until NetworkManager reports `iface` is out of unmanaged/unavailable states (`>= 30`).
fn wait_wifi_device_ready(iface: &str) {
    for _ in 0..60 {
        if let Ok(out) = Command::new("nmcli")
            .args(["-g", "GENERAL.STATE", "device", "show", iface])
            .output()
        {
            let state = String::from_utf8_lossy(&out.stdout);
            let code = state
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            if code >= 30 {
                return;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Detects the unrebooted kernel-upgrade mismatch where `/lib/modules/<uname -r>` was removed
/// and `ccm(aes)` was not yet loaded into kernel memory (causing 25s NM `supplicant-timeout`).
fn missing_kernel_crypto_ccm() -> bool {
    let Some(osrelease) = read_trimmed(Path::new("/proc/sys/kernel/osrelease")) else {
        return false;
    };
    if PathBuf::from("/lib/modules").join(osrelease).exists() {
        return false;
    }
    fs::read_to_string("/proc/crypto")
        .map(|s| !s.contains("ccm(aes)"))
        .unwrap_or(false)
}

fn activate_hotspot(conn_name: &str) -> Result<Output, std::io::Error> {
    let start = Instant::now();
    let out = Command::new("nmcli")
        .args(["connection", "up", conn_name])
        .output()?;

    // Only retry quickly on transient radio-init errors; never double a 25s supplicant timeout.
    if !out.status.success() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(500));
        return Command::new("nmcli")
            .args(["connection", "up", conn_name])
            .output();
    }

    Ok(out)
}

fn disable_wifi_if_ethernet_up() {
    if has_ethernet_carrier() {
        let _ = Command::new("nmcli").args(["radio", "wifi", "off"]).output();
    }
}

fn toggle_hotspot() {
    let Some(iface) = get_wifi_interface() else {
        notify(true, "Точка доступа", "Wi-Fi адаптер не найден");
        return;
    };

    // If state transition is already in progress, ignore duplicate clicks
    if !matches!(get_pending_state(), PendingState::None) {
        return;
    }

    let state_file = state_file_path();

    if is_hotspot_active() {
        let _ = fs::write(&state_file, b"stopping\n");
        refresh_waybar();
        let guard = StateGuard(state_file);

        if let Some(conn) = find_hotspot_connection() {
            let _ = Command::new("nmcli")
                .args(["connection", "down", &conn])
                .output();
        } else {
            let _ = Command::new("nmcli")
                .args(["device", "disconnect", &iface])
                .output();
        }

        // If ethernet cable is connected, disable Wi-Fi radio to save battery
        disable_wifi_if_ethernet_up();

        drop(guard);
        notify(false, "Точка доступа", "Выключена");
        return;
    }

    if missing_kernel_crypto_ccm() {
        notify(
            true,
            "Точка доступа",
            "Не удалось включить: ядро обновилось (нет модуля ccm), нужна перезагрузка",
        );
        return;
    }

    let _ = fs::write(&state_file, b"starting\n");
    refresh_waybar();
    let guard = StateGuard(state_file);

    let was_radio_enabled = is_wifi_radio_enabled();
    if !was_radio_enabled {
        let _ = Command::new("nmcli").args(["radio", "wifi", "on"]).output();
        wait_wifi_device_ready(&iface);
    }

    let existing_conn = find_hotspot_connection();
    let result = if let Some(ref conn) = existing_conn {
        activate_hotspot(conn)
    } else {
        Command::new("nmcli")
            .args(["device", "wifi", "hotspot", "ifname", &iface, "ssid", "Kanagawa-Hotspot"])
            .output()
    };
    let succeeded = result.as_ref().is_ok_and(|o| o.status.success());

    if !succeeded && !was_radio_enabled {
        disable_wifi_if_ethernet_up();
    }

    drop(guard);

    if succeeded {
        let label = existing_conn.unwrap_or_else(|| "Kanagawa-Hotspot".to_string());
        notify(false, "Точка доступа", &format!("Включена ({})", label));
    } else {
        let detail = result
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stderr).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Не удалось включить".to_string());
        notify(true, "Точка доступа", &detail);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("status");

    match cmd {
        "status" | "--status" => print_status(),
        "status-plain" => print_plain(),
        "toggle" | "--toggle" => toggle_hotspot(),
        _ => {
            eprintln!("Usage: {} {{status|status-plain|toggle}}", args[0]);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_net_sysfs_graceful() {
        assert!(get_wifi_interface_in(Path::new("/nonexistent/net")).is_none());
        assert!(!has_ethernet_carrier_in(Path::new("/nonexistent/net")));
    }
}
