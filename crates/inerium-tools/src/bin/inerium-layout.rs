//! Inerium Dragon — Native Rust Keyboard Layout Watcher for Waybar

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;

use inerium_tools::format::escape_json;
use inerium_tools::ipc::hyprland::get_socket2;

fn get_current_layout() -> &'static str {
    if let Ok(out) = Command::new("hyprctl").args(["devices", "-j"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for target in &["telink-vxe-v87-2.4g-dongle-keyboard", "at-translated-set-2-keyboard"] {
            if let Some(pos) = text.find(target) {
                let end = (pos + 600).min(text.len());
                let slice = &text[pos..end];
                if let Some(kpos) = slice.find("\"active_keymap\": \"") {
                    let rest = &slice[kpos + 18..];
                    if let Some(q_pos) = rest.find('"') {
                        let km = &rest[..q_pos];
                        if km.contains("Russian") || km.contains("RU") {
                            return "RU";
                        } else {
                            return "EN";
                        }
                    }
                }
            }
        }
    }
    "RU"
}

fn switch_keyboards() {
    let cur = get_current_layout();
    let target_idx = if cur == "RU" { "0" } else { "1" };
    for target in &[
        "telink-vxe-v87-2.4g-dongle-keyboard",
        "telink-vxe-v87-2.4g-dongle",
        "at-translated-set-2-keyboard",
        "ydotoold-virtual-device"
    ] {
        let _ = Command::new("hyprctl")
            .args(["switchxkblayout", target, target_idx])
            .output();
    }
}

fn format_output(layout: &str, waybar: bool) -> String {
    if waybar {
        let long_name = if layout == "RU" { "Русская (RU)" } else { "English (EN)" };
        let tooltip = format!("Раскладка: {}\nЛКМ: Переключить (RU/EN)", long_name);
        format!(
            "{{\"text\":\"{}\",\"tooltip\":\"{}\",\"class\":\"{}\"}}",
            escape_json(layout),
            escape_json(&tooltip),
            layout.to_lowercase()
        )
    } else {
        layout.to_string()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--switch") {
        switch_keyboards();
        println!("{}", get_current_layout());
        return;
    }

    let waybar = args.iter().any(|a| a == "--waybar" || a == "--json");
    let stream_mode = waybar || args.iter().any(|a| a == "--watch" || a == "--stream");

    let mut current = get_current_layout();
    println!("{}", format_output(current, waybar));
    std::io::stdout().flush().ok();

    if !stream_mode {
        return;
    }

    if let Some(s2) = get_socket2()
        && let Ok(stream) = UnixStream::connect(s2) {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(ev) = line.strip_prefix("activelayout>>") {
                    let new_layout = if ev.contains("Russian") || ev.contains("RU") {
                        "RU"
                    } else {
                        "EN"
                    };
                    if new_layout != current {
                        current = new_layout;
                        println!("{}", format_output(current, waybar));
                        std::io::stdout().flush().ok();
                    }
                }
            }
        }
}
