use std::process::Command;

pub fn notify_info(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "Dolphin Converter",
            "-i",
            "document-save",
            "-u",
            "normal",
            title,
            body,
        ])
        .spawn();
}

pub fn notify_error(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "Dolphin Converter",
            "-i",
            "dialog-error",
            "-u",
            "critical",
            title,
            body,
        ])
        .spawn();
}
