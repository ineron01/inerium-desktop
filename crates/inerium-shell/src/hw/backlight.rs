use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

use super::sysfs::read_sysfs_u32;

pub struct BacklightBackend {
    pub brightness_path: Option<PathBuf>,
    pub max_brightness: u32,
    pub dev_name: Option<String>,
}

impl BacklightBackend {
    pub fn probe() -> Self {
        let base = Path::new("/sys/class/backlight");
        let (dir, dev_name) = Self::find_best_backlight(base);

        let (brightness_path, max_brightness) = if let Some(ref d) = dir {
            let max = read_sysfs_u32(&d.join("max_brightness")).unwrap_or(255);
            (Some(d.join("brightness")), max)
        } else {
            (None, 255)
        };

        Self {
            brightness_path,
            max_brightness,
            dev_name,
        }
    }

    fn find_best_backlight(base: &Path) -> (Option<PathBuf>, Option<String>) {
        let entries = match fs::read_dir(base) {
            Ok(e) => e,
            Err(_) => return (None, None),
        };

        let mut best: Option<(usize, PathBuf, String)> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            let prio = if name.starts_with("intel_backlight")
                || name.starts_with("amdgpu_bl")
                || name.starts_with("nvidia_wmi")
            {
                0
            } else if name.starts_with("apple_backlight") || name.contains("backlight") {
                1
            } else {
                2
            };

            if best.as_ref().is_none_or(|(p, _, _)| prio < *p) {
                best = Some((prio, path, name));
            }
        }

        match best {
            Some((_, path, name)) => (Some(path), Some(name)),
            None => (None, None),
        }
    }

    pub fn has_backlight(&self) -> bool {
        self.brightness_path.is_some()
    }

    pub fn get_brightness(&self) -> u32 {
        if let Some(ref p) = self.brightness_path
            && let Some(cur) = read_sysfs_u32(p)
            && self.max_brightness > 0
        {
            return ((cur as f64 / self.max_brightness as f64) * 100.0).round() as u32;
        }
        50
    }

    pub fn set_brightness(&self, pct: u32) {
        let clamped = pct.clamp(5, 100);
        let dev_arg = self.dev_name.clone();
        thread::spawn(move || {
            let mut cmd = Command::new("brightnessctl");
            if let Some(ref dev) = dev_arg {
                cmd.args(["-d", dev]);
            }
            let _ = cmd.args(["-q", "s", &format!("{}%", clamped)]).output();
        });
    }
}

// -------------------------------------------------------------
// DDC/CI (External Monitor via ddcutil)
// -------------------------------------------------------------
enum DdcCmd {
    Set(u32),
}

static DDC_TX: OnceLock<Sender<DdcCmd>> = OnceLock::new();
static DDC_CACHE: Mutex<u32> = Mutex::new(50);
static DDC_BUS: OnceLock<String> = OnceLock::new();

fn get_ddc_bus() -> &'static str {
    DDC_BUS.get_or_init(|| {
        if let Ok(output) = Command::new("ddcutil").args(["detect", "--terse"]).output() {
            let s = String::from_utf8_lossy(&output.stdout);
            for line in s.lines() {
                if let Some(pos) = line.find("/dev/i2c-") {
                    let num: String = line[pos + 9..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if !num.is_empty() {
                        return num;
                    }
                }
            }
        }
        "3".to_string()
    })
}

pub fn has_external_monitor() -> bool {
    let card_dir = "/sys/class/drm";
    if let Ok(entries) = fs::read_dir(card_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("card") && name.contains("HDMI") {
                let status_path = entry.path().join("status");
                if let Ok(status) = fs::read_to_string(&status_path)
                    && status.trim() == "connected" {
                        return true;
                    }
            }
        }
    }
    false
}

fn init_ddc_worker() -> Sender<DdcCmd> {
    let (tx, rx) = channel::<DdcCmd>();
    thread::spawn(move || {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                DdcCmd::Set(val) => {
                    let bus = get_ddc_bus();
                    let _ = Command::new("ddcutil")
                        .args(["setvcp", "10", &val.to_string(), "--bus", bus])
                        .output();
                    if let Ok(mut g) = DDC_CACHE.lock() {
                        *g = val;
                    }
                }
            }
        }
    });
    tx
}

pub fn get_cached_ddc_brightness() -> u32 {
    DDC_CACHE.lock().map(|g| *g).unwrap_or(50)
}

pub fn refresh_ddc_brightness_async() {
    if !has_external_monitor() {
        return;
    }
    thread::spawn(move || {
        let bus = get_ddc_bus();
        if let Ok(output) = Command::new("ddcutil")
            .args(["getvcp", "10", "--bus", bus, "--brief"])
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 4
                && let Ok(v) = parts[3].parse::<u32>() {
                    if let Ok(mut g) = DDC_CACHE.lock() {
                        *g = v;
                    }
                    glib::MainContext::default().invoke(move || {
                        crate::views::control_center::on_ddc_updated(v);
                    });
                }
        }
    });
}

#[allow(dead_code)]
pub fn get_ddc_brightness() -> u32 {
    get_cached_ddc_brightness()
}

pub fn set_ddc_brightness_async(val: u32) {
    if !has_external_monitor() {
        return;
    }
    let tx = DDC_TX.get_or_init(init_ddc_worker);
    let _ = tx.send(DdcCmd::Set(val.clamp(0, 100)));
}
