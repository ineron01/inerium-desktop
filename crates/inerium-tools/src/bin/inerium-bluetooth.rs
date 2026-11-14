//! Inerium Dragon — Native Rust Bluetooth Provider for Waybar
//! - Fast sysfs & rfkill checking
//! - Watch mode: Streams bluetooth status, reacts to SIGUSR1
//! - Tooltip: Rich Pango markup with adapter status and connected devices

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};

use inerium_i18n::tr;
use inerium_tools::format::escape_json;
use inerium_tools::ipc::SelfPipe;

enum BtState {
    PoweredOff,
    PoweredOnNoDev,
    Connected(String),
}

fn has_bluetooth() -> bool {
    fs::read_dir("/sys/class/bluetooth")
        .map(|mut rd| rd.next().is_some())
        .unwrap_or(false)
}

fn get_bt_state() -> BtState {
    if !has_bluetooth() {
        return BtState::PoweredOff;
    }

    // Check rfkill
    let mut rfkill_on = true;
    if let Ok(entries) = fs::read_dir("/sys/class/rfkill") {
        for entry in entries.flatten() {
            let tpath = entry.path().join("type");
            if let Ok(t) = fs::read_to_string(&tpath)
                && t.trim() == "bluetooth" {
                    let spath = entry.path().join("state");
                    if let Ok(s) = fs::read_to_string(&spath)
                        && s.trim() == "0" {
                            rfkill_on = false;
                        }
                }
        }
    }

    if !rfkill_on {
        return BtState::PoweredOff;
    }

    // Check if controller is powered
    let is_powered = Command::new("timeout")
        .args(["2", "bluetoothctl", "show"])
        .stdin(Stdio::null())
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).contains("Powered: yes"))
        .unwrap_or(false);

    if !is_powered {
        return BtState::PoweredOff;
    }

    // Check connected devices
    if let Ok(output) = Command::new("timeout")
        .args(["2", "bluetoothctl", "devices", "Connected"])
        .stdin(Stdio::null())
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let first_line = text.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() {
            let parts: Vec<&str> = first_line.splitn(3, ' ').collect();
            if parts.len() == 3 {
                return BtState::Connected(parts[2].to_string());
            }
        }
    }

    BtState::PoweredOnNoDev
}

fn format_status() -> String {
    match get_bt_state() {
        BtState::PoweredOff => "󰂲".to_string(),
        BtState::PoweredOnNoDev => "󰂯".to_string(),
        BtState::Connected(name) => {
            let short_name = if name.chars().count() > 12 {
                format!("{}…", name.chars().take(11).collect::<String>())
            } else {
                name
            };
            format!("󰂱 {}", short_name)
        }
    }
}

fn format_tooltip() -> String {
    match get_bt_state() {
        BtState::PoweredOff => {
            format!(
                "<span font_desc='JetBrainsMono Nerd Font 10.5'>\
                <span color='#c4746e' weight='bold'>{}</span>\n\
                <span color='#727169'>{}</span>\n\
                <span color='#727169' size='small'>{}</span>\
                </span>",
                tr("󰂲 Bluetooth is disabled", "󰂲 Bluetooth выключен"),
                tr("Adapter is turned off", "Адаптер отключен"),
                tr("LMB: Devices · RMB: Toggle · MMB: Bluetooth Manager", "ЛКМ: Устройства · ПКМ: Вкл/выкл · СКМ: Диспетчер Bluetooth")
            )
        }
        BtState::PoweredOnNoDev => {
            format!(
                "<span font_desc='JetBrainsMono Nerd Font 10.5'>\
                <span color='#8ba4b0' weight='bold'>{}</span>\n\
                <span color='#727169'>{}</span>\n\
                <span color='#727169' size='small'>{}</span>\
                </span>",
                tr("󰂯 Bluetooth active", "󰂯 Bluetooth активен"),
                tr("No connected devices", "Нет подключенных устройств"),
                tr("LMB: Devices · RMB: Toggle · MMB: Bluetooth Manager", "ЛКМ: Устройства · ПКМ: Вкл/выкл · СКМ: Диспетчер Bluetooth")
            )
        }
        BtState::Connected(name) => {
            format!(
                "<span font_desc='JetBrainsMono Nerd Font 10.5'>\
                <span color='#7fb4ca' weight='bold'>{}</span>\n\
                <span color='#727169'>{}</span> <span color='#e6e2cc'>{}</span>\n\
                <span color='#727169' size='small'>{}</span>\
                </span>",
                tr("󰂱 Connected", "󰂱 Подключено"),
                tr("Device:", "Устройство:"),
                name,
                tr("LMB: Devices · RMB: Toggle · MMB: Bluetooth Manager", "ЛКМ: Устройства · ПКМ: Вкл/выкл · СКМ: Диспетчер Bluetooth")
            )
        }
    }
}

fn format_json() -> String {
    let (class, text) = match get_bt_state() {
        BtState::PoweredOff => ("off", "󰂲".to_string()),
        BtState::PoweredOnNoDev => ("on", "󰂯".to_string()),
        BtState::Connected(ref name) => {
            let short_name = if name.chars().count() > 12 {
                format!("{}…", name.chars().take(11).collect::<String>())
            } else {
                name.clone()
            };
            ("connected", format!("󰂱 {}", short_name))
        }
    };
    let tooltip = format_tooltip();
    format!(
        "{{\"text\":\"{}\",\"tooltip\":\"{}\",\"class\":\"{}\"}}",
        escape_json(&text),
        escape_json(&tooltip),
        class
    )
}

fn watch_mode(waybar: bool) {
    let self_pipe = match SelfPipe::new() {
        Ok(sp) => sp,
        Err(e) => {
            eprintln!("Failed to setup self-pipe: {e}");
            return;
        }
    };

    let mut last_output = String::new();

    loop {
        let current = if waybar { format_json() } else { format_status() };
        if current != last_output {
            println!("{}", current);
            io::stdout().flush().ok();
            last_output = current;
        }

        if self_pipe.wait_event(3500) {
            inerium_i18n::Lang::reinit();
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--tooltip") {
        println!("{}", format_tooltip());
        return;
    }
    if args.iter().any(|a| a == "--waybar") {
        watch_mode(true);
        return;
    }
    if args.iter().any(|a| a == "--watch") {
        let waybar = args.iter().any(|a| a == "--json" || a == "--waybar");
        watch_mode(waybar);
        return;
    }

    println!("{}", format_status());
}
