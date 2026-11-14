use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::thread;

use inerium_i18n::tr_fmt;

use super::sysfs::{cache_dir, config_dir, home_dir};

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

pub fn inerium_wallpapers_dir() -> PathBuf {
    home_dir().join(".local/share/inerium/wallpapers")
}

fn ensure_wallpapers_dir_populated(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let has_files = fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                let p = e.path();
                p.is_file()
                    && matches!(
                        p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase().as_str(),
                        "jpg" | "jpeg" | "png" | "webp"
                    )
            })
        })
        .unwrap_or(false);

    if !has_files {
        let home = home_dir();
        let fallback_sources = [
            home.join("Pictures/Wallpapers"),
            home.join("Repos/inerium-shell/assets/wallpapers"),
            home.join("Repos/inerium-desktop/assets/wallpapers"),
            PathBuf::from("/usr/share/backgrounds"),
        ];
        for src in &fallback_sources {
            if let Ok(entries) = fs::read_dir(src) {
                for entry in entries.flatten() {
                    let sp = entry.path();
                    let ext = sp.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
                    if sp.is_file()
                        && matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
                        && let Some(name) = sp.file_name()
                    {
                        let target = dir.join(name);
                        if !target.exists() {
                            let _ = fs::copy(&sp, &target);
                        }
                    }
                }
            }
        }
    }
}

pub fn list_available_wallpapers() -> Vec<WallpaperItem> {
    let mut items = Vec::new();
    let home = home_dir();
    let inerium_dir = inerium_wallpapers_dir();
    ensure_wallpapers_dir_populated(&inerium_dir);

    let dirs = [
        inerium_dir,
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
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
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
// SYSTEM META (Uptime, Hostname)
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
                return tr_fmt!("uptime {}h {}m", "аптайм {}ч {}м", hrs, mins);
            }
    tr_fmt!("uptime 0h 0m", "аптайм 0ч 0м")
}
