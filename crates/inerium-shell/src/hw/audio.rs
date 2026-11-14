use std::process::Command;
use std::thread;

pub struct AudioSink {
    pub id: u32,
    pub name: String,
    pub is_default: bool,
}

pub fn get_volume() -> u32 {
    if let Ok(output) = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output() {
        let s = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2
            && let Ok(val) = parts[1].parse::<f64>()
        {
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
