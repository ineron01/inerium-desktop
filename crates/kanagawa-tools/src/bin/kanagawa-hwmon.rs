//! Kanagawa Dragon — Native Rust Hardware Monitor Provider for Waybar
//! - CPU temperature from sysfs hwmon
//! - CPU usage from /proc/stat
//! - Memory usage from /proc/meminfo
//! - Click toggle for detailed (CPU+RAM) vs compact mode via SIGUSR1
//! - Tooltip with detailed breakdown and warning/critical thresholds

use std::env;
use std::fs;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use kanagawa_tools::format::escape_json;
use kanagawa_tools::ipc::{PidRegistry, SelfPipe};
use kanagawa_tools::sys::posix::*;

fn hwmon_registry() -> PidRegistry {
    PidRegistry::new("hwmon-detailed")
}

fn toggle_detailed() {
    let reg = hwmon_registry();
    reg.toggle_state();
    reg.notify_all(SIGUSR1);
}

fn is_detailed() -> bool {
    hwmon_registry().is_state_active()
}

fn cpu_driver_priority(name: &str) -> Option<usize> {
    let n = name.trim();
    if n == "k10temp" {
        Some(0)
    } else if n == "zenpower" {
        Some(1)
    } else if n == "coretemp" {
        Some(2)
    } else if n == "cpu_thermal" || n == "soc_dts" || n.starts_with("soc_dts") || n == "acpitz" {
        Some(3)
    } else {
        None
    }
}

fn read_hwmon_dir_temp(dir: &std::path::Path) -> Option<i32> {
    // Prefer labeled CPU package/die sensors (e.g., "Package id 0", "Tctl", "Tdie")
    for idx in 1..=16 {
        let label_path = dir.join(format!("temp{idx}_label"));
        if let Ok(label) = fs::read_to_string(&label_path) {
            let l = label.trim();
            if l.eq_ignore_ascii_case("Package id 0")
                || l.eq_ignore_ascii_case("Tctl")
                || l.eq_ignore_ascii_case("Tdie")
            {
                let input_path = dir.join(format!("temp{idx}_input"));
                if let Ok(content) = fs::read_to_string(&input_path)
                    && let Ok(val) = content.trim().parse::<i32>()
                {
                    return Some(val / 1000);
                }
            }
        }
    }

    // Fallback to temp1_input (or first available tempN_input) within this sensor
    for idx in 1..=16 {
        let input_path = dir.join(format!("temp{idx}_input"));
        if let Ok(content) = fs::read_to_string(&input_path)
            && let Ok(val) = content.trim().parse::<i32>()
        {
            return Some(val / 1000);
        }
    }
    None
}

fn read_temp_from_base(base: &std::path::Path) -> Option<i32> {
    let entries = fs::read_dir(base).ok()?;
    let mut best: Option<(usize, i32)> = None;
    let mut fallback: Option<i32> = None;

    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let name = fs::read_to_string(p.join("name")).unwrap_or_default();
        if let Some(prio) = cpu_driver_priority(&name) {
            if let Some(temp) = read_hwmon_dir_temp(&p)
                && best.is_none_or(|(best_prio, _)| prio < best_prio)
            {
                best = Some((prio, temp));
            }
        } else if fallback.is_none() {
            fallback = read_hwmon_dir_temp(&p);
        }
    }

    best.map(|(_, t)| t).or(fallback)
}

fn read_temp() -> Option<i32> {
    read_temp_from_base(std::path::Path::new("/sys/class/hwmon"))
}

fn parse_proc_stat() -> (u64, u64) {
    let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
    let first_line = stat.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace().skip(1);
    let mut vals = [0u64; 10];
    for v in &mut vals {
        if let Some(val) = parts.next().and_then(|s| s.parse().ok()) {
            *v = val;
        }
    }
    let idle = vals[3] + vals[4];
    let total: u64 = vals.iter().sum();
    (total, idle)
}

fn read_cpu_with_file_cache(uid: u32) -> u32 {
    let (total, idle) = parse_proc_stat();
    let cache_path = format!("/run/user/{}/kanagawa-hwmon-cpu", uid);

    let mut usage = 0;
    if let Ok(prev_str) = fs::read_to_string(&cache_path) {
        let parts: Vec<&str> = prev_str.split_whitespace().collect();
        if parts.len() == 2
            && let (Ok(p_total), Ok(p_idle)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                let d_total = total.saturating_sub(p_total);
                let d_idle = idle.saturating_sub(p_idle);
                if let Some(u) = ((d_total.saturating_sub(d_idle)) * 100).checked_div(d_total) {
                    usage = u as u32;
                }
            }
    } else {
        // First run without cache: quick 40ms delta
        thread::sleep(Duration::from_millis(40));
        let (t2, i2) = parse_proc_stat();
        let d_total = t2.saturating_sub(total);
        let d_idle = i2.saturating_sub(idle);
        if let Some(u) = ((d_total.saturating_sub(d_idle)) * 100).checked_div(d_total) {
            usage = u as u32;
        }
    }

    let _ = fs::write(&cache_path, format!("{} {}", total, idle));
    usage.min(100)
}

fn read_cpu(prev: &mut Option<(u64, u64)>) -> u32 {
    let (total, idle) = parse_proc_stat();
    let mut usage = 0;
    if let Some((p_total, p_idle)) = *prev {
        let d_total = total.saturating_sub(p_total);
        let d_idle = idle.saturating_sub(p_idle);
        if let Some(u) = ((d_total.saturating_sub(d_idle)) * 100).checked_div(d_total) {
            usage = u as u32;
        }
    }
    *prev = Some((total, idle));
    usage.min(100)
}

fn read_mem() -> (u32, f64, f64) {
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            avail_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        }
        if total_kb > 0 && avail_kb > 0 {
            break;
        }
    }
    if total_kb == 0 {
        return (0, 0.0, 0.0);
    }
    let used_kb = total_kb.saturating_sub(avail_kb);
    let pct = (used_kb * 100 / total_kb) as u32;
    let used_gib = used_kb as f64 / 1048576.0;
    let total_gib = total_kb as f64 / 1048576.0;
    (pct, used_gib, total_gib)
}

fn temp_icon(t: Option<i32>) -> &'static str {
    match t {
        Some(v) if v < 45 => "",
        Some(v) if v < 60 => "",
        Some(v) if v < 75 => "",
        Some(_) => "",
        None => "",
    }
}

fn format_output(cpu: u32, plain: bool, detailed: bool) -> String {
    let t = read_temp();
    let (mem_pct, used_gib, total_gib) = read_mem();

    let temp_txt = match t {
        Some(v) => format!("{} {}°", temp_icon(t), v),
        None => " --".to_string(),
    };

    let text = if detailed {
        format!(
            "{}   <span color='#737c73'>CPU</span> {}%   <span color='#737c73'>RAM</span> {}%",
            temp_txt, cpu, mem_pct
        )
    } else {
        temp_txt
    };

    if plain {
        text
    } else {
        let mut class_list = Vec::new();
        if detailed {
            class_list.push("\"detailed\"");
        }
        let temp_val = t.unwrap_or(0);
        if temp_val >= 85 || cpu >= 90 || mem_pct >= 90 {
            class_list.push("\"critical\"");
        } else if temp_val >= 70 || cpu >= 70 || mem_pct >= 75 {
            class_list.push("\"warning\"");
        }

        let tooltip = format!(
            "Температура CPU: {}°C\nЗагрузка CPU: {}%\nОЗУ: {:.1} / {:.1} ГиБ ({}%)\n\nЛКМ: {} · ПКМ: btop",
            t.map(|v| v.to_string()).unwrap_or_else(|| "--".to_string()),
            cpu,
            used_gib,
            total_gib,
            mem_pct,
            if detailed { "Свернуть" } else { "Подробнее (CPU, ОЗУ)" }
        );

        format!(
            "{{\"text\":\"{}\",\"tooltip\":\"{}\",\"class\":[{}]}}",
            escape_json(&text),
            escape_json(&tooltip),
            class_list.join(",")
        )
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let plain = args.iter().any(|a| a == "--plain");
    let oneshot = args.iter().any(|a| a == "--oneshot");
    let toggle = args.iter().any(|a| a == "--toggle");

    let uid = posix_getuid();

    if toggle {
        toggle_detailed();
        return;
    }

    if oneshot {
        let detailed = is_detailed();
        let cpu = read_cpu_with_file_cache(uid);
        let out = format_output(cpu, plain, detailed);
        println!("{}", out);
        return;
    }

    // Persistent streaming mode (Waybar or watch)
    let reg = hwmon_registry();
    let _pid_guard = reg.register();
    let self_pipe = match SelfPipe::new() {
        Ok(sp) => sp,
        Err(e) => {
            eprintln!("Failed to create self pipe: {e}");
            return;
        }
    };

    let mut prev_cpu = None;
    read_cpu(&mut prev_cpu); // initial reference
    thread::sleep(Duration::from_millis(100));

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut last_output = String::new();

    loop {
        let detailed = is_detailed();
        let cpu = read_cpu(&mut prev_cpu);
        let out = format_output(cpu, plain, detailed);
        if out != last_output {
            let _ = writeln!(handle, "{}", out);
            let _ = handle.flush();
            last_output = out;
        }

        self_pipe.wait_event(2500);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_hwmon_prioritizes_cpu_over_nvme_and_reads_package_label() {
        let base = std::env::temp_dir().join(format!("kanagawa_hwmon_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        // hwmon0: nvme (should be ignored when coretemp/k10temp exists)
        let hw0 = base.join("hwmon0");
        fs::create_dir_all(&hw0).unwrap();
        fs::write(hw0.join("name"), "nvme\n").unwrap();
        fs::write(hw0.join("temp1_input"), "39000\n").unwrap();

        // hwmon1: acpitz (lower priority than coretemp)
        let hw1 = base.join("hwmon1");
        fs::create_dir_all(&hw1).unwrap();
        fs::write(hw1.join("name"), "acpitz\n").unwrap();
        fs::write(hw1.join("temp1_input"), "41000\n").unwrap();

        // hwmon2: coretemp with Package id 0 on temp2
        let hw2 = base.join("hwmon2");
        fs::create_dir_all(&hw2).unwrap();
        fs::write(hw2.join("name"), "coretemp\n").unwrap();
        fs::write(hw2.join("temp1_label"), "Core 0\n").unwrap();
        fs::write(hw2.join("temp1_input"), "48000\n").unwrap();
        fs::write(hw2.join("temp2_label"), "Package id 0\n").unwrap();
        fs::write(hw2.join("temp2_input"), "56000\n").unwrap();

        assert_eq!(read_temp_from_base(&base), Some(56));

        // Add k10temp (highest priority)
        let hw3 = base.join("hwmon3");
        fs::create_dir_all(&hw3).unwrap();
        fs::write(hw3.join("name"), "k10temp\n").unwrap();
        fs::write(hw3.join("temp1_label"), "Tctl\n").unwrap();
        fs::write(hw3.join("temp1_input"), "62000\n").unwrap();

        assert_eq!(read_temp_from_base(&base), Some(62));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn cpu_hwmon_empty_dir_returns_none() {
        let base = std::env::temp_dir().join(format!("kanagawa_hwmon_empty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        assert_eq!(read_temp_from_base(&base), None);
        let _ = fs::remove_dir_all(&base);
    }
}
