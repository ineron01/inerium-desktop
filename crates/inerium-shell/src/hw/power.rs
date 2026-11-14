use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::thread;

static HAS_POWER_PROFILES: OnceLock<bool> = OnceLock::new();

pub struct PowerBackend;

impl PowerBackend {
    pub fn probe() -> Self {
        Self
    }

    pub fn has_power_profiles(&self) -> bool {
        *HAS_POWER_PROFILES.get_or_init(|| {
            Command::new("powerprofilesctl")
                .arg("get")
                .output()
                .is_ok_and(|o| o.status.success())
        })
    }

    pub fn get_power_profile(&self) -> String {
        if let Ok(output) = Command::new("powerprofilesctl").arg("get").output() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
        "balanced".to_string()
    }

    pub fn set_power_profile(&self, profile: &str, msi_ec_present: bool) {
        let p = profile.to_string();
        thread::spawn(move || {
            let _ = Command::new("powerprofilesctl").args(["set", &p]).output();
            if msi_ec_present {
                let shift = match p.as_str() {
                    "power-saver" => "eco",
                    "performance" => "turbo",
                    _ => "comfort",
                };
                let shift_path = Path::new("/sys/devices/platform/msi-ec/shift_mode");
                if shift_path.exists() {
                    let _ = fs::write(shift_path, format!("{}\n", shift));
                }
            }
        });
    }
}
