//! Inerium Dragon — Native Rust DND (Do Not Disturb) Watcher & Controller for Waybar
//! - Instant state sync via PidRegistry & SelfPipe (SIGUSR1)
//! - Controls Mako notification daemon modes (dnd & do-not-disturb)
//! - WirePlumber / PipeWire mute for notification audio & Telegram Desktop
//! - Waybar streaming JSON format

use std::env;
use std::io::{self, Write};
use std::process::Command;

use inerium_tools::format::escape_json;
use inerium_tools::ipc::{PidRegistry, SelfPipe};
use inerium_tools::sys::posix::SIGUSR1;

fn dnd_registry() -> PidRegistry {
    PidRegistry::new("dnd")
}

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

fn set_dnd_audio_mute(mute: bool) {
    let mute_str = if mute { "true" } else { "false" };

    // 1. Configure WirePlumber route-settings metadata for media.role:Notification
    let notif_json = format!(
        "{{\"mute\":{}, \"volume\":1.000000, \"channelMap\":[\"UNK\", \"UNK\"], \"channelVolumes\":[1.000000, 1.000000]}}",
        mute_str
    );
    let _ = Command::new("pw-metadata")
        .args([
            "-n",
            "route-settings",
            "0",
            "restore.stream.Output/Audio.media.role:Notification",
            &notif_json,
            "Spa:String:JSON",
        ])
        .output();

    // 2. Configure WirePlumber route-settings metadata for Telegram application streams
    let tg_json = format!(
        "{{\"mute\":{}, \"volume\":1.000000, \"channelMap\":[\"FL\", \"FR\"], \"channelVolumes\":[1.000000, 1.000000]}}",
        mute_str
    );
    let _ = Command::new("pw-metadata")
        .args([
            "-n",
            "route-settings",
            "0",
            "restore.stream.Output/Audio.application.name:Telegram",
            &tg_json,
            "Spa:String:JSON",
        ])
        .output();

    // 3. Mute/unmute active Telegram sink-inputs via pactl if any stream is currently playing
    if let Ok(out) = Command::new("pactl").args(["list", "sink-inputs"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut cur_id: Option<String> = None;
        let mute_flag = if mute { "1" } else { "0" };

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Sink Input #") {
                cur_id = trimmed.strip_prefix("Sink Input #").map(|s| s.to_string());
            } else if trimmed.contains("application.name = \"Telegram\"")
                && let Some(ref id) = cur_id
            {
                let _ = Command::new("pactl")
                    .args(["set-sink-input-mute", id, mute_flag])
                    .output();
            }
        }
    }
}

pub fn toggle_dnd() {
    let currently_active = is_dnd_active();
    let next_active = !currently_active;

    if next_active {
        let _ = Command::new("makoctl")
            .args(["mode", "-a", "dnd", "-a", "do-not-disturb"])
            .output();
        set_dnd_audio_mute(true);
    } else {
        let _ = Command::new("makoctl")
            .args(["mode", "-r", "dnd", "-r", "do-not-disturb"])
            .output();
        set_dnd_audio_mute(false);
    }

    dnd_registry().notify_all(SIGUSR1);
}

pub fn notify_dnd() {
    dnd_registry().notify_all(SIGUSR1);
}

fn format_output(dnd: bool) -> String {
    let (icon, class, tooltip) = if dnd {
        (
            "󰂛",
            "on",
            "Уведомления: выключены (Не беспокоить)\nЛКМ / ПКМ: Включить уведомления",
        )
    } else {
        (
            "󰂚",
            "off",
            "Уведомления: включены\nЛКМ / ПКМ: Выключить уведомления (Не беспокоить)",
        )
    };

    format!(
        "{{\"text\":\"{}\",\"tooltip\":\"{}\",\"class\":\"{}\"}}",
        escape_json(icon),
        escape_json(tooltip),
        class
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--toggle") {
        toggle_dnd();
        let cur = is_dnd_active();
        println!("{}", if cur { "on" } else { "off" });
        return;
    }

    if args.iter().any(|a| a == "--notify") {
        notify_dnd();
        return;
    }

    if args.iter().any(|a| a == "--status") {
        println!("{}", if is_dnd_active() { "on" } else { "off" });
        return;
    }

    // Streaming mode for Waybar
    let reg = dnd_registry();
    let _guard = reg.register();

    let self_pipe = match SelfPipe::new() {
        Ok(sp) => sp,
        Err(e) => {
            eprintln!("Failed to initialize self pipe: {e}");
            return;
        }
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    let mut current_state = is_dnd_active();
    let _ = writeln!(handle, "{}", format_output(current_state));
    let _ = handle.flush();

    loop {
        // Wait for SIGUSR1 notification or fallback poll every 15 seconds
        self_pipe.wait_event(15000);

        let new_state = is_dnd_active();
        current_state = new_state;
        let _ = writeln!(handle, "{}", format_output(current_state));
        let _ = handle.flush();
    }
}
