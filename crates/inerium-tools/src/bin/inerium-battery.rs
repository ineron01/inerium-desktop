//! Inerium Dragon — Native Rust Battery Provider for Waybar
//! - Normal format: {icon} {capacity}%
//! - Extended format: {icon} {capacity}% · {time} (toggled by LKM click)
//! - Dynamic power_supply discovery (BAT* / Mains) with graceful desktop fallback
//! - Tooltip: Rich Pango markup with capacity, remaining time, power in W, health %
//! - Watch mode: Streams battery updates, instant reaction to SIGUSR1 on toggle

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use inerium_tools::format::escape_json;
use inerium_tools::ipc::{PidRegistry, SelfPipe};
use inerium_tools::sys::posix::SIGUSR1;

const BATTERY_ICONS: [&str; 10] = ["󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"];
const ICON_CHARGING: &str = "󰂄";
const ICON_PLUGGED: &str = "󰚥";

fn battery_registry() -> PidRegistry {
    PidRegistry::new("battery-alt")
}

fn is_alt_mode() -> bool {
    battery_registry().is_state_active()
}

fn toggle_alt_mode() {
    let reg = battery_registry();
    reg.toggle_state();
    reg.notify_all(SIGUSR1);
}

fn read_sysfs_str(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

fn read_sysfs_i64(path: &Path) -> i64 {
    read_sysfs_str(path).parse().unwrap_or(0)
}

fn read_battery_threshold(bat_dir: &Path) -> i64 {
    for name in &["charge_control_end_threshold", "charge_stop_threshold"] {
        let path = bat_dir.join(name);
        if path.exists() {
            let val = read_sysfs_i64(&path);
            if (1..=100).contains(&val) {
                return val;
            }
        }
    }
    100
}

fn discover_power_supplies() -> (Option<PathBuf>, bool) {
    discover_power_supplies_in(Path::new("/sys/class/power_supply"))
}

fn discover_power_supplies_in(base: &Path) -> (Option<PathBuf>, bool) {
    let mut bat_path: Option<PathBuf> = None;
    let mut fallback_bat: Option<PathBuf> = None;
    let mut ac_online = false;

    if let Ok(entries) = fs::read_dir(base) {
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for path in dirs {
            let ptype = read_sysfs_str(&path.join("type"));
            if ptype == "Mains" {
                if read_sysfs_i64(&path.join("online")) == 1 {
                    ac_online = true;
                }
            } else if ptype == "Battery" {
                let scope = read_sysfs_str(&path.join("scope"));
                if scope == "Device" {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with("BAT") && bat_path.is_none() {
                    bat_path = Some(path);
                } else if fallback_bat.is_none() {
                    fallback_bat = Some(path);
                }
            }
        }
    }

    (bat_path.or(fallback_bat), ac_online)
}

struct BatteryInfo {
    present: bool,
    capacity: i64,
    status: String,
    ac_online: bool,
    power_w: f64,
    health_pct: f64,
    threshold: i64,
    time_str: String,
    icon: &'static str,
}

fn get_battery_info() -> BatteryInfo {
    let (bat_opt, ac_online) = discover_power_supplies();
    let Some(bat_dir) = bat_opt else {
        return BatteryInfo {
            present: false,
            capacity: 0,
            status: String::new(),
            ac_online: true,
            power_w: 0.0,
            health_pct: 100.0,
            threshold: 100,
            time_str: String::new(),
            icon: ICON_PLUGGED,
        };
    };

    let cap = read_sysfs_i64(&bat_dir.join("capacity")).clamp(0, 100);
    let st = read_sysfs_str(&bat_dir.join("status"));

    let (now, full, design, power_w, rate) = if bat_dir.join("charge_now").exists() {
        let now = read_sysfs_i64(&bat_dir.join("charge_now"));
        let full = read_sysfs_i64(&bat_dir.join("charge_full"));
        let design = read_sysfs_i64(&bat_dir.join("charge_full_design")).max(1);
        let curr = read_sysfs_i64(&bat_dir.join("current_now"));
        let volt = read_sysfs_i64(&bat_dir.join("voltage_now"));
        let pw = (volt as f64 * curr as f64) / 1e12;
        (now, full, design, pw, curr)
    } else {
        let now = read_sysfs_i64(&bat_dir.join("energy_now"));
        let full = read_sysfs_i64(&bat_dir.join("energy_full"));
        let design = read_sysfs_i64(&bat_dir.join("energy_full_design")).max(1);
        let pnow = read_sysfs_i64(&bat_dir.join("power_now"));
        let pw = pnow as f64 / 1e6;
        (now, full, design, pw, pnow)
    };

    let health_pct = if full > 0 {
        (full as f64 / design as f64) * 100.0
    } else {
        100.0
    };

    let threshold = read_battery_threshold(&bat_dir);

    let time_str = if rate > 0 {
        if st == "Discharging" || (!ac_online && now > 0) {
            let hrs = now as f64 / rate as f64;
            let h = hrs as i64;
            let m = ((hrs - h as f64) * 60.0) as i64;
            format!("{}ч {:02}м", h, m)
        } else if st == "Charging" {
            let target = if threshold < 100 {
                ((full as f64) * (threshold as f64 / 100.0)).round() as i64
            } else {
                full
            };
            if target > now {
                let rem = target - now;
                let hrs = rem as f64 / rate as f64;
                let h = hrs as i64;
                let m = ((hrs - h as f64) * 60.0) as i64;
                format!("{}ч {:02}м до {}%", h, m, threshold)
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let icon = if ac_online {
        if st == "Charging" {
            ICON_CHARGING
        } else {
            ICON_PLUGGED
        }
    } else {
        let idx = ((cap / 10).clamp(0, 9)) as usize;
        BATTERY_ICONS[idx]
    };

    BatteryInfo {
        present: true,
        capacity: cap,
        status: st,
        ac_online,
        power_w,
        health_pct,
        threshold,
        time_str,
        icon,
    }
}

fn format_battery(info: &BatteryInfo) -> String {
    if !info.present {
        return format!("{} Сеть", info.icon);
    }
    if is_alt_mode() && !info.time_str.is_empty() {
        format!("{} {}% · {}", info.icon, info.capacity, info.time_str)
    } else {
        format!("{} {}%", info.icon, info.capacity)
    }
}

fn format_tooltip(info: &BatteryInfo) -> String {
    if !info.present {
        return "Батарея не обнаружена (питание от сети)".to_string();
    }
    let status_ru = match info.status.as_str() {
        "Charging" => "Заряжается",
        "Discharging" => "Разряжается",
        "Full" => "Полный заряд",
        "Not charging" => "Не заряжается (лимит)",
        _ => &info.status,
    };

    let ac_str = if info.ac_online { "Подключен" } else { "Отключен" };

    let mut lines = Vec::new();
    lines.push("<span font_desc='JetBrainsMono Nerd Font 10.5'>".to_string());
    lines.push(format!(
        "<span color='#87a987' weight='bold'>󰁹 Батарея: {}%</span>",
        info.capacity
    ));
    lines.push(format!("<span color='#727169'>{:<13}</span><span color='#e6e2cc'>{}</span>", "Статус:", status_ru));
    lines.push(format!("<span color='#727169'>{:<13}</span><span color='#8ba4b0'>{}</span>", "Блок питания:", ac_str));

    if info.threshold < 100 {
        lines.push(format!("<span color='#727169'>{:<13}</span><span color='#c4b28a'>{}%</span>", "Лимит заряда:", info.threshold));
    }

    if !info.time_str.is_empty() {
        let label = if info.status == "Charging" {
            if info.threshold < 100 {
                "До лимита:"
            } else {
                "До полной:"
            }
        } else {
            "Осталось:"
        };
        lines.push(format!("<span color='#727169'>{:<13}</span><span color='#c4b28a'>{}</span>", label, info.time_str));
    }

    if info.power_w > 0.05 {
        lines.push(format!("<span color='#727169'>{:<13}</span><span color='#e6e2cc'>{:.1} Вт</span>", "Мощность:", info.power_w));
    }

    lines.push(format!("<span color='#727169'>{:<13}</span><span color='#87a987'>{:.1}%</span>", "Здоровье АКБ:", info.health_pct));
    lines.push(String::new());
    lines.push("<span color='#727169' size='small'>ЛКМ: Переключить время / процент</span>".to_string());
    lines.push("</span>".to_string());

    lines.join("\n")
}

fn format_json(info: &BatteryInfo) -> String {
    if !info.present {
        return "{\"text\":\"\",\"tooltip\":\"\",\"class\":[\"none\"]}".to_string();
    }
    let text = format_battery(info);
    let tooltip = format_tooltip(info);
    let mut class_list = vec!["\"battery\""];
    if info.ac_online {
        if info.status == "Charging" {
            class_list.push("\"charging\"");
        } else {
            class_list.push("\"plugged\"");
        }
    } else if info.capacity <= 15 {
        class_list.push("\"critical\"");
    } else if info.capacity <= 30 {
        class_list.push("\"warning\"");
    }
    format!(
        "{{\"text\":\"{}\",\"tooltip\":\"{}\",\"class\":[{}]}}",
        escape_json(&text),
        escape_json(&tooltip),
        class_list.join(",")
    )
}

fn run_watch(waybar: bool) {
    let reg = battery_registry();
    let _pid_guard = reg.register();
    let self_pipe = match SelfPipe::new() {
        Ok(sp) => sp,
        Err(e) => {
            eprintln!("Failed to create self pipe: {e}");
            return;
        }
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut last_output = String::new();

    loop {
        let info = get_battery_info();
        let current = if waybar { format_json(&info) } else { format_battery(&info) };
        if current != last_output {
            let _ = writeln!(handle, "{}", current);
            let _ = handle.flush();
            last_output = current;
        }

        self_pipe.wait_event(2000);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--toggle" => {
                toggle_alt_mode();
                let info = get_battery_info();
                println!("{}", format_battery(&info));
                return;
            }
            "--tooltip" => {
                let info = get_battery_info();
                println!("{}", format_tooltip(&info));
                return;
            }
            "--waybar" => {
                run_watch(true);
                return;
            }
            "--watch" => {
                let waybar = args.iter().any(|a| a == "--json" || a == "--waybar");
                run_watch(waybar);
                return;
            }
            _ => {}
        }
    }
    let info = get_battery_info();
    println!("{}", format_battery(&info));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_without_battery_hides_waybar_module() {
        let (bat, ac) = discover_power_supplies_in(Path::new("/nonexistent/power_supply"));
        assert!(bat.is_none());
        assert!(!ac);
        let info = BatteryInfo {
            present: false,
            capacity: 0,
            status: String::new(),
            ac_online: true,
            power_w: 0.0,
            health_pct: 100.0,
            threshold: 100,
            time_str: String::new(),
            icon: ICON_PLUGGED,
        };
        assert_eq!(
            format_json(&info),
            "{\"text\":\"\",\"tooltip\":\"\",\"class\":[\"none\"]}"
        );
    }

    #[test]
    fn test_charging_time_calculation_with_threshold_80() {
        let full = 4_000_000i64;
        let now = 2_000_000i64;
        let rate = 2_000_000i64;
        let threshold = 80i64;
        let target = ((full as f64) * (threshold as f64 / 100.0)).round() as i64;
        assert_eq!(target, 3_200_000);
        let rem = target - now;
        assert_eq!(rem, 1_200_000);
        let hrs = rem as f64 / rate as f64;
        let h = hrs as i64;
        let m = ((hrs - h as f64) * 60.0) as i64;
        let time_str = format!("{}ч {:02}м до {}%", h, m, threshold);
        assert_eq!(time_str, "0ч 36м до 80%");
    }

    #[test]
    fn test_charging_time_calculation_full_100() {
        let full = 4_000_000i64;
        let now = 2_000_000i64;
        let rate = 2_000_000i64;
        let threshold = 100i64;
        let target = full;
        let rem = target - now;
        let hrs = rem as f64 / rate as f64;
        let h = hrs as i64;
        let m = ((hrs - h as f64) * 60.0) as i64;
        let time_str = format!("{}ч {:02}м до {}%", h, m, threshold);
        assert_eq!(time_str, "1ч 00м до 100%");
    }

    #[test]
    fn test_tooltip_includes_threshold_and_label() {
        let info = BatteryInfo {
            present: true,
            capacity: 55,
            status: "Charging".to_string(),
            ac_online: true,
            power_w: 25.0,
            health_pct: 95.0,
            threshold: 80,
            time_str: "0ч 30м до 80%".to_string(),
            icon: ICON_CHARGING,
        };
        let tip = format_tooltip(&info);
        assert!(tip.contains("Лимит заряда:"));
        assert!(tip.contains("80%"));
        assert!(tip.contains("До лимита:"));
        assert!(tip.contains("0ч 30м до 80%"));
    }
}
