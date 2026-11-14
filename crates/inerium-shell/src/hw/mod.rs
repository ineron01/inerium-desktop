use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

pub mod audio;
pub mod backlight;
pub mod battery;
pub mod cooling;
pub mod network;
pub mod power;
pub mod sysfs;
pub mod system;

// Re-export common structs
pub use audio::AudioSink;
pub use sysfs::home_dir;
pub use system::{MonitorTarget, SystemUpdates, WallpaperItem};

pub struct HardwareContext {
    pub cooling: cooling::CoolingSubsystem,
    pub battery: battery::BatteryBackend,
    pub backlight: backlight::BacklightBackend,
    pub power: power::PowerBackend,
}

static HW: OnceLock<HardwareContext> = OnceLock::new();

pub fn ctx() -> &'static HardwareContext {
    HW.get_or_init(|| HardwareContext {
        cooling: cooling::CoolingSubsystem::probe(),
        battery: battery::BatteryBackend::probe(),
        backlight: backlight::BacklightBackend::probe(),
        power: power::PowerBackend::probe(),
    })
}

// ============================================================================
// 1. COOLING & FAN TELEMETRY FACADE
// ============================================================================
pub fn has_msi_ec() -> bool {
    matches!(ctx().cooling.control, cooling::PlatformControl::MsiEc { .. })
}

#[allow(dead_code)]
pub fn has_msi_ec_at(path: &Path) -> bool {
    path.exists()
}

pub fn has_fan_telemetry() -> bool {
    ctx().cooling.has_fan_telemetry()
}

pub fn get_cooler_boost() -> bool {
    ctx().cooling.control.get_cooler_boost()
}

pub fn set_cooler_boost(on: bool) {
    ctx().cooling.control.set_cooler_boost(on);
}

#[allow(dead_code)]
pub fn get_fan_mode() -> String {
    ctx().cooling.control.get_fan_mode()
}

#[allow(dead_code)]
pub fn format_fan_meta() -> String {
    ctx().cooling.format_fan_meta()
}

#[allow(dead_code)]
pub fn apply_balanced_fan_curve() {
    ctx().cooling.control.apply_balanced_fan_curve();
}

pub fn get_fan_telemetry() -> (Option<u32>, Option<u32>, Option<u32>, Option<u32>) {
    ctx().cooling.get_telemetry()
}

#[allow(dead_code)]
pub fn format_fan_telemetry() -> String {
    ctx().cooling.format_fan_telemetry()
}

// ============================================================================
// 2. BATTERY & THRESHOLDS FACADE
// ============================================================================
pub fn has_battery_threshold() -> bool {
    ctx().battery.has_threshold()
}

pub fn get_battery_threshold() -> u32 {
    ctx().battery.get_threshold()
}

pub fn set_battery_threshold(thresh: u32) {
    if let Err(e) = ctx().battery.set_threshold(thresh) {
        eprintln!("[inerium-shell] Failed to set battery threshold: {e}");
    }
}

pub fn get_battery_line() -> String {
    ctx().battery.get_battery_line()
}

// ============================================================================
// 3. POWER PROFILES FACADE
// ============================================================================
pub fn has_power_profiles() -> bool {
    ctx().power.has_power_profiles()
}

pub fn get_power_profile() -> String {
    ctx().power.get_power_profile()
}

pub fn set_power_profile(profile: &str) {
    ctx().power.set_power_profile(profile, has_msi_ec());
}

// ============================================================================
// 4. BACKLIGHT & DDC/CI FACADE
// ============================================================================
pub fn has_backlight() -> bool {
    ctx().backlight.has_backlight()
}

pub fn get_laptop_brightness() -> u32 {
    ctx().backlight.get_brightness()
}

pub fn set_laptop_brightness(pct: u32) {
    ctx().backlight.set_brightness(pct);
}

pub fn has_external_monitor() -> bool {
    backlight::has_external_monitor()
}

pub fn get_cached_ddc_brightness() -> u32 {
    backlight::get_cached_ddc_brightness()
}

pub fn refresh_ddc_brightness_async() {
    backlight::refresh_ddc_brightness_async();
}

#[allow(dead_code)]
pub fn get_ddc_brightness() -> u32 {
    backlight::get_ddc_brightness()
}

pub fn set_ddc_brightness_async(val: u32) {
    backlight::set_ddc_brightness_async(val);
}

// ============================================================================
// 5. AUDIO PIPEWIRE / WIREPLUMBER FACADE
// ============================================================================
pub fn get_volume() -> u32 {
    audio::get_volume()
}

pub fn is_volume_muted() -> bool {
    audio::is_volume_muted()
}

pub fn set_volume(pct: u32) {
    audio::set_volume(pct);
}

pub fn toggle_volume_mute() {
    audio::toggle_volume_mute();
}

pub fn is_mic_muted() -> bool {
    audio::is_mic_muted()
}

pub fn toggle_mic_mute() {
    audio::toggle_mic_mute();
}

pub fn list_audio_sinks() -> Vec<AudioSink> {
    audio::list_audio_sinks()
}

pub fn set_default_sink(id: u32) {
    audio::set_default_sink(id);
}

// ============================================================================
// 6. NETWORK & BLUETOOTH FACADE
// ============================================================================
pub fn is_wifi_enabled() -> bool {
    network::is_wifi_enabled()
}

pub fn get_active_wifi_ssid() -> Option<String> {
    network::get_active_wifi_ssid()
}

pub fn toggle_wifi(enable: bool) {
    network::toggle_wifi(enable);
}

pub fn is_hotspot_active() -> bool {
    network::is_hotspot_active()
}

pub fn has_bluetooth() -> bool {
    network::has_bluetooth()
}

pub fn is_bluetooth_enabled() -> bool {
    network::is_bluetooth_enabled()
}

pub fn toggle_bluetooth(enable: bool) {
    network::toggle_bluetooth(enable);
}

// ============================================================================
// 7. SYSTEM & SESSION FACADE
// ============================================================================
pub fn is_hypridle_active() -> bool {
    system::is_hypridle_active()
}

pub fn toggle_hypridle(enable: bool) {
    system::toggle_hypridle(enable);
}

pub fn list_connected_monitors() -> Vec<MonitorTarget> {
    system::list_connected_monitors()
}

pub fn get_active_wallpapers() -> HashMap<String, String> {
    system::get_active_wallpapers()
}

#[allow(dead_code)]
pub fn get_active_wallpaper() -> String {
    system::get_active_wallpaper()
}

pub fn list_available_wallpapers() -> Vec<WallpaperItem> {
    system::list_available_wallpapers()
}

pub fn inerium_wallpapers_dir() -> std::path::PathBuf {
    system::inerium_wallpapers_dir()
}

pub fn set_wallpaper_async(path: String, monitor: Option<String>) {
    system::set_wallpaper_async(path, monitor);
}

pub fn is_dnd_active() -> bool {
    system::is_dnd_active()
}

pub fn toggle_dnd() {
    system::toggle_dnd();
}

pub fn get_updates_info() -> SystemUpdates {
    system::get_updates_info()
}

pub fn has_envycontrol() -> bool {
    system::has_envycontrol()
}

pub fn get_gpu_mode() -> String {
    system::get_gpu_mode()
}

pub fn switch_gpu_mode(mode: &str) -> bool {
    system::switch_gpu_mode(mode)
}

pub fn get_hostname() -> String {
    system::get_hostname()
}

pub fn get_uptime_line() -> String {
    system::get_uptime_line()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_detection_nonexistent_paths() {
        assert!(!has_msi_ec_at(Path::new("/nonexistent/msi-ec")));
    }
}
