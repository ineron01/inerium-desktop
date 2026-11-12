//! Inerium Dragon — Native Rust Network Provider for Waybar
//! - Instant sysfs checking for Ethernet (eno1) and Wi-Fi (wlp4s0/wl*)
//! - Zero subprocess execution overhead (direct POSIX getifaddrs() system call)
//! - Watch mode: Streams network updates, reacts to SIGUSR1
//! - Tooltip: Rich Pango markup with interface name, IP address, state

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use inerium_tools::format::escape_json;
use inerium_tools::ipc::SelfPipe;
use inerium_tools::sys::posix::get_interface_ipv4;

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

enum NetState {
    Ethernet(String),
    Wifi(String),
    Disconnected,
}

fn detect_network() -> NetState {
    // 1. Check Ethernet interfaces dynamically (eno1, enp*, enx*, eth*)
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("en") || name.starts_with("eth") {
                let carrier_path = entry.path().join("carrier");
                if let Some(c) = read_trimmed(&carrier_path)
                    && c == "1" {
                        return NetState::Ethernet(name);
                    }
            }
        }
    }

    // 2. Check Wi-Fi interfaces
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("wl") {
                let oper_path = entry.path().join("operstate");
                if let Some(state) = read_trimmed(&oper_path)
                    && state == "up" {
                        return NetState::Wifi(name);
                    }
            }
        }
    }

    NetState::Disconnected
}

fn format_status() -> &'static str {
    match detect_network() {
        NetState::Ethernet(_) => "󰈀 Eth",
        NetState::Wifi(_) => "󰤨 WiFi",
        NetState::Disconnected => "󰤭",
    }
}

fn get_ip_address(iface: &str) -> Option<String> {
    get_interface_ipv4(iface)
}

fn format_tooltip() -> String {
    match detect_network() {
        NetState::Ethernet(iface) => {
            let ip = get_ip_address(&iface).unwrap_or_else(|| "Получение...".to_string());
            format!(
                "<span font_desc='JetBrainsMono Nerd Font 10.5'>\
                <span color='#87a987' weight='bold'>󰈀 Проводная сеть</span>\n\
                <span color='#727169'>Интерфейс:</span> <span color='#e6e2cc'>{}</span>\n\
                <span color='#727169'>IP-адрес:</span>   <span color='#8ba4b0'>{}</span>\n\
                <span color='#727169'>Статус:</span>     <span color='#87a987'>Подключено</span>\n\
                <span color='#727169' size='small'>ЛКМ: Сети и Wi-Fi · ПКМ: Вкл/выкл · СКМ: Настройка соединений</span>\
                </span>",
                iface, ip
            )
        }
        NetState::Wifi(iface) => {
            let ip = get_ip_address(&iface).unwrap_or_else(|| "Получение...".to_string());
            format!(
                "<span font_desc='JetBrainsMono Nerd Font 10.5'>\
                <span color='#7fb4ca' weight='bold'>󰤨 Беспроводная сеть</span>\n\
                <span color='#727169'>Интерфейс:</span> <span color='#e6e2cc'>{}</span>\n\
                <span color='#727169'>IP-адрес:</span>   <span color='#8ba4b0'>{}</span>\n\
                <span color='#727169'>Статус:</span>     <span color='#87a987'>В сети</span>\n\
                <span color='#727169' size='small'>ЛКМ: Сети и Wi-Fi · ПКМ: Вкл/выкл · СКМ: Настройка соединений</span>\
                </span>",
                iface, ip
            )
        }
        NetState::Disconnected => {
            "<span font_desc='JetBrainsMono Nerd Font 10.5'>\
            <span color='#c4746e' weight='bold'>󰤭 Сеть отключена</span>\n\
            <span color='#727169'>Интерфейсы не активны</span>\n\
            <span color='#727169' size='small'>ЛКМ: Сети и Wi-Fi · СКМ: Настройка соединений</span>\
            </span>".to_string()
        }
    }
}

fn format_json() -> String {
    let text = format_status();
    let tooltip = format_tooltip();
    let class = match detect_network() {
        NetState::Ethernet(_) => "ethernet",
        NetState::Wifi(_) => "wifi",
        NetState::Disconnected => "disconnected",
    };
    format!(
        "{{\"text\":\"{}\",\"tooltip\":\"{}\",\"class\":\"{}\"}}",
        escape_json(text),
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
        let current = if waybar { format_json() } else { format_status().to_string() };
        if current != last_output {
            println!("{}", current);
            io::stdout().flush().ok();
            last_output = current;
        }

        self_pipe.wait_event(3000);
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
