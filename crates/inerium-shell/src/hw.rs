use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir())
}

pub fn config_dir() -> PathBuf {
    env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".config"))
}

pub fn cache_dir() -> PathBuf {
    env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".cache"))
}

pub fn has_backlight() -> bool {
    has_backlight_in(Path::new("/sys/class/backlight"))
}

pub fn has_backlight_in(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|mut entries| entries.any(|e| e.is_ok()))
        .unwrap_or(false)
}

pub fn has_msi_ec() -> bool {
    has_msi_ec_at(Path::new("/sys/devices/platform/msi-ec"))
}

pub fn has_msi_ec_at(path: &Path) -> bool {
    path.exists()
}

fn find_battery_dir() -> Option<PathBuf> {
    find_battery_dir_in(Path::new("/sys/class/power_supply"))
}

fn find_battery_dir_in(base: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(base).ok()?;
    let mut fallback: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let ptype = fs::read_to_string(path.join("type")).unwrap_or_default();
        if ptype.trim() != "Battery" {
            continue;
        }
        let scope = fs::read_to_string(path.join("scope")).unwrap_or_default();
        if scope.trim() == "Device" {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("BAT") {
            return Some(path);
        }
        if fallback.is_none() {
            fallback = Some(path);
        }
    }
    fallback
}

pub fn find_battery_threshold_path() -> Option<PathBuf> {
    let dir = find_battery_dir()?;
    for name in &["charge_control_end_threshold", "charge_stop_threshold"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub fn has_battery_threshold() -> bool {
    find_battery_threshold_path().is_some()
}

pub struct AudioSink {
    pub id: u32,
    pub name: String,
    pub is_default: bool,
}

// -------------------------------------------------------------
// BACKLIGHT (Direct sysfs read/write + brightnessctl fallback)
// -------------------------------------------------------------
pub fn get_laptop_brightness() -> u32 {
    let base = fs::read_dir("/sys/class/backlight")
        .ok()
        .and_then(|entries| entries.flatten().next().map(|e| e.path()));
    if let Some(p) = base {
        let cur_res = fs::read_to_string(p.join("brightness"));
        let max_res = fs::read_to_string(p.join("max_brightness"));
        if let (Ok(cur), Ok(max)) = (cur_res, max_res)
            && let (Ok(c), Ok(m)) = (cur.trim().parse::<f64>(), max.trim().parse::<f64>())
                && m > 0.0 {
                    return ((c / m) * 100.0).round() as u32;
                }
    }
    50
}

pub fn set_laptop_brightness(pct: u32) {
    let clamped = pct.clamp(5, 100);
    thread::spawn(move || {
        let _ = Command::new("brightnessctl")
            .args(["-q", "s", &format!("{}%", clamped)])
            .output();
    });
}

// -------------------------------------------------------------
// DDC/CI (Serialized worker channel to prevent I2C flock collision)
// -------------------------------------------------------------
enum DdcCmd {
    Set(u32),
}

static DDC_TX: OnceLock<Sender<DdcCmd>> = OnceLock::new();
static DDC_CACHE: Mutex<u32> = Mutex::new(50);
static DDC_BUS: OnceLock<String> = OnceLock::new();

fn get_ddc_bus() -> &'static str {
    DDC_BUS.get_or_init(|| {
        if let Ok(output) = Command::new("ddcutil").args(["detect", "--terse"]).output() {
            let s = String::from_utf8_lossy(&output.stdout);
            for line in s.lines() {
                if let Some(pos) = line.find("/dev/i2c-") {
                    let num: String = line[pos + 9..].chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !num.is_empty() {
                        return num;
                    }
                }
            }
        }
        "3".to_string()
    })
}

pub fn has_external_monitor() -> bool {
    let card_dir = "/sys/class/drm";
    if let Ok(entries) = fs::read_dir(card_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("card") && name.contains("HDMI") {
                let status_path = entry.path().join("status");
                if let Ok(status) = fs::read_to_string(&status_path)
                    && status.trim() == "connected" {
                        return true;
                    }
            }
        }
    }
    false
}

fn init_ddc_worker() -> Sender<DdcCmd> {
    let (tx, rx) = channel::<DdcCmd>();
    thread::spawn(move || {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                DdcCmd::Set(val) => {
                    let bus = get_ddc_bus();
                    let _ = Command::new("ddcutil")
                        .args(["setvcp", "10", &val.to_string(), "--bus", bus])
                        .output();
                    if let Ok(mut g) = DDC_CACHE.lock() {
                        *g = val;
                    }
                }
            }
        }
    });
    tx
}

pub fn get_cached_ddc_brightness() -> u32 {
    DDC_CACHE.lock().map(|g| *g).unwrap_or(50)
}

pub fn refresh_ddc_brightness_async() {
    if !has_external_monitor() {
        return;
    }
    thread::spawn(move || {
        let bus = get_ddc_bus();
        if let Ok(output) = Command::new("ddcutil")
            .args(["getvcp", "10", "--bus", bus, "--brief"])
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 4
                && let Ok(v) = parts[3].parse::<u32>() {
                    if let Ok(mut g) = DDC_CACHE.lock() {
                        *g = v;
                    }
                    glib::MainContext::default().invoke(move || {
                        crate::views::control_center::on_ddc_updated(v);
                    });
                }
        }
    });
}

#[allow(dead_code)]
pub fn get_ddc_brightness() -> u32 {
    get_cached_ddc_brightness()
}

pub fn set_ddc_brightness_async(val: u32) {
    if !has_external_monitor() {
        return;
    }
    let tx = DDC_TX.get_or_init(init_ddc_worker);
    let _ = tx.send(DdcCmd::Set(val.clamp(0, 100)));
}

// -------------------------------------------------------------
// AUDIO (WirePlumber / wpctl)
// -------------------------------------------------------------
pub fn get_volume() -> u32 {
    if let Ok(output) = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output() {
        let s = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2
            && let Ok(val) = parts[1].parse::<f64>() {
                return (val * 100.0).round() as u32;
            }
    }
    75
}

pub fn is_volume_muted() -> bool {
    if let Ok(output) = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output() {
        let s = String::from_utf8_lossy(&output.stdout);
        return s.contains("MUTED");
    }
    false
}

pub fn set_volume(pct: u32) {
    let v = pct.clamp(0, 150) as f64 / 100.0;
    thread::spawn(move || {
        let _ = Command::new("wpctl")
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", v)])
            .output();
    });
}

pub fn toggle_volume_mute() {
    thread::spawn(|| {
        let _ = Command::new("swayosd-client").args(["--output-volume", "mute-toggle"]).output();
    });
}

pub fn is_mic_muted() -> bool {
    if let Ok(output) = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SOURCE@"]).output() {
        let s = String::from_utf8_lossy(&output.stdout);
        return s.contains("MUTED");
    }
    false
}

pub fn toggle_mic_mute() {
    thread::spawn(|| {
        let _ = Command::new("swayosd-client").args(["--input-volume", "mute-toggle"]).output();
    });
}

pub fn list_audio_sinks() -> Vec<AudioSink> {
    let mut sinks = Vec::new();
    if let Ok(output) = Command::new("wpctl").arg("status").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut in_sinks = false;
        for line in text.lines() {
            if line.contains("Sinks:") {
                in_sinks = true;
                continue;
            }
            if in_sinks {
                if line.contains("Sources:") || line.contains("Filters:") || line.contains("Streams:") {
                    break;
                }
                let clean_line = line.replace(['│', '├', '└', '─'], "");
                let trimmed = clean_line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let is_default = trimmed.starts_with('*');
                let clean = trimmed.trim_start_matches('*').trim();
                let mut parts = clean.splitn(2, '.');
                if let (Some(id_str), Some(rest)) = (parts.next(), parts.next())
                    && let Ok(id) = id_str.trim().parse::<u32>() {
                        let name = rest.split('[').next().unwrap_or(rest).trim().to_string();
                        sinks.push(AudioSink {
                            id,
                            name,
                            is_default,
                        });
                    }
            }
        }
    }
    sinks
}

pub fn set_default_sink(id: u32) {
    thread::spawn(move || {
        let _ = Command::new("wpctl").args(["set-default", &id.to_string()]).output();
    });
}

// -------------------------------------------------------------
// MSI-EC & HARDWARE CHIPS
// -------------------------------------------------------------
const CB_PATH: &str = "/sys/devices/platform/msi-ec/cooler_boost";

pub fn get_cooler_boost() -> bool {
    if !has_msi_ec() {
        return false;
    }
    fs::read_to_string(CB_PATH)
        .map(|s| s.trim() == "on")
        .unwrap_or(false)
}

pub fn set_cooler_boost(on: bool) {
    if !has_msi_ec() {
        return;
    }
    let val = if on { "on\n" } else { "off\n" };
    let _ = fs::write(CB_PATH, val);
    if !on {
        apply_balanced_fan_curve();
    }
}

pub fn get_battery_threshold() -> u32 {
    let Some(path) = find_battery_threshold_path() else {
        return 100;
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(100)
}

pub fn set_battery_threshold(thresh: u32) {
    if let Some(path) = find_battery_threshold_path() {
        let _ = fs::write(path, format!("{}\n", thresh));
    }
}

pub fn get_fan_mode() -> String {
    fs::read_to_string("/sys/devices/platform/msi-ec/fan_mode")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "advanced".to_string())
}

#[allow(dead_code)]
pub fn format_fan_meta() -> String {
    let mode = get_fan_mode();
    let (cpu_rpm, gpu_rpm, cpu_pct, gpu_pct) = get_fan_telemetry();
    let c_pct = cpu_pct.unwrap_or(0);
    let g_pct = gpu_pct.unwrap_or(0);

    match (cpu_rpm, gpu_rpm) {
        (Some(cr), Some(gr)) if cr > 0 && gr > 0 => {
            format!("msi-ec: {}  ·  CPU {}% ({} RPM)  ·  GPU {}% ({} RPM)", mode, c_pct, cr, g_pct, gr)
        }
        (Some(cr), _) if cr > 0 => {
            format!("msi-ec: {}  ·  CPU {}% ({} RPM)  ·  GPU {}%", mode, c_pct, cr, g_pct)
        }
        _ => {
            format!("msi-ec: {}  ·  CPU {}%  ·  GPU {}%", mode, c_pct, g_pct)
        }
    }
}

/// Ensures fan mode is set to advanced (custom curves in EC RAM).
/// Note: Flashing the curve bytes into EC RAM is handled by the root daemon `ppd-msi-sync`.
pub fn apply_balanced_fan_curve() {
    let _ = fs::write("/sys/devices/platform/msi-ec/fan_mode", "advanced\n");
}

pub fn get_fan_telemetry() -> (Option<u32>, Option<u32>, Option<u32>, Option<u32>) {
    let mut cpu_rpm = None;
    let mut gpu_rpm = None;

    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(name) = fs::read_to_string(path.join("name"))
                && name.trim() == "msi_wmi_platform" {
                    if let Ok(r1) = fs::read_to_string(path.join("fan1_input")) {
                        cpu_rpm = r1.trim().parse::<u32>().ok();
                    }
                    if let Ok(r2) = fs::read_to_string(path.join("fan2_input")) {
                        gpu_rpm = r2.trim().parse::<u32>().ok();
                    }
                    break;
                }
        }
    }

    let cpu_pct = fs::read_to_string("/sys/devices/platform/msi-ec/cpu/realtime_fan_speed")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    let gpu_pct = fs::read_to_string("/sys/devices/platform/msi-ec/gpu/realtime_fan_speed")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    (cpu_rpm, gpu_rpm, cpu_pct, gpu_pct)
}

#[allow(dead_code)]
pub fn format_fan_telemetry() -> String {
    let (cpu_rpm, gpu_rpm, cpu_pct, gpu_pct) = get_fan_telemetry();
    let c_pct = cpu_pct.unwrap_or(0);
    let g_pct = gpu_pct.unwrap_or(0);

    let c_rpm_str = match cpu_rpm {
        Some(rpm) if rpm > 0 => format!("{} RPM", rpm),
        _ => "0 RPM".to_string(),
    };
    let g_rpm_str = match gpu_rpm {
        Some(rpm) if rpm > 0 => format!("{} RPM", rpm),
        _ => "0 RPM".to_string(),
    };

    format!("󰈨 Кулеры: CPU {}% ({}) · GPU {}% ({})", c_pct, c_rpm_str, g_pct, g_rpm_str)
}

// -------------------------------------------------------------
// POWER PROFILES
// -------------------------------------------------------------
static HAS_POWER_PROFILES: OnceLock<bool> = OnceLock::new();

pub fn has_power_profiles() -> bool {
    *HAS_POWER_PROFILES.get_or_init(|| {
        Command::new("powerprofilesctl")
            .arg("get")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

pub fn get_power_profile() -> String {
    if let Ok(output) = Command::new("powerprofilesctl").arg("get").output() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    "balanced".to_string()
}

pub fn set_power_profile(profile: &str) {
    let p = profile.to_string();
    thread::spawn(move || {
        let _ = Command::new("powerprofilesctl").args(["set", &p]).output();
        if has_msi_ec() {
            let shift = match p.as_str() {
                "power-saver" => "eco",
                "performance" => "turbo",
                _ => "comfort",
            };
            let _ = fs::write("/sys/devices/platform/msi-ec/shift_mode", format!("{}\n", shift));
        }
    });
}

// -------------------------------------------------------------
// IDLE INHIBITOR (HYPRIDLE)
// -------------------------------------------------------------
pub fn is_hypridle_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "hypridle.service"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

pub fn toggle_hypridle(enable: bool) {
    let action = if enable { "start" } else { "stop" };
    thread::spawn(move || {
        let _ = Command::new("systemctl")
            .args(["--user", action, "hypridle.service"])
            .output();
    });
}

// -------------------------------------------------------------
// WALLPAPERS
// -------------------------------------------------------------
pub struct WallpaperItem {
    pub path: String,
    pub name: String,
    pub thumb_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MonitorTarget {
    pub name: String,
    pub label: String,
    #[allow(dead_code)]
    pub is_laptop: bool,
}

pub fn list_connected_monitors() -> Vec<MonitorTarget> {
    let mut list = Vec::new();
    if let Ok(output) = Command::new("hyprctl").args(["monitors", "-j"]).output()
        && let Ok(val) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            && let Some(arr) = val.as_array() {
                for item in arr {
                    if let Some(name) = item["name"].as_str() {
                        let is_laptop = name.starts_with("eDP") || name.starts_with("LVDS");
                        let label = if is_laptop {
                            format!("󰌢 {}", name)
                        } else {
                            format!("󰍹 {}", name)
                        };
                        list.push(MonitorTarget {
                            name: name.to_string(),
                            label,
                            is_laptop,
                        });
                    }
                }
            }
    list
}

pub fn get_active_wallpapers() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(output) = Command::new("hyprctl").args(["hyprpaper", "listactive"]).output() {
        let s = String::from_utf8_lossy(&output.stdout);
        for line in s.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                let mon = parts[0].trim().to_string();
                let path = parts[1].trim().to_string();
                if !mon.is_empty() && !path.is_empty() {
                    map.insert(mon, path);
                }
            }
        }
    }

    if map.is_empty() {
        let conf_path = config_dir().join("hypr/hyprpaper.conf");
        if let Ok(content) = fs::read_to_string(conf_path) {
            let mut cur_mon = String::new();
            for line in content.lines() {
                let tr = line.trim();
                if tr.starts_with("monitor =") {
                    if let Some(val) = tr.split('=').nth(1) {
                        cur_mon = val.trim().to_string();
                    }
                } else if tr.starts_with("path =")
                    && let Some(val) = tr.split('=').nth(1) {
                        let p = val.trim().to_string();
                        if !cur_mon.is_empty() {
                            map.insert(cur_mon.clone(), p);
                        } else {
                            map.insert("fallback".to_string(), p);
                        }
                    }
            }
        }
    }
    map
}

#[allow(dead_code)]
pub fn get_active_wallpaper() -> String {
    let map = get_active_wallpapers();
    map.values().next().cloned().unwrap_or_default()
}

pub fn list_available_wallpapers() -> Vec<WallpaperItem> {
    let mut items = Vec::new();
    let home = home_dir();
    let dirs = [
        home.join("Pictures/Wallpapers"),
        home.join("Обои"),
        home.join("Wallpapers"),
    ];
    let thumb_base = cache_dir().join("inerium-wall-thumbs");
    let mut seen_names = std::collections::HashSet::new();

    for d in &dirs {
        if let Ok(entries) = fs::read_dir(d) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    if ["jpg", "jpeg", "png", "webp"].contains(&ext.as_str()) {
                        let filename = p.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
                        if !seen_names.insert(filename.clone()) {
                            continue;
                        }
                        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        let path_str = p.to_string_lossy().to_string();

                        let thumb_candidate = thumb_base.join(format!("{}.84x48.png", filename));
                        let thumb_path = if thumb_candidate.exists() {
                            Some(thumb_candidate.to_string_lossy().into_owned())
                        } else {
                            None
                        };

                        items.push(WallpaperItem {
                            path: path_str,
                            name,
                            thumb_path,
                        });
                    }
                }
            }
        }
    }
    items
}

pub fn set_wallpaper_async(path: String, monitor: Option<String>) {
    thread::spawn(move || {
        let bin_path = home_dir().join(".local/bin/set-wallpaper");
        let mut cmd = if bin_path.exists() {
            Command::new(bin_path)
        } else {
            Command::new("set-wallpaper")
        };
        cmd.arg(&path);
        if let Some(ref m) = monitor
            && m != "all" {
                cmd.arg(m);
            }
        let _ = cmd.status();
    });
}

// -------------------------------------------------------------
// DND (Mako Notifications)
// -------------------------------------------------------------
pub fn is_dnd_active() -> bool {
    if let Ok(output) = Command::new("makoctl").arg("mode").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let l = line.trim();
            if l == "dnd" || l == "do-not-disturb" {
                return true;
            }
        }
    }
    false
}

pub fn toggle_dnd() {
    thread::spawn(|| {
        let _ = Command::new("inerium-dnd").arg("--toggle").output();
    });
}

// -------------------------------------------------------------
// SYSTEM PACKAGE UPDATES
// -------------------------------------------------------------
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[derive(Default)]
pub struct SystemUpdates {
    pub total: usize,
    pub repo: usize,
    pub aur: usize,
    pub timestamp: u64,
    pub sample_packages: Vec<String>,
}


unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

pub fn get_updates_info() -> SystemUpdates {
    let uid = unsafe { libc_getuid() };
    let cache_path = format!("/run/user/{}/inerium-updates-cache.json", uid);

    let mut cache: Option<SystemUpdates> = None;
    let mut need_refresh = false;

    if let Ok(data) = fs::read_to_string(&cache_path) {
        if let Ok(parsed) = serde_json::from_str::<SystemUpdates>(&data) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // Auto refresh in background if cache is older than 45 minutes
            if now.saturating_sub(parsed.timestamp) > 2700 {
                need_refresh = true;
            }
            cache = Some(parsed);
        } else {
            need_refresh = true;
        }
    } else {
        need_refresh = true;
    }

    if need_refresh {
        thread::spawn(|| {
            let _ = Command::new("inerium-updates").arg("--check").output();
        });
    }

    cache.unwrap_or_default()
}

// -------------------------------------------------------------
// WI-FI & BLUETOOTH TOGGLES
// -------------------------------------------------------------
pub fn is_wifi_enabled() -> bool {
    if let Ok(output) = Command::new("nmcli").args(["radio", "wifi"]).output() {
        return String::from_utf8_lossy(&output.stdout).trim() == "enabled";
    }
    true
}

pub fn get_active_wifi_ssid() -> Option<String> {
    // 1. Быстрая прямая проверка через iw link (2 мс)
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let iface = entry.file_name().to_string_lossy().into_owned();
            if iface.starts_with("wl")
                && let Ok(out) = Command::new("iw").args(["dev", &iface, "link"]).output() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    for line in text.lines() {
                        let trimmed = line.trim();
                        if let Some(rest) = trimmed.strip_prefix("SSID: ") {
                            let s = rest.trim();
                            if !s.is_empty() {
                                return Some(s.to_string());
                            }
                        }
                    }
                }
        }
    }

    // 2. Резервная проверка активного подключения через nmcli
    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "TYPE,NAME", "connection", "show", "--active"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && (parts[0] == "802-11-wireless" || parts[0] == "wifi") {
                let name = parts[1].replace("\\:", ":").trim().to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }

    None
}

pub fn toggle_wifi(enable: bool) {
    thread::spawn(move || {
        let val = if enable { "on" } else { "off" };
        let _ = Command::new("nmcli").args(["radio", "wifi", val]).output();
    });
}

pub fn is_hotspot_active() -> bool {
    // 1. Проверка режима AP через iw (1 мс)
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let iface = entry.file_name().to_string_lossy().into_owned();
            if iface.starts_with("wl")
                && let Ok(out) = Command::new("iw").args(["dev", &iface, "info"]).output()
                    && String::from_utf8_lossy(&out.stdout).contains("type AP") {
                        return true;
                    }
        }
    }

    // 2. Проверка активного соединения hotspot через nmcli
    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "TYPE,NAME", "connection", "show", "--active"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && (parts[0] == "802-11-wireless" || parts[0] == "wifi") {
                let name = parts[1].replace("\\:", ":").trim().to_string();
                if name == "Hotspot" || name == "inerium-hotspot" {
                    return true;
                }
            }
        }
    }

    false
}

pub fn has_bluetooth() -> bool {
    fs::read_dir("/sys/class/bluetooth")
        .map(|mut rd| rd.next().is_some())
        .unwrap_or(false)
}

pub fn is_bluetooth_enabled() -> bool {
    if !has_bluetooth() {
        return false;
    }

    // 1. Проверка rfkill
    if let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") {
        for entry in entries.flatten() {
            let tpath = entry.path().join("type");
            if let Ok(t) = std::fs::read_to_string(&tpath) && t.trim() == "bluetooth" {
                let spath = entry.path().join("state");
                if let Ok(s) = std::fs::read_to_string(&spath) && s.trim() == "0" {
                    return false;
                }
            }
        }
    }

    // 2. Проверка bluetoothctl show с внешним timeout (чтобы не зависать при мёртвом bluetoothd)
    if let Ok(output) = Command::new("timeout")
        .args(["2", "bluetoothctl", "show"])
        .stdin(Stdio::null())
        .output()
    {
        return String::from_utf8_lossy(&output.stdout).contains("Powered: yes");
    }
    false
}

pub fn toggle_bluetooth(enable: bool) {
    thread::spawn(move || {
        if enable {
            let _ = Command::new("rfkill").args(["unblock", "bluetooth"]).output();
            for _ in 0..10 {
                if let Ok(out) = Command::new("bluetoothctl").arg("show").output()
                    && String::from_utf8_lossy(&out.stdout).contains("Powered: yes") {
                        break;
                    }
                let _ = Command::new("bluetoothctl").args(["power", "on"]).output();
                thread::sleep(std::time::Duration::from_millis(100));
            }
        } else {
            let _ = Command::new("bluetoothctl").args(["power", "off"]).output();
        }
        let _ = Command::new("pkill").args(["-SIGUSR1", "inerium-bluetooth"]).output();
    });
}

// -------------------------------------------------------------
// ENVYCONTROL (GPU Switch)
// -------------------------------------------------------------
static HAS_ENVYCONTROL: OnceLock<bool> = OnceLock::new();

pub fn has_envycontrol() -> bool {
    *HAS_ENVYCONTROL.get_or_init(|| {
        Command::new("envycontrol")
            .arg("--query")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

pub fn get_gpu_mode() -> String {
    if let Ok(output) = Command::new("envycontrol").arg("--query").output() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
        if s.contains("hybrid") {
            return "hybrid".to_string();
        } else if s.contains("integrated") {
            return "integrated".to_string();
        } else if s.contains("nvidia") {
            return "nvidia".to_string();
        }
    }
    "hybrid".to_string()
}

pub fn switch_gpu_mode(mode: &str) -> bool {
    let out = Command::new("sudo")
        .args(["-n", "envycontrol", "-s", mode])
        .output();
    matches!(out, Ok(res) if res.status.success())
}

// -------------------------------------------------------------
// SYSTEM META (Uptime, Battery line, Hostname)
// -------------------------------------------------------------
pub fn get_hostname() -> String {
    if let Ok(s) = fs::read_to_string("/etc/hostname") {
        let t = s.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    "inerium-desktop".to_string()
}

pub fn get_uptime_line() -> String {
    if let Ok(s) = fs::read_to_string("/proc/uptime")
        && let Some(sec_str) = s.split_whitespace().next()
            && let Ok(sec) = sec_str.parse::<f64>() {
                let s_total = sec as u64;
                let hrs = s_total / 3600;
                let mins = (s_total % 3600) / 60;
                return format!("аптайм {}ч {}м", hrs, mins);
            }
    "аптайм 0ч 0м".to_string()
}

pub fn get_battery_line() -> String {
    if let Some(bat_dir) = find_battery_dir() {
        let cap_res = fs::read_to_string(bat_dir.join("capacity"));
        let status_res = fs::read_to_string(bat_dir.join("status"));
        if let Ok(cap) = cap_res {
            let c = cap.trim();
            let st = status_res.unwrap_or_default();
            let is_charging = st.trim() == "Charging";
            let ico = if is_charging { "󰂄" } else { "󰂀" };
            return format!("{} {}%", ico, c);
        }
    }
    "󰚥 Сеть".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_detection_nonexistent_paths() {
        assert!(!has_msi_ec_at(Path::new("/nonexistent/msi-ec")));
        assert!(!has_backlight_in(Path::new("/nonexistent/backlight")));
        assert!(find_battery_dir_in(Path::new("/nonexistent/power_supply")).is_none());
    }

    #[test]
    fn test_find_battery_dir_prefers_bat_and_skips_device() {
        let root = env::temp_dir().join("inerium_shell_test_ps");
        let _ = fs::remove_dir_all(&root);
        let dev = root.join("hidpp_battery_0");
        let bat0 = root.join("BAT0");
        fs::create_dir_all(&dev).unwrap();
        fs::create_dir_all(&bat0).unwrap();
        fs::write(dev.join("type"), "Battery\n").unwrap();
        fs::write(dev.join("scope"), "Device\n").unwrap();
        fs::write(bat0.join("type"), "Battery\n").unwrap();

        assert_eq!(find_battery_dir_in(&root).as_deref(), Some(bat0.as_path()));
        let _ = fs::remove_dir_all(&root);
    }
}
