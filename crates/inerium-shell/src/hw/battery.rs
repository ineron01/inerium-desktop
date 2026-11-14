use std::fs;
use std::path::{Path, PathBuf};

use super::sysfs::{read_sysfs_string, read_sysfs_u32};

pub enum BatteryBackend {
    PercentageThreshold {
        bat_dir: PathBuf,
        end_path: PathBuf,
        start_path: Option<PathBuf>,
    },
    BinaryConservationMode {
        bat_dir: PathBuf,
        control_path: PathBuf,
    },
    ReadOnly {
        bat_dir: Option<PathBuf>,
    },
}

impl BatteryBackend {
    pub fn probe() -> Self {
        let bat_dir = Self::find_battery_dir();
        if let Some(ref dir) = bat_dir {
            // 1. Standard Linux kernel percentage threshold (MSI, ASUS, ThinkPad, Huawei)
            let end_thresh = dir.join("charge_control_end_threshold");
            if end_thresh.exists() {
                let start_thresh = dir.join("charge_control_start_threshold");
                return Self::PercentageThreshold {
                    bat_dir: dir.clone(),
                    end_path: end_thresh,
                    start_path: if start_thresh.exists() { Some(start_thresh) } else { None },
                };
            }

            let stop_thresh = dir.join("charge_stop_threshold");
            if stop_thresh.exists() {
                let start_thresh = dir.join("charge_start_threshold");
                return Self::PercentageThreshold {
                    bat_dir: dir.clone(),
                    end_path: stop_thresh,
                    start_path: if start_thresh.exists() { Some(start_thresh) } else { None },
                };
            }

            // 2. Lenovo IdeaPad / Legion Conservation Mode
            let ideapad_driver = Path::new("/sys/bus/platform/drivers/ideapad_acpi");
            if let Ok(entries) = fs::read_dir(ideapad_driver) {
                for entry in entries.flatten() {
                    let cm = entry.path().join("conservation_mode");
                    if cm.exists() {
                        return Self::BinaryConservationMode {
                            bat_dir: dir.clone(),
                            control_path: cm,
                        };
                    }
                }
            }
        }

        Self::ReadOnly { bat_dir }
    }

    fn find_battery_dir() -> Option<PathBuf> {
        let base = Path::new("/sys/class/power_supply");
        let entries = fs::read_dir(base).ok()?;
        let mut fallback: Option<PathBuf> = None;

        for entry in entries.flatten() {
            let path = entry.path();
            let ptype = read_sysfs_string(&path.join("type")).unwrap_or_default();
            if ptype != "Battery" {
                continue;
            }
            let scope = read_sysfs_string(&path.join("scope")).unwrap_or_default();
            if scope == "Device" {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("BAT") || name.starts_with("CMB") {
                return Some(path);
            }
            if fallback.is_none() {
                fallback = Some(path);
            }
        }

        fallback
    }

    pub fn has_threshold(&self) -> bool {
        !matches!(self, Self::ReadOnly { .. })
    }

    pub fn get_threshold(&self) -> u32 {
        match self {
            Self::PercentageThreshold { end_path, .. } => {
                read_sysfs_u32(end_path).unwrap_or(100)
            }
            Self::BinaryConservationMode { control_path, .. } => {
                let val = read_sysfs_u32(control_path).unwrap_or(0);
                if val == 1 { 80 } else { 100 }
            }
            Self::ReadOnly { .. } => 100,
        }
    }

    pub fn set_threshold(&self, target_percent: u32) -> Result<(), String> {
        match self {
            Self::PercentageThreshold { end_path, start_path, .. } => {
                let start_val = if target_percent <= 80 { 75 } else { 95 };
                let current_end = read_sysfs_u32(end_path).unwrap_or(100);

                if target_percent > current_end {
                    // Raising limit (80 -> 100): Ceiling (end) first, then Floor (start)
                    // Prevents kernel invariant check start <= end from throwing EINVAL
                    fs::write(end_path, format!("{target_percent}\n"))
                        .map_err(|e| format!("Permission denied or write failed: {e}"))?;
                    if let Some(start) = start_path {
                        let _ = fs::write(start, format!("{start_val}\n"));
                    }
                } else {
                    // Lowering limit (100 -> 80): Floor (start) first, then Ceiling (end)
                    if let Some(start) = start_path {
                        let _ = fs::write(start, format!("{start_val}\n"));
                    }
                    fs::write(end_path, format!("{target_percent}\n"))
                        .map_err(|e| format!("Permission denied or write failed: {e}"))?;
                }
                Ok(())
            }
            Self::BinaryConservationMode { control_path, .. } => {
                let val = if target_percent <= 80 { "1\n" } else { "0\n" };
                fs::write(control_path, val)
                    .map_err(|e| format!("Permission denied or write failed: {e}"))
            }
            Self::ReadOnly { .. } => {
                Err("Battery does not support threshold control".to_string())
            }
        }
    }

    pub fn get_battery_line(&self) -> String {
        let bat_dir = match self {
            Self::PercentageThreshold { bat_dir, .. } => Some(bat_dir),
            Self::BinaryConservationMode { bat_dir, .. } => Some(bat_dir),
            Self::ReadOnly { bat_dir } => bat_dir.as_ref(),
        };

        if let Some(dir) = bat_dir
            && let Some(cap) = read_sysfs_u32(&dir.join("capacity"))
        {
            let status = read_sysfs_string(&dir.join("status")).unwrap_or_default();
            let is_charging = status == "Charging";
            let ico = if is_charging { "󰂄" } else { "󰂀" };
            return format!("{} {}%", ico, cap);
        }

        "󰚥 Сеть".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_threshold_invariant_ordering() {
        let tmp_dir = std::env::temp_dir().join(format!("inerium_test_bat_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).unwrap();

        let end_path = tmp_dir.join("charge_control_end_threshold");
        let start_path = tmp_dir.join("charge_control_start_threshold");

        // Initial state: 80% (start=75, end=80)
        fs::write(&end_path, "80\n").unwrap();
        fs::write(&start_path, "75\n").unwrap();

        let backend = BatteryBackend::PercentageThreshold {
            bat_dir: tmp_dir.clone(),
            end_path: end_path.clone(),
            start_path: Some(start_path.clone()),
        };

        // Raise to 100%: end should be written first, then start
        backend.set_threshold(100).unwrap();
        assert_eq!(read_sysfs_u32(&end_path), Some(100));
        assert_eq!(read_sysfs_u32(&start_path), Some(95));

        // Lower to 80%: start should be written first, then end
        backend.set_threshold(80).unwrap();
        assert_eq!(read_sysfs_u32(&end_path), Some(80));
        assert_eq!(read_sysfs_u32(&start_path), Some(75));

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_binary_conservation_mode() {
        let tmp_dir = std::env::temp_dir().join(format!("inerium_test_lenovo_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).unwrap();

        let control_path = tmp_dir.join("conservation_mode");
        fs::write(&control_path, "0\n").unwrap();

        let backend = BatteryBackend::BinaryConservationMode {
            bat_dir: tmp_dir.clone(),
            control_path: control_path.clone(),
        };

        assert_eq!(backend.get_threshold(), 100);

        backend.set_threshold(80).unwrap();
        assert_eq!(read_sysfs_u32(&control_path), Some(1));
        assert_eq!(backend.get_threshold(), 80);

        backend.set_threshold(100).unwrap();
        assert_eq!(read_sysfs_u32(&control_path), Some(0));
        assert_eq!(backend.get_threshold(), 100);

        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
