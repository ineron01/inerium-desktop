//! Kanagawa Dragon — Native Rust Clipboard Manager for Rofi & cliphist
//! - Instant startup, zero Python runtime overhead
//! - Reads cliphist history and generates 56x56 thumbnails via ImageMagick streaming pipe
//! - Integrates with screenshot metadata (~/.cache/cliphist/image-meta.json)
//! - Supports --backup flag for backup session styling

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const THUMB_SIZE: u32 = 56;
const MAX_HISTORY_ITEMS: usize = 100;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ImageMetaItem {
    #[serde(default)]
    filename: String,
    #[serde(default)]
    datetime: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(rename = "type", default)]
    item_type: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CachedLabel {
    label: String,
}

fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir())
}

fn cache_dir() -> PathBuf {
    home_dir().join(".cache/cliphist")
}

fn thumbs_dir() -> PathBuf {
    cache_dir().join("thumbs")
}

fn meta_file_path() -> PathBuf {
    cache_dir().join("image-meta.json")
}

fn load_metadata() -> BTreeMap<String, ImageMetaItem> {
    let p = meta_file_path();
    if let Ok(data) = fs::read_to_string(&p)
        && let Ok(map) = serde_json::from_str(&data) {
            return map;
        }
    BTreeMap::new()
}

fn save_metadata(mut meta: BTreeMap<String, ImageMetaItem>) {
    let p = meta_file_path();
    if meta.len() > MAX_HISTORY_ITEMS {
        let keys_to_remove = meta.len() - MAX_HISTORY_ITEMS;
        let keys: Vec<String> = meta.keys().take(keys_to_remove).cloned().collect();
        for k in keys {
            meta.remove(&k);
        }
    }
    if let Ok(s) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(p, s);
    }
}

fn cleanup_thumbs(active_cids: &HashSet<&str>) {
    let dir = thumbs_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != "png" && ext != "json" {
            continue;
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && !active_cids.contains(stem)
        {
            let _ = fs::remove_file(&path);
        }
    }
}

fn get_cliphist_items() -> Vec<(String, String)> {
    let output = match Command::new("cliphist").arg("list").output() {
        Ok(out) if out.status.success() => out.stdout,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output);
    let mut items = Vec::new();

    for line in text.lines() {
        if let Some((cid, content)) = line.split_once('\t') {
            items.push((cid.trim().to_string(), content.to_string()));
            if items.len() >= MAX_HISTORY_ITEMS {
                break;
            }
        }
    }

    items
}

fn extract_dimensions(content: &str) -> String {
    // "[[ binary data 168 KiB png 720x920 ]]"
    for word in content.split_whitespace() {
        if let Some((w, h)) = word.split_once('x')
            && !w.is_empty()
                && !h.is_empty()
                && w.chars().all(|c| c.is_ascii_digit())
                && h.chars().all(|c| c.is_ascii_digit())
            {
                return format!("({}x{})", w, h);
            }
    }
    String::new()
}

fn format_current_timestamp() -> (String, String) {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const std::ffi::c_char,
    }

    unsafe extern "C" {
        fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    }

    let mut tm: Tm = unsafe { std::mem::zeroed() };
    unsafe {
        localtime_r(&now, &mut tm);
    }

    let year = tm.tm_year + 1900;
    let month = tm.tm_mon + 1;
    let day = tm.tm_mday;
    let hour = tm.tm_hour;
    let min = tm.tm_min;
    let sec = tm.tm_sec;

    let human = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hour, min, sec
    );
    let fname = format!(
        "image_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.png",
        year, month, day, hour, min, sec
    );

    (human, fname)
}

fn process_image_item(
    cid: &str,
    content: &str,
    meta_registry: &mut BTreeMap<String, ImageMetaItem>,
    meta_modified: &mut bool,
) -> (String, String) {
    let thumbs = thumbs_dir();
    let thumb_path = thumbs.join(format!("{}.png", cid));
    let meta_cache_path = thumbs.join(format!("{}.json", cid));

    // Fast path: thumbnail and label are already cached
    if thumb_path.exists() && meta_cache_path.exists()
        && let Ok(c) = fs::read_to_string(&meta_cache_path)
            && let Ok(cached) = serde_json::from_str::<CachedLabel>(&c) {
                return (cached.label, thumb_path.to_string_lossy().into_owned());
            }

    let dim_str = extract_dimensions(content);

    // Decode image from cliphist
    let img_data = match Command::new("cliphist").args(["decode", cid]).output() {
        Ok(out) if out.status.success() => out.stdout,
        _ => {
            let label = if dim_str.is_empty() {
                "󰋩  [Изображение]".to_string()
            } else {
                format!("󰋩  [Изображение {}]", dim_str)
            };
            return (label, "image-x-generic".to_string());
        }
    };

    if img_data.is_empty() {
        return ("󰋩  [Пустое изображение]".to_string(), "image-x-generic".to_string());
    }

    // Generate thumbnail using ImageMagick
    if !thumb_path.exists() {
        let size_arg = format!("{}x{}", THUMB_SIZE, THUMB_SIZE);
        if let Ok(mut child) = Command::new("magick")
            .args(["-", "-thumbnail", &size_arg, thumb_path.to_str().unwrap_or("")])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(&img_data);
            }
            let _ = child.wait();
        }
    }

    // SHA256 of original image
    let hash = format!("{:x}", Sha256::digest(&img_data));

    let label = if let Some(info) = meta_registry.get(&hash) {
        if !info.filename.is_empty() {
            format!("󰋩  {}  {}", info.filename, dim_str).trim().to_string()
        } else {
            format!("󰋩  Скриншот {}  {}", info.datetime, dim_str).trim().to_string()
        }
    } else {
        let (now_str, fname) = format_current_timestamp();
        meta_registry.insert(
            hash,
            ImageMetaItem {
                filename: fname,
                datetime: now_str.clone(),
                path: None,
                item_type: "image".to_string(),
            },
        );
        *meta_modified = true;
        format!("󰋩  Изображение {}  {}", now_str, dim_str).trim().to_string()
    };

    // Cache computed label
    let _ = fs::write(
        &meta_cache_path,
        serde_json::to_string(&CachedLabel {
            label: label.clone(),
        })
        .unwrap_or_default(),
    );

    let icon = if thumb_path.exists() {
        thumb_path.to_string_lossy().into_owned()
    } else {
        "image-x-generic".to_string()
    };

    (label, icon)
}

fn process_text_item(content: &str) -> (String, &'static str) {
    let mut clean = String::with_capacity(content.len());
    let mut last_was_space = false;

    for c in content.chars() {
        if c.is_whitespace() {
            if !last_was_space && !clean.is_empty() {
                clean.push(' ');
                last_was_space = true;
            }
        } else {
            clean.push(c);
            last_was_space = false;
        }
    }

    let trimmed = clean.trim();
    let char_count = trimmed.chars().count();
    let label = if char_count > 85 {
        let truncated: String = trimmed.chars().take(82).collect();
        format!("{}...", truncated)
    } else {
        trimmed.to_string()
    };

    (label, "text-x-generic")
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("kanagawa-clipboard — Native Rust Clipboard Manager for Rofi & cliphist\n\nUsage: kanagawa-clipboard [--backup]");
        return;
    }
    let is_backup = args.iter().any(|a| a == "--backup" || a == "-b");

    let items = get_cliphist_items();
    if items.is_empty() {
        return;
    }

    let _ = fs::create_dir_all(thumbs_dir());
    let active_cids: HashSet<&str> = items.iter().map(|(id, _)| id.as_str()).collect();
    cleanup_thumbs(&active_cids);

    let mut meta_registry = load_metadata();
    let mut meta_modified = false;

    let mut rofi_input = Vec::new();

    for (cid, content) in &items {
        if content.starts_with("[[ binary data") {
            let (label, icon) =
                process_image_item(cid, content, &mut meta_registry, &mut meta_modified);
            rofi_input.push(format!("{}\0icon\x1f{}\n", label, icon));
        } else {
            let (label, icon) = process_text_item(content);
            rofi_input.push(format!("{}\0icon\x1f{}\n", label, icon));
        }
    }

    if meta_modified {
        save_metadata(meta_registry);
    }

    let prompt = if is_backup {
        "󰅍  Буфер обмена (Резерв)"
    } else {
        "󰅍  Буфер обмена"
    };

    let theme_file = if is_backup {
        home_dir().join(".config/rofi/clipboard_backup.rasi")
    } else {
        home_dir().join(".config/rofi/clipboard.rasi")
    };

    let mut rofi_cmd = Command::new("rofi");
    rofi_cmd.args([
        "-dmenu",
        "-i",
        "-p",
        prompt,
        "-format",
        "i",
        "-theme",
        theme_file.to_str().unwrap_or(""),
        "-show-icons",
    ]);

    let mut child = match rofi_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[kanagawa-clipboard] Failed to spawn rofi: {e}");
            return;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        for line in rofi_input {
            let _ = stdin.write_all(line.as_bytes());
        }
    }

    let output = match child.wait_with_output() {
        Ok(out) => out,
        Err(_) => return,
    };

    if output.status.success() {
        let out_str = String::from_utf8_lossy(&output.stdout);
        if let Ok(idx) = out_str.trim().parse::<usize>()
            && idx < items.len() {
                let selected_id = &items[idx].0;

                // Decode from cliphist and copy to Wayland clipboard
                if let Ok(mut dec_child) = Command::new("cliphist")
                    .args(["decode", selected_id])
                    .stdout(Stdio::piped())
                    .spawn()
                {
                    if let Some(dec_out) = dec_child.stdout.take() {
                        let _ = Command::new("wl-copy")
                            .stdin(dec_out)
                            .status();
                    }
                    let _ = dec_child.wait();
                }
            }
    }
}
