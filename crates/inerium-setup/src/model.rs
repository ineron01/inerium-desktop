use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct SetupPaths {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub state_home: PathBuf,
}

impl SetupPaths {
    pub fn from_env() -> Self {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()));
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let state_home = std::env::var("XDG_STATE_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"));
        Self {
            home,
            config_home,
            state_home,
        }
    }

    pub fn hypr_dir(&self) -> PathBuf {
        self.config_home.join("hypr")
    }

    pub fn workspaces_count_path(&self) -> PathBuf {
        self.hypr_dir().join("config/workspaces_count")
    }

    pub fn monitors_local_path(&self) -> PathBuf {
        self.hypr_dir().join("config/monitors_local.lua")
    }

    pub fn inputs_local_path(&self) -> PathBuf {
        self.hypr_dir().join("config/inputs_local.lua")
    }

    pub fn profile_path(&self) -> PathBuf {
        self.hypr_dir().join("profile")
    }

    pub fn hyprpaper_conf_path(&self) -> PathBuf {
        self.hypr_dir().join("hyprpaper.conf")
    }

    pub fn hypridle_local_conf_path(&self) -> PathBuf {
        self.hypr_dir().join("hypridle_local.conf")
    }

    pub fn hypridle_dropin_path(&self) -> PathBuf {
        self.config_home
            .join("systemd/user/hypridle.service.d/local.conf")
    }

    pub fn locale_conf_path(&self) -> PathBuf {
        self.config_home.join("locale.conf")
    }

    pub fn hyprlock_i18n_conf_path(&self) -> PathBuf {
        self.hypr_dir().join("hyprlock-i18n.conf")
    }

    pub fn binds_local_path(&self) -> PathBuf {
        self.hypr_dir().join("config/binds_local.lua")
    }

    pub fn user_dirs_path(&self) -> PathBuf {
        self.config_home.join("user-dirs.dirs")
    }

    pub fn user_dirs_locale_path(&self) -> PathBuf {
        self.config_home.join("user-dirs.locale")
    }

    pub fn inerium_wallpapers_dir(&self) -> PathBuf {
        self.home.join(".local/share/inerium/wallpapers")
    }

    pub fn first_login_steps_dir(&self) -> PathBuf {
        self.config_home.join("inerium/first-login.d")
    }

    pub fn first_login_state_dir(&self) -> PathBuf {
        self.state_home.join("inerium/first-login")
    }

    pub fn setup_done_path(&self) -> PathBuf {
        self.state_home.join("inerium/setup.done")
    }

    pub fn wallpaper_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.inerium_wallpapers_dir(),
            self.home.join("Pictures/Wallpapers"),
            self.home.join("Обои"),
            self.home.join("Wallpapers"),
        ]
    }
}

pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed creating {}: {e}", parent.display()))?;
    }
    let tmp_name = format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("inerium"),
        std::process::id()
    );
    let tmp_path = path.with_file_name(tmp_name);
    let mut file = fs::File::create(&tmp_path)
        .map_err(|e| format!("Failed creating {}: {e}", tmp_path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed writing {}: {e}", tmp_path.display()))?;
    file.sync_all()
        .map_err(|e| format!("Failed syncing {}: {e}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .map_err(|e| format!("Failed renaming to {}: {e}", path.display()))
}

/// Applies system-wide and session-wide language settings atomically.
pub fn apply_system_language(
    paths: &SetupPaths,
    lang: inerium_i18n::Lang,
) -> Result<Option<String>, String> {
    inerium_i18n::Lang::set(lang);

    let (loc_str, date_cmd) = match lang {
        inerium_i18n::Lang::Ru => (
            "ru_RU.UTF-8",
            "LC_TIME=ru_RU.UTF-8 date +\"%A, %-d %B\"",
        ),
        inerium_i18n::Lang::En => (
            "en_US.UTF-8",
            "LC_TIME=en_US.UTF-8 date +\"%A, %B %-d\"",
        ),
    };

    // 1. Write ~/.config/locale.conf atomically
    let locale_content = format!("LANG={loc_str}\nLC_TIME={loc_str}\n");
    atomic_write(&paths.locale_conf_path(), &locale_content)?;

    // 2. Write ~/.config/hypr/hyprlock-i18n.conf atomically
    let (lock_ph, lock_fail, lock_check) = match lang {
        inerium_i18n::Lang::Ru => (
            "<span foreground=\"##737c73\">Пароль</span>",
            "<span foreground=\"##c4746e\">Неверный пароль · попытка $ATTEMPTS</span>",
            "<span foreground=\"##87a987\">Проверка…</span>",
        ),
        inerium_i18n::Lang::En => (
            "<span foreground=\"##737c73\">Password</span>",
            "<span foreground=\"##c4746e\">Wrong password · attempt $ATTEMPTS</span>",
            "<span foreground=\"##87a987\">Verifying…</span>",
        ),
    };

    let hyprlock_i18n_content = format!(
        "# Auto-generated by Inerium Setup\n\
         $LOCK_PLACEHOLDER = {lock_ph}\n\
         $LOCK_FAIL = {lock_fail}\n\
         $LOCK_CHECK = {lock_check}\n\
         $LOCK_DATE_CMD = {date_cmd}\n"
    );
    atomic_write(&paths.hyprlock_i18n_conf_path(), &hyprlock_i18n_content)?;

    // 2b. Enforce English XDG directories to prevent Cyrillic folders (~/Загрузки, ~/Изображения)
    let user_dirs_content = "# Auto-generated by Inerium Setup to enforce standard English directory names\n\
XDG_DESKTOP_DIR=\"$HOME/Desktop\"\n\
XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n\
XDG_TEMPLATES_DIR=\"$HOME/Templates\"\n\
XDG_PUBLICSHARE_DIR=\"$HOME/Public\"\n\
XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n\
XDG_MUSIC_DIR=\"$HOME/Music\"\n\
XDG_PICTURES_DIR=\"$HOME/Pictures\"\n\
XDG_VIDEOS_DIR=\"$HOME/Videos\"\n";
    let _ = atomic_write(&paths.user_dirs_path(), user_dirs_content);
    let _ = atomic_write(&paths.user_dirs_locale_path(), "en_US\n");
    let _ = Command::new("xdg-user-dirs-update").spawn();

    // 3. Live session environment updates (Hyprland, systemd user session, D-Bus activation)
    let _ = Command::new("hyprctl")
        .args(["keyword", "env", &format!("LANG,{loc_str}")])
        .spawn();

    let _ = Command::new("systemctl")
        .args([
            "--user",
            "set-environment",
            &format!("LANG={loc_str}"),
            &format!("LC_TIME={loc_str}"),
        ])
        .spawn();

    let _ = Command::new("dbus-update-activation-environment")
        .args([
            "--systemd",
            &format!("LANG={loc_str}"),
            &format!("LC_TIME={loc_str}"),
        ])
        .spawn();

    // 4. Invalidate running applets via precise SIGUSR1 (NO pkill -f to avoid killing cargo/nvim)
    // Note: Linux kernel TASK_COMM_LEN is 16 bytes (15 chars max + NUL in /proc/[pid]/comm).
    // Binaries longer than 15 chars (e.g. inerium-bluetooth -> inerium-bluetoo) must use truncated names with -x.
    for comm_name in [
        "inerium-clock",
        "inerium-battery",
        "inerium-network",
        "inerium-bluetoo",
        "inerium-hwmon",
        "inerium-dnd",
        "inerium-updates",
    ] {
        let _ = Command::new("pkill")
            .args(["-USR1", "-x", comm_name])
            .spawn();
    }

    // 5. Verify glibc locales
    let mut warning = None;
    if let Ok(output) = Command::new("locale").arg("-a").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let normalized: Vec<String> = stdout
            .lines()
            .map(|l| l.trim().to_ascii_lowercase().replace('-', ""))
            .collect();

        if !normalized.iter().any(|l| l.starts_with("en_us.utf8")) {
            warning = Some("Base locale en_US.UTF-8 is not generated in /etc/locale.gen".to_string());
        } else if lang == inerium_i18n::Lang::Ru
            && !normalized.iter().any(|l| l.starts_with("ru_ru.utf8"))
        {
            warning = Some(
                "Russian locale ru_RU.UTF-8 is missing from locale -a (run: sudo locale-gen)".to_string(),
            );
        }
    }

    Ok(warning)
}

// ============================================================================
// Workspaces Count
// ============================================================================

pub fn parse_workspaces_count(raw: &str) -> u8 {
    raw.trim()
        .lines()
        .next()
        .and_then(|l| l.trim().parse::<u8>().ok())
        .filter(|&v| (1..=9).contains(&v))
        .unwrap_or(3)
}

pub fn read_workspaces_count(path: &Path) -> u8 {
    fs::read_to_string(path)
        .map(|s| parse_workspaces_count(&s))
        .unwrap_or(3)
}

pub fn write_workspaces_count(path: &Path, count: u8) -> Result<(), String> {
    if !(1..=9).contains(&count) {
        return Err(format!("Workspaces count must be 1..=9, got {count}"));
    }
    atomic_write(path, &format!("{count}\n"))
}

// ============================================================================
// Monitors & monitors_local.lua
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyprMonitor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub refresh_rate: f64,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub vrr: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub available_modes: Vec<String>,
}

fn default_scale() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorConfig {
    pub output: String,
    pub mode: String,
    pub position: String,
    pub scale: f64,
    pub vrr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPreset {
    Auto,
    HorizontalTop,
    HorizontalBottom,
    Vertical,
}

pub fn parse_hypr_monitors_json(raw: &str) -> Result<Vec<HyprMonitor>, String> {
    serde_json::from_str::<Vec<HyprMonitor>>(raw)
        .map_err(|e| format!("Failed parsing hyprctl monitors JSON: {e}"))
}

pub fn format_scale(scale: f64) -> String {
    let rounded = (scale * 100.0).round() / 100.0;
    if (rounded - rounded.round()).abs() < 0.005 {
        format!("{:.1}", rounded.round())
    } else {
        let s = format!("{rounded:.2}");
        s.trim_end_matches('0').to_string()
    }
}

pub fn parse_mode_dimensions(mode: &str, fallback_w: u32, fallback_h: u32) -> (u32, u32) {
    let res_part = mode.split('@').next().unwrap_or("");
    if let Some((w_str, h_str)) = res_part.split_once('x')
        && let (Ok(w), Ok(h)) = (w_str.trim().parse::<u32>(), h_str.trim().parse::<u32>())
        && w > 0
        && h > 0
    {
        return (w, h);
    }
    (fallback_w.max(640), fallback_h.max(480))
}

pub fn apply_layout_preset(
    configs: &mut [MonitorConfig],
    live: &[HyprMonitor],
    preset: LayoutPreset,
) {
    if preset == LayoutPreset::Auto {
        for cfg in configs.iter_mut() {
            cfg.position = "auto".to_string();
        }
        return;
    }

    let logical_sizes: Vec<(i32, i32)> = configs
        .iter()
        .map(|cfg| {
            let live_m = live.iter().find(|m| m.name == cfg.output);
            let (fw, fh) = live_m.map(|m| (m.width, m.height)).unwrap_or((1920, 1080));
            let (pw, ph) = parse_mode_dimensions(&cfg.mode, fw, fh);
            let scale = if cfg.scale > 0.1 { cfg.scale } else { 1.0 };
            (
                ((pw as f64) / scale).round() as i32,
                ((ph as f64) / scale).round() as i32,
            )
        })
        .collect();

    let max_h = logical_sizes.iter().map(|&(_, h)| h).max().unwrap_or(1080);
    let mut cursor_x = 0i32;
    let mut cursor_y = 0i32;

    for (cfg, &(lw, lh)) in configs.iter_mut().zip(logical_sizes.iter()) {
        match preset {
            LayoutPreset::Auto => {}
            LayoutPreset::HorizontalTop => {
                cfg.position = format!("{cursor_x}x0");
                cursor_x += lw;
            }
            LayoutPreset::HorizontalBottom => {
                let y = (max_h - lh).max(0);
                cfg.position = format!("{cursor_x}x{y}");
                cursor_x += lw;
            }
            LayoutPreset::Vertical => {
                cfg.position = format!("0x{cursor_y}");
                cursor_y += lh;
            }
        }
    }
}

fn escape_lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn generate_monitors_lua(monitors: &[MonitorConfig]) -> String {
    let mut out = String::from(
        "-- Auto-generated by inerium-setup (local host overrides, gitignored)\n\n",
    );

    for m in monitors {
        if m.output.trim().is_empty() {
            continue;
        }
        let scale_str = format_scale(m.scale);
        let vrr_val = if m.vrr { 1 } else { 0 };
        out.push_str(&format!(
            "hl.monitor({{\n    output   = \"{}\",\n    mode     = \"{}\",\n    position = \"{}\",\n    scale    = {},\n    vrr      = {},\n}})\n\n",
            escape_lua_string(m.output.trim()),
            escape_lua_string(m.mode.trim()),
            escape_lua_string(m.position.trim()),
            scale_str,
            vrr_val
        ));
    }

    out.push_str(
        "-- Fallback for hotplugged monitors\nhl.monitor({\n    output   = \"\",\n    mode     = \"preferred\",\n    position = \"auto\",\n    scale    = 1.0,\n})\n",
    );
    out
}

fn extract_lua_field(block: &str, key: &str) -> Option<String> {
    for line in block.lines() {
        let code = line.split("--").next().unwrap_or("").trim();
        let Some((lhs, rhs)) = code.split_once('=') else {
            continue;
        };
        let lhs_clean = lhs.trim().trim_start_matches("[\"").trim_end_matches("\"]");
        if lhs_clean != key {
            continue;
        }
        let val = rhs.trim().trim_end_matches(',').trim();
        if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
            return Some(val[1..val.len() - 1].replace("\\\"", "\""));
        }
        return Some(val.to_string());
    }
    None
}

pub fn parse_monitors_lua(content: &str) -> Vec<MonitorConfig> {
    let mut result = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("hl.monitor(") {
        let after = &rest[start + "hl.monitor(".len()..];
        let Some(end) = after.find("})") else {
            break;
        };
        let block = &after[..end];
        if let Some(output) = extract_lua_field(block, "output")
            && !output.is_empty()
        {
            let mode = extract_lua_field(block, "mode").unwrap_or_else(|| "preferred".to_string());
            let position =
                extract_lua_field(block, "position").unwrap_or_else(|| "auto".to_string());
            let scale = extract_lua_field(block, "scale")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|&v| (0.5..=4.0).contains(&v))
                .unwrap_or(1.0);
            let vrr = extract_lua_field(block, "vrr")
                .map(|v| v == "1" || v == "true")
                .unwrap_or(false);
            result.push(MonitorConfig {
                output,
                mode,
                position,
                scale,
                vrr,
            });
        }
        rest = &after[end + 2..];
    }
    result
}

pub fn merge_monitors(live: &[HyprMonitor], saved: &[MonitorConfig]) -> Vec<MonitorConfig> {
    live.iter()
        .filter(|m| !m.disabled)
        .map(|m| {
            if let Some(s) = saved.iter().find(|s| s.output == m.name) {
                s.clone()
            } else {
                let current_mode = if m.width > 0 && m.height > 0 && m.refresh_rate > 1.0 {
                    let candidate = format!("{}x{}@{:.2}Hz", m.width, m.height, m.refresh_rate);
                    if m.available_modes.contains(&candidate) {
                        candidate
                    } else {
                        "preferred".to_string()
                    }
                } else {
                    "preferred".to_string()
                };
                MonitorConfig {
                    output: m.name.clone(),
                    mode: current_mode,
                    position: format!("{}x{}", m.x, m.y),
                    scale: m.scale,
                    vrr: m.vrr,
                }
            }
        })
        .collect()
}

// ============================================================================
// Profile & Wallpapers
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyprProfile {
    Waybar,
    Backup,
}

impl HyprProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Waybar => "waybar",
            Self::Backup => "backup",
        }
    }
}

pub fn parse_profile(raw: &str) -> HyprProfile {
    match raw.trim() {
        "backup" => HyprProfile::Backup,
        _ => HyprProfile::Waybar,
    }
}

pub fn read_profile(path: &Path) -> HyprProfile {
    fs::read_to_string(path)
        .map(|s| parse_profile(&s))
        .unwrap_or(HyprProfile::Waybar)
}

pub fn write_profile(path: &Path, profile: HyprProfile) -> Result<(), String> {
    atomic_write(path, &format!("{}\n", profile.as_str()))
}

pub fn parse_active_wallpaper(hyprpaper_conf: &str) -> Option<PathBuf> {
    let mut current_mon: Option<String> = None;
    let mut current_path: Option<String> = None;
    let mut fallback: Option<PathBuf> = None;

    for raw_line in hyprpaper_conf.lines() {
        let line = raw_line.trim();
        if line.starts_with("wallpaper") && line.contains('{') {
            current_mon = None;
            current_path = None;
            continue;
        }
        if line == "}" {
            if let Some(p) = current_path.take()
                && !p.is_empty()
            {
                if current_mon.as_deref().unwrap_or("").is_empty() {
                    fallback = Some(PathBuf::from(p));
                } else {
                    return Some(PathBuf::from(p));
                }
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if key == "monitor" {
                current_mon = Some(val.to_string());
            } else if key == "path" {
                current_path = Some(val.to_string());
            }
        }
    }
    fallback
}

pub fn scan_wallpapers(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") {
                continue;
            }
            let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(canon) {
                items.push(path);
            }
        }
    }
    items.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    items
}

pub fn ensure_default_wallpapers(paths: &SetupPaths) {
    let dest_dir = paths.inerium_wallpapers_dir();
    let _ = fs::create_dir_all(&dest_dir);

    // If destination already has images, nothing to do
    let has_any = fs::read_dir(&dest_dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                let p = e.path();
                p.is_file()
                    && matches!(
                        p.extension().and_then(|x| x.to_str()).unwrap_or(""),
                        "jpg" | "jpeg" | "png" | "webp"
                    )
            })
        })
        .unwrap_or(false);

    if !has_any {
        let sources = [
            paths.home.join("Pictures/Wallpapers"),
            paths.home.join("Repos/inerium-desktop/assets/wallpapers"),
            PathBuf::from("/usr/share/backgrounds"),
        ];
        for src_dir in &sources {
            if let Ok(entries) = fs::read_dir(src_dir) {
                for entry in entries.flatten() {
                    let sp = entry.path();
                    let ext = sp.extension().and_then(|x| x.to_str()).unwrap_or("");
                    if sp.is_file()
                        && matches!(ext, "jpg" | "jpeg" | "png" | "webp")
                        && let Some(file_name) = sp.file_name()
                    {
                        let target = dest_dir.join(file_name);
                        if !target.exists() {
                            let _ = fs::copy(&sp, &target);
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Keyboard, Touchpad, Mouse & Hypridle Overrides
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct InputOverrides {
    pub kb_layout: String,
    pub kb_options: String,
    pub natural_scroll: bool,
    pub tap_to_click: bool,
    pub disable_while_typing: bool,
    pub sensitivity: f64,
    pub accel_profile: String,
    pub scroll_factor: f64,
    pub launcher_bind: Option<String>,
}

impl Default for InputOverrides {
    fn default() -> Self {
        Self {
            kb_layout: "us,ru".to_string(),
            kb_options: "grp:alt_shift_toggle".to_string(),
            natural_scroll: false,
            tap_to_click: true,
            disable_while_typing: true,
            sensitivity: 0.0,
            accel_profile: "flat".to_string(),
            scroll_factor: 1.0,
            launcher_bind: None,
        }
    }
}

pub fn generate_inputs_lua(cfg: &InputOverrides) -> String {
    format!(
        "-- Auto-generated by inerium-setup (local input overrides, gitignored)\n\
hl.config({{\n    input = {{\n        kb_layout     = \"{layout}\",\n        kb_options    = \"{options}\",\n        sensitivity   = {sensitivity:.2},\n        accel_profile = \"{accel}\",\n        scroll_factor = {scroll:.2},\n        touchpad      = {{\n            natural_scroll       = {natural_scroll},\n            tap_to_click         = {tap_to_click},\n            disable_while_typing = {disable_while_typing},\n        }},\n    }},\n}})\n",
        layout = escape_lua_string(cfg.kb_layout.trim()),
        options = escape_lua_string(cfg.kb_options.trim()),
        sensitivity = cfg.sensitivity.clamp(-1.0, 1.0),
        accel = escape_lua_string(cfg.accel_profile.trim()),
        scroll = cfg.scroll_factor.clamp(0.1, 5.0),
        natural_scroll = cfg.natural_scroll,
        tap_to_click = cfg.tap_to_click,
        disable_while_typing = cfg.disable_while_typing,
    )
}

pub fn parse_inputs_lua(content: &str) -> InputOverrides {
    let mut cfg = InputOverrides::default();
    if let Some(v) = extract_lua_field(content, "kb_layout")
        && !v.is_empty()
    {
        cfg.kb_layout = v;
    }
    if let Some(v) = extract_lua_field(content, "kb_options") {
        cfg.kb_options = v;
    }
    if let Some(v) = extract_lua_field(content, "natural_scroll") {
        cfg.natural_scroll = v == "true";
    }
    if let Some(v) = extract_lua_field(content, "tap_to_click")
        .or_else(|| extract_lua_field(content, "tap-to-click"))
    {
        cfg.tap_to_click = v == "true";
    }
    if let Some(v) = extract_lua_field(content, "disable_while_typing") {
        cfg.disable_while_typing = v == "true";
    }
    if let Some(v) = extract_lua_field(content, "sensitivity")
        && let Ok(f) = v.parse::<f64>()
    {
        cfg.sensitivity = f;
    }
    if let Some(v) = extract_lua_field(content, "accel_profile")
        && !v.is_empty()
    {
        cfg.accel_profile = v;
    }
    if let Some(v) = extract_lua_field(content, "scroll_factor")
        && let Ok(f) = v.parse::<f64>()
    {
        cfg.scroll_factor = f;
    }
    cfg
}

pub fn generate_binds_lua(launcher_key: Option<&str>) -> String {
    match launcher_key {
        Some(k) if !k.trim().is_empty() => {
            let key = k.trim();
            let is_release = key.eq_ignore_ascii_case("super")
                || key.eq_ignore_ascii_case("super_l")
                || key.contains("release");
            let bind_call = if is_release {
                "hl.bind(\"SUPER_L\", hl.dsp.exec_cmd(cmd), { release = true })"
            } else {
                &format!("hl.bind(\"{key}\", hl.dsp.exec_cmd(cmd))")
            };
            format!(
                "-- Auto-generated by inerium-setup (local keybind overrides, gitignored)\n\
local home = os.getenv(\"HOME\") or \"\"\n\
local is_backup = os.getenv(\"HYPR_PROFILE\") == \"backup\"\n\
local cmd = is_backup and (\"rofi -config \" .. home .. \"/.config/rofi/config_backup.rasi -show drun\") or \"uwsm app -- rofi -show drun\"\n\n\
{bind_call}\n\n\
return {{ launcher_bound = true }}\n"
            )
        }
        _ => {
            "-- Auto-generated by inerium-setup (local keybind overrides, gitignored)\nreturn {}\n".to_string()
        }
    }
}

pub fn parse_binds_lua(content: &str) -> Option<String> {
    if !content.contains("launcher_bound = true") {
        return None;
    }
    if content.contains("{ release = true }") || content.contains("SUPER_L") {
        return Some("SUPER_L".to_string());
    }
    for line in content.lines() {
        if let Some(start) = line.find("hl.bind(\"") {
            let rem = &line[start + 9..];
            if let Some(end) = rem.find('"') {
                return Some(rem[..end].to_string());
            }
        }
    }
    None
}

pub fn detect_battery_charge_threshold() -> Option<(PathBuf, u8)> {
    let power_dir = Path::new("/sys/class/power_supply");
    if let Ok(entries) = fs::read_dir(power_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("BAT") {
                for node in ["charge_control_end_threshold", "charge_stop_threshold"] {
                    let th_path = path.join(node);
                    if th_path.exists()
                        && let Ok(val_str) = fs::read_to_string(&th_path)
                        && let Ok(val) = val_str.trim().parse::<u8>()
                    {
                        return Some((th_path, val));
                    }
                }
            }
        }
    }
    None
}

pub fn set_battery_charge_threshold(val: u8) -> Result<(), String> {
    if let Some((path, _)) = detect_battery_charge_threshold() {
        fs::write(&path, format!("{val}\n")).map_err(|e| {
            format!("Failed writing {} to {}: {e}", val, path.display())
        })?;
        Ok(())
    } else {
        Err("No battery threshold control found in sysfs".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdleConfig {
    pub enabled: bool,
    pub dim_secs: u32,
    pub lock_secs: u32,
    pub dpms_secs: u32,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dim_secs: 240,
            lock_secs: 420,
            dpms_secs: 600,
        }
    }
}

pub fn generate_hypridle_conf(cfg: &IdleConfig) -> String {
    let mut out = format!(
        "# Auto-generated by inerium-setup (local hypridle overrides, gitignored)\n\
# inerium-setup: enabled={} dim={} lock={} dpms={}\n\n\
general {{\n    lock_cmd = pidof hyprlock || hyprlock\n    before_sleep_cmd = loginctl lock-session\n    after_sleep_cmd = hyprctl dispatch dpms on\n}}\n",
        cfg.enabled, cfg.dim_secs, cfg.lock_secs, cfg.dpms_secs
    );

    if cfg.enabled {
        if cfg.dim_secs > 0 {
            out.push_str(&format!(
                "\nlistener {{\n    timeout = {}\n    on-timeout = brightnessctl -s set 20%\n    on-resume = brightnessctl -r\n}}\n",
                cfg.dim_secs
            ));
        }
        if cfg.lock_secs > 0 {
            out.push_str(&format!(
                "\nlistener {{\n    timeout = {}\n    on-timeout = loginctl lock-session\n}}\n",
                cfg.lock_secs
            ));
        }
        if cfg.dpms_secs > 0 {
            out.push_str(&format!(
                "\nlistener {{\n    timeout = {}\n    on-timeout = hyprctl dispatch dpms off\n    on-resume = hyprctl dispatch dpms on\n}}\n",
                cfg.dpms_secs
            ));
        }
    }
    out
}

pub fn parse_hypridle_conf(content: &str) -> IdleConfig {
    let mut cfg = IdleConfig::default();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(meta) = trimmed.strip_prefix("# inerium-setup:") {
            for token in meta.split_whitespace() {
                if let Some((k, v)) = token.split_once('=') {
                    match k {
                        "enabled" => cfg.enabled = v == "true",
                        "dim" => {
                            if let Ok(n) = v.parse() {
                                cfg.dim_secs = n;
                            }
                        }
                        "lock" => {
                            if let Ok(n) = v.parse() {
                                cfg.lock_secs = n;
                            }
                        }
                        "dpms" => {
                            if let Ok(n) = v.parse() {
                                cfg.dpms_secs = n;
                            }
                        }
                        _ => {}
                    }
                }
            }
            return cfg;
        }
    }

    // Fallback: parse raw hypridle listener blocks (e.g., generated by install.sh)
    let mut in_listener = false;
    let mut cur_timeout: Option<u32> = None;
    let mut cur_cmd = String::new();
    let mut saw_listener = false;
    let mut saw_dim = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("listener") && trimmed.contains('{') {
            in_listener = true;
            cur_timeout = None;
            cur_cmd.clear();
            continue;
        }
        if in_listener {
            if trimmed == "}" {
                if let Some(t) = cur_timeout {
                    saw_listener = true;
                    if cur_cmd.contains("brightnessctl") {
                        cfg.dim_secs = t;
                        saw_dim = true;
                    } else if cur_cmd.contains("lock-session") {
                        cfg.lock_secs = t;
                    } else if cur_cmd.contains("dpms off") {
                        cfg.dpms_secs = t;
                    }
                }
                in_listener = false;
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if k == "timeout" {
                    cur_timeout = v.parse::<u32>().ok();
                } else if k == "on-timeout" {
                    cur_cmd = v.to_string();
                }
            }
        }
    }

    if saw_listener && !saw_dim {
        cfg.dim_secs = 0;
    }

    cfg
}

pub fn generate_hypridle_dropin(conf_path: &Path) -> String {
    format!(
        "[Service]\nExecStart=\nExecStart=/usr/bin/hypridle -c {}\n",
        conf_path.display()
    )
}

// ============================================================================
// First-Login Steps & Setup Wizard State
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    Terminal,
    Gui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Configured,
    Deferred,
    Pending,
}

#[derive(Debug, Clone)]
pub struct FirstLoginStep {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub mode: StepMode,
    pub status: StepStatus,
}

pub fn parse_step_metadata(path: &Path, content: &str) -> (String, StepMode) {
    let fallback_title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("step")
        .to_string();
    let mut title = None;
    let mut mode = StepMode::Terminal;

    for line in content.lines().take(20) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim();
            if let Some(val) = rest.strip_prefix("Title:") {
                let t = val.trim();
                if !t.is_empty() {
                    title = Some(t.to_string());
                }
            } else if let Some(val) = rest.strip_prefix("Mode:")
                && val.trim().eq_ignore_ascii_case("gui")
            {
                mode = StepMode::Gui;
            }
        }
    }

    (title.unwrap_or(fallback_title), mode)
}

pub fn list_executable_steps(steps_dir: &Path) -> Vec<FirstLoginStep> {
    let Ok(entries) = fs::read_dir(steps_dir) else {
        return Vec::new();
    };
    let mut map = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() || (meta.permissions().mode() & 0o111) == 0 {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let content = fs::read_to_string(&path).unwrap_or_default();
        let (title, mode) = parse_step_metadata(&path, &content);
        map.insert(
            name.to_string(),
            FirstLoginStep {
                id: name.to_string(),
                title,
                path,
                mode,
                status: StepStatus::Pending,
            },
        );
    }
    map.into_values().collect()
}

pub fn step_done_marker(state_dir: &Path, step_id: &str) -> PathBuf {
    state_dir.join(format!("{step_id}.done"))
}

pub fn mark_step_done(state_dir: &Path, step_id: &str) -> Result<(), String> {
    fs::create_dir_all(state_dir)
        .map_err(|e| format!("Failed creating {}: {e}", state_dir.display()))?;
    let marker = step_done_marker(state_dir, step_id);
    fs::write(&marker, b"")
        .map_err(|e| format!("Failed writing {}: {e}", marker.display()))
}

pub fn mark_setup_done(paths: &SetupPaths) -> Result<(), String> {
    atomic_write(&paths.setup_done_path(), "done\n")
}

pub fn reset_all_markers(paths: &SetupPaths) -> Result<(), String> {
    let setup_done = paths.setup_done_path();
    if setup_done.exists() {
        fs::remove_file(&setup_done)
            .map_err(|e| format!("Failed removing {}: {e}", setup_done.display()))?;
    }
    let fl_dir = paths.first_login_state_dir();
    if fl_dir.exists() {
        fs::remove_dir_all(&fl_dir)
            .map_err(|e| format!("Failed removing {}: {e}", fl_dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspaces_count_parses_and_clamps() {
        assert_eq!(parse_workspaces_count("5\n"), 5);
        assert_eq!(parse_workspaces_count(" 9 "), 9);
        assert_eq!(parse_workspaces_count("0"), 3);
        assert_eq!(parse_workspaces_count("12"), 3);
        assert_eq!(parse_workspaces_count("abc"), 3);
    }

    #[test]
    fn hypr_monitors_json_parses_and_generates_lua() {
        let json = r#"[
            {
                "name": "eDP-2",
                "description": "Laptop Panel",
                "width": 1920,
                "height": 1080,
                "refreshRate": 144.003,
                "x": 0,
                "y": 216,
                "scale": 1.25,
                "focused": true,
                "vrr": false,
                "disabled": false,
                "availableModes": ["1920x1080@144.00Hz", "1920x1080@60.02Hz"]
            },
            {
                "name": "HDMI-A-1",
                "description": "External",
                "width": 1920,
                "height": 1080,
                "refreshRate": 60.0,
                "x": 1536,
                "y": 0,
                "scale": 1.0,
                "focused": false,
                "vrr": true,
                "disabled": false,
                "availableModes": ["1920x1080@60.00Hz"]
            }
        ]"#;

        let live = parse_hypr_monitors_json(json).expect("valid monitor json");
        assert_eq!(live.len(), 2);

        let mut merged = merge_monitors(&live, &[]);
        assert_eq!(merged[0].output, "eDP-2");
        assert_eq!(merged[0].mode, "1920x1080@144.00Hz");
        assert_eq!(merged[0].position, "0x216");
        assert_eq!(merged[1].output, "HDMI-A-1");
        assert!(merged[1].vrr);

        apply_layout_preset(&mut merged, &live, LayoutPreset::HorizontalBottom);
        assert_eq!(merged[0].position, "0x216");
        assert_eq!(merged[1].position, "1536x0");

        let lua = generate_monitors_lua(&merged);
        assert!(lua.contains("output   = \"eDP-2\""));
        assert!(lua.contains("scale    = 1.25"));
        assert!(lua.contains("vrr      = 1"));
        assert!(lua.contains("output   = \"\""));

        let reparsed = parse_monitors_lua(&lua);
        assert_eq!(reparsed, merged);
    }

    #[test]
    fn inputs_and_hypridle_roundtrip() {
        let inputs = InputOverrides {
            kb_layout: "us,ru,ua".to_string(),
            kb_options: "grp:alt_shift_toggle".to_string(),
            natural_scroll: true,
            tap_to_click: false,
            disable_while_typing: true,
            sensitivity: 0.25,
            accel_profile: "flat".to_string(),
            scroll_factor: 1.2,
            launcher_bind: Some("SUPER_L".to_string()),
        };
        let lua = generate_inputs_lua(&inputs);
        let parsed = parse_inputs_lua(&lua);
        assert_eq!(parsed.kb_layout, inputs.kb_layout);
        assert_eq!(parsed.kb_options, inputs.kb_options);
        assert_eq!(parsed.natural_scroll, inputs.natural_scroll);
        assert_eq!(parsed.tap_to_click, inputs.tap_to_click);
        assert_eq!(parsed.disable_while_typing, inputs.disable_while_typing);
        assert!((parsed.sensitivity - inputs.sensitivity).abs() < 0.01);
        assert_eq!(parsed.accel_profile, inputs.accel_profile);
        assert!((parsed.scroll_factor - inputs.scroll_factor).abs() < 0.01);

        let binds_lua = generate_binds_lua(Some("SUPER_L"));
        assert!(binds_lua.contains("SUPER_L"));
        assert_eq!(parse_binds_lua(&binds_lua), Some("SUPER_L".to_string()));

        let idle = IdleConfig {
            enabled: true,
            dim_secs: 180,
            lock_secs: 300,
            dpms_secs: 480,
        };
        let conf = generate_hypridle_conf(&idle);
        assert!(conf.contains("timeout = 180"));
        assert!(conf.contains("timeout = 300"));
        assert!(conf.contains("timeout = 480"));
        assert_eq!(parse_hypridle_conf(&conf), idle);

        let disabled_idle = IdleConfig {
            enabled: false,
            ..idle
        };
        let disabled_conf = generate_hypridle_conf(&disabled_idle);
        assert!(!disabled_conf.contains("listener {"));
        assert_eq!(parse_hypridle_conf(&disabled_conf), disabled_idle);

        let raw_laptop = r#"
# Auto-generated by inerium-desktop (laptop chassis, gitignored)
listener {
    timeout = 150
    on-timeout = brightnessctl -s set 20%
}
listener {
    timeout = 300
    on-timeout = loginctl lock-session
}
listener {
    timeout = 330
    on-timeout = hyprctl dispatch dpms off
}
"#;
        assert_eq!(
            parse_hypridle_conf(raw_laptop),
            IdleConfig {
                enabled: true,
                dim_secs: 150,
                lock_secs: 300,
                dpms_secs: 330,
            }
        );

        let raw_desktop = r#"
# Auto-generated by inerium-desktop (desktop chassis, gitignored)
listener {
    timeout = 600
    on-timeout = loginctl lock-session
}
listener {
    timeout = 900
    on-timeout = hyprctl dispatch dpms off
}
"#;
        assert_eq!(
            parse_hypridle_conf(raw_desktop),
            IdleConfig {
                enabled: true,
                dim_secs: 0,
                lock_secs: 600,
                dpms_secs: 900,
            }
        );
    }

    #[test]
    fn wallpaper_parsing_and_step_metadata() {
        let conf = r#"
wallpaper {
    monitor = eDP-2
    path = /home/user/Pictures/Wallpapers/fuji.jpg
    fit_mode = cover
}
wallpaper {
    monitor = 
    path = /home/user/Pictures/Wallpapers/default.jpg
    fit_mode = cover
}
"#;
        assert_eq!(
            parse_active_wallpaper(conf),
            Some(PathBuf::from("/home/user/Pictures/Wallpapers/fuji.jpg"))
        );

        let script = "#!/usr/bin/env bash\n# Title: GitHub — вход\n# Mode: gui\necho ok\n";
        let (title, mode) = parse_step_metadata(Path::new("10-github"), script);
        assert_eq!(title, "GitHub — вход");
        assert_eq!(mode, StepMode::Gui);
    }

    #[test]
    fn markers_and_atomic_write_in_tempdir() {
        let base = std::env::temp_dir().join(format!("inerium_setup_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = SetupPaths {
            home: base.clone(),
            config_home: base.join(".config"),
            state_home: base.join(".local/state"),
        };

        write_workspaces_count(&paths.workspaces_count_path(), 6).unwrap();
        assert_eq!(read_workspaces_count(&paths.workspaces_count_path()), 6);

        write_profile(&paths.profile_path(), HyprProfile::Backup).unwrap();
        assert_eq!(read_profile(&paths.profile_path()), HyprProfile::Backup);

        mark_setup_done(&paths).unwrap();
        mark_step_done(&paths.first_login_state_dir(), "10-github").unwrap();
        assert!(paths.setup_done_path().exists());
        assert!(step_done_marker(&paths.first_login_state_dir(), "10-github").exists());

        reset_all_markers(&paths).unwrap();
        assert!(!paths.setup_done_path().exists());
        assert!(!paths.first_login_state_dir().exists());

        let _ = fs::remove_dir_all(&base);
    }
}
