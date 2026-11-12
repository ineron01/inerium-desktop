//! Inerium Dragon — Software Fan Regulator for MSI Bravo 15 C7VE (MS-158N, EC 158NIMS1.10F)
//!
//! Root cause:
//! The EC firmware on this model (158NIMS1.10F) has a microcode bug where it never
//! transitions past step 0 of its fan tables for CPU temperatures.
//! This native daemon dynamically reads high-precision silicon temperatures (k10temp Tctl)
//! and sets steps 0..=6 (0x72..=0x78 CPU, 0x8A..=0x90 GPU) directly in EC RAM.
//! On Cooler Boost 5 with shared heatpipes, the daemon also coordinates dual-fan assist
//! under sustained high CPU workloads even when discrete GPU is asleep in D3cold.
//!
//! Safety guarantees:
//! - Validates EC firmware signature (158NIMS1.10F) at offset 0xA0 before writing.
//! - Strictly enforces fan_mode = advanced (0x8D at 0xD4) so EC honours custom RAM tables.
//! - Emergency Cooler Boost (6000 RPM hardware overdrive) if CPU >= 95°C or GPU >= 90°C.
//! - Dual Cooler Boost path: sysfs platform driver with direct EC register 0x98 bit 7 fallback.
//! - Signal traps (SIGINT/SIGTERM/SIGQUIT) restore safe 50% fan PWM on exit without disabling emergency boost if overheated.
//! - Targeted EC reads (15 bytes/s vs 256 bytes) via pread: eliminates LPC bus congestion and SCI storms.
//! - Zero stall zone: motors never driven in 1..=31% PWM zone, instant jump to/from 0%.
//! - Sensor-aware dT/dt derivative anticipation decoupled from acoustic ramp smoothing.
//! - Resilient across sleep/resume using CLOCK_BOOTTIME without 18s spool-down lag or false derivative spikes.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::FileExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
    fn clock_gettime(clk_id: i32, tp: *mut Timespec) -> i32;
}
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const CLOCK_BOOTTIME: i32 = 7;

/// Read monotonic boottime clock that continues advancing across ACPI suspend/resume
fn get_boottime() -> Duration {
    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    if unsafe { clock_gettime(CLOCK_BOOTTIME, &mut ts) } == 0 && ts.tv_sec >= 0 && ts.tv_nsec >= 0 {
        Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }
}

struct SingleInstanceLock {
    _file: File,
}

impl SingleInstanceLock {
    fn acquire() -> Result<Self, String> {
        let path = "/run/lock/msi-fan-regulator.lock";
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| format!("Cannot open lock file {}: {}", path, e))?;

        let fd = file.as_raw_fd();
        let ret = unsafe { flock(fd, LOCK_EX | LOCK_NB) };
        if ret != 0 {
            let pid = fs::read_to_string(path).unwrap_or_default().trim().to_string();
            let pid_info = if !pid.is_empty() { format!(" (PID {})", pid) } else { String::new() };
            return Err(format!(
                "Another instance of msi-fan-regulator is already running{}.\n\
                 The system service manages fan cooling in the background.\n\
                 To inspect live status and telemetry, run: sudo msi-fan-regulator --status",
                pid_info
            ));
        }

        let _ = file.set_len(0);
        let mut f = &file;
        let _ = writeln!(f, "{}", std::process::id());

        Ok(Self { _file: file })
    }
}

const EC_IO: &str = "/sys/kernel/debug/ec/ec0/io";
const MSI_EC: &str = "/sys/devices/platform/msi-ec";
const EXPECTED_FW: &[u8] = b"158NIMS1.10F";
const GPU_PCI_STATUS: &str = "/sys/bus/pci/devices/0000:01:00.0/power/runtime_status";

// EC RAM offsets
const REG_FW: u64 = 0xA0;
const REG_CPU_TEMP: usize = 0x68;
const REG_GPU_TEMP: usize = 0x80;
const REG_CPU_STEP0: u64 = 0x72;
const REG_GPU_STEP0: u64 = 0x8A;
const REG_FAN_MODE: usize = 0xD4;
const REG_COOLER_BOOST: u64 = 0x98;
const COOLER_BOOST_BIT: u8 = 0x80;
const BOOST_STATE_FILE: &str = "/run/lock/msi-fan-regulator.boost";

// Advanced mode (0x8D) is required so the EC honours custom fan tables in EC RAM (0x72..0x78 and 0x8A..0x90).
// In auto (0x0D) or silent (0x1D), the EC uses fixed internal factory curves and ignores RAM tables.
const FAN_MODE_ADVANCED: u8 = 0x8D;

// Minimum spinning PWM of brushless fan impeller (prevents stall zone operation in 1..=31%)
const MIN_SPIN_PWM: u8 = 32;

// Thermal curves: (temperature °C, fan PWM %)
// Optimized "Cool & Responsive" profile:
// - Retains silent 0 RPM below 42°C (46 - STOP_HYSTERESIS)
// - Pre-emptive steep curve (68% at 65°C, 82% at 72°C, 100% at 84°C)
// - Anticipatory dT/dt derivative cooling to overcome motor spool-up inertia
const CPU_CURVE: &[(i32, u8)] = &[
    (46, 32),
    (52, 40),
    (58, 52),
    (65, 68),
    (72, 82),
    (78, 92),
    (84, 100),
];

const GPU_CURVE: &[(i32, u8)] = &[
    (46, 32),
    (52, 40),
    (60, 58),
    (68, 75),
    (74, 90),
    (80, 100),
];

const STOP_HYSTERESIS: i32 = 4; // Fan stops only when dropping below 42°C
const DOWN_STEP: u8 = 3;        // Max % decrease per second (smooth acoustic spin-down)
const UP_STEP: u8 = 10;         // Max % increase per second below IMMEDIATE_TEMP
const IMMEDIATE_TEMP: i32 = 58; // Above 58°C, ramp-up is immediate to counter rapid thermal rise
const SAFE_PCT: u8 = 50;        // Safe speed written on daemon exit or failure

const EMERGENCY_CPU: i32 = 95;  // AMD Ryzen 7 7735HS Tjmax threshold
const EMERGENCY_GPU: i32 = 90;  // RTX 4050 mobile thermal limit
const RELEASE_CPU: i32 = 85;    // Cooler Boost release threshold
const RELEASE_GPU: i32 = 75;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_signal(_: i32) {
    RUNNING.store(false, Ordering::SeqCst);
}

extern "C" fn handle_sighup(_: i32) {
    // SIGHUP is intentionally ignored so terminal disconnects or login session exits
    // do not inadvertently terminate fan cooling regulation.
}

fn setup_signals() {
    inerium_tools::sys::posix_signal(inerium_tools::sys::SIGINT, handle_signal);
    inerium_tools::sys::posix_signal(inerium_tools::sys::SIGTERM, handle_signal);
    inerium_tools::sys::posix_signal(inerium_tools::sys::SIGQUIT, handle_signal);
    inerium_tools::sys::posix_signal(inerium_tools::sys::SIGHUP, handle_sighup);
}

/// Find k10temp hwmon temp1_input path dynamically
fn find_k10temp_path() -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(name) = fs::read_to_string(p.join("name"))
                && name.trim() == "k10temp"
            {
                let temp_file = p.join("temp1_input");
                if temp_file.exists() {
                    return Some(temp_file);
                }
            }
        }
    }
    None
}

/// Linear interpolation between curve points with hysteresis
fn calculate_curve_pct(curve: &[(i32, u8)], temp: Option<i32>, running: bool) -> u8 {
    let Some(t) = temp else { return 0 };
    let (first_t, first_p) = curve[0];

    if t < first_t {
        if running && t >= (first_t - STOP_HYSTERESIS) {
            return first_p;
        }
        return 0;
    }

    for window in curve.windows(2) {
        let (t0, p0) = window[0];
        let (t1, p1) = window[1];
        if t <= t1 {
            let pct = p0 as i32 + ((p1 as i32 - p0 as i32) * (t - t0)) / (t1 - t0);
            return pct.clamp(0, 100) as u8;
        }
    }

    curve.last().map(|&(_, p)| p).unwrap_or(100)
}

/// Smooth PWM transitions to prevent abrupt fan noise and completely avoid the stall zone (1..=31%)
fn smooth_pct(prev: u8, target: u8, temp: i32) -> u8 {
    if target == prev {
        return prev;
    }
    // Starting from 0%: jump straight to MIN_SPIN_PWM (32%) or target if >= IMMEDIATE_TEMP (58°C)
    if prev == 0 && target >= MIN_SPIN_PWM {
        return if temp >= IMMEDIATE_TEMP { target } else { MIN_SPIN_PWM };
    }
    // Stopping towards 0%: drop directly to 0 if at or below MIN_SPIN_PWM to avoid stall zone
    if target == 0 && prev <= MIN_SPIN_PWM {
        return 0;
    }

    if target > prev {
        // High load: immediate reaction above IMMEDIATE_TEMP (58°C) for fast cooling
        if temp >= IMMEDIATE_TEMP {
            target
        } else {
            // Low load: smooth acoustic ramp-up
            prev.saturating_add(UP_STEP).min(target)
        }
    } else {
        let next = prev.saturating_sub(DOWN_STEP);
        // Avoid lingering in 1..=31% stall zone when spinning down to 0
        if target == 0 && next < MIN_SPIN_PWM {
            0
        } else {
            next.max(target)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TempSource {
    K10Temp,
    EcFallback,
}

/// Unified GPU target calculation combining base curve and shared heatpipe CPU assist
fn calculate_gpu_target(
    tg: Option<i32>,
    cpu_t: Option<i32>,
    target_cpu: Option<u8>,
    gpu_running: bool,
) -> u8 {
    let base_gpu = calculate_curve_pct(GPU_CURVE, tg, gpu_running);
    let cpu_assist = match (cpu_t, target_cpu) {
        (Some(c_t), Some(c_target)) if c_t >= 68 => {
            (c_target as u16 * 3 / 4).max(MIN_SPIN_PWM as u16) as u8
        }
        (Some(c_t), _) if gpu_running && c_t >= (68 - STOP_HYSTERESIS) => MIN_SPIN_PWM,
        _ => 0,
    };
    base_gpu.max(cpu_assist)
}

struct HardwareController {
    ec_file: Option<File>,
    k10temp_file: Option<File>,
    k10temp_path: Option<PathBuf>,
    pci_status_file: Option<File>,
    boost_by_us: bool,
    cpu_pct: u8,
    gpu_pct: u8,
    cpu_warm_ticks: u32,
    gpu_warm_ticks: u32,
    prev_cpu_temp: Option<i32>,
    prev_cpu_source: Option<TempSource>,
    ec_fail_count: u32,
    last_tick: Option<Duration>,
}

impl HardwareController {
    fn new(is_test: bool) -> io::Result<Self> {
        let mut ctrl = Self {
            ec_file: None,
            k10temp_file: None,
            k10temp_path: None,
            pci_status_file: None,
            boost_by_us: false,
            cpu_pct: 0,
            gpu_pct: 0,
            cpu_warm_ticks: 0,
            gpu_warm_ticks: 0,
            prev_cpu_temp: None,
            prev_cpu_source: None,
            ec_fail_count: 0,
            last_tick: Some(get_boottime()),
        };
        ctrl.init_ec_with_retry(5, 100)?;
        ctrl.init_sensors();

        // Restore boost_by_us if Cooler Boost was activated by daemon before a service restart (daemon mode only)
        let hw_boost = ctrl.is_cooler_boost_hw_active();
        if !is_test {
            ctrl.boost_by_us = Path::new(BOOST_STATE_FILE).exists() && hw_boost;
            if !hw_boost && Path::new(BOOST_STATE_FILE).exists() {
                let _ = fs::remove_file(BOOST_STATE_FILE);
            }
        }

        // Initialize fan percentages based on actual current hardware state.
        // If fan was physically stopped (< MIN_SPIN_PWM), initialize to 0 so
        // the 2-tick anti-hunting debounce is preserved.
        let cpu_reading = ctrl.read_cpu_temp();
        let gpu_susp = ctrl.is_gpu_suspended();
        let tg = ctrl.read_gpu_temp(gpu_susp);

        let mut current_cpu_tbl = [0u8; 7];
        let mut current_gpu_tbl = [0u8; 7];
        let has_cpu_tbl = ctrl.read_ec_bytes_at(REG_CPU_STEP0, &mut current_cpu_tbl).is_ok();
        let has_gpu_tbl = ctrl.read_ec_bytes_at(REG_GPU_STEP0, &mut current_gpu_tbl).is_ok();

        let cpu_spinning = has_cpu_tbl && current_cpu_tbl[0] >= MIN_SPIN_PWM;
        let gpu_spinning = has_gpu_tbl && current_gpu_tbl[0] >= MIN_SPIN_PWM;

        let target_cpu_init = calculate_curve_pct(CPU_CURVE, cpu_reading.map(|(t, _)| t), cpu_spinning);
        let target_gpu_init = calculate_gpu_target(
            tg,
            cpu_reading.map(|(t, _)| t),
            Some(target_cpu_init),
            gpu_spinning,
        );

        ctrl.cpu_pct = if cpu_spinning {
            current_cpu_tbl[0].min(target_cpu_init)
        } else {
            0
        };
        ctrl.gpu_pct = if gpu_spinning {
            current_gpu_tbl[0].min(target_gpu_init)
        } else {
            0
        };
        if let Some((t, src)) = cpu_reading {
            ctrl.prev_cpu_temp = Some(t);
            ctrl.prev_cpu_source = Some(src);
        }

        Ok(ctrl)
    }

    fn init_ec(&mut self) -> io::Result<()> {
        self.init_ec_with_retry(1, 0)
    }

    fn init_ec_with_retry(&mut self, retries: u32, delay_ms: u64) -> io::Result<()> {
        let mut last_err = None;
        for attempt in 1..=retries {
            match OpenOptions::new().read(true).write(true).open(EC_IO) {
                Ok(file) => {
                    let mut fw_buf = [0u8; 12];
                    if let Err(e) = file.read_exact_at(&mut fw_buf, REG_FW) {
                        last_err = Some(e);
                        if attempt < retries {
                            thread::sleep(Duration::from_millis(delay_ms));
                        }
                        continue;
                    }
                    if fw_buf != *EXPECTED_FW {
                        let found = String::from_utf8_lossy(&fw_buf);
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Unexpected EC firmware '{}', expected '{}'; refusing to write EC RAM",
                                found,
                                String::from_utf8_lossy(EXPECTED_FW)
                            ),
                        ));
                    }
                    self.ec_file = Some(file);
                    return Ok(());
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < retries {
                        thread::sleep(Duration::from_millis(delay_ms));
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to open EC_IO after retries")))
    }

    fn init_sensors(&mut self) {
        if let Some(path) = find_k10temp_path()
            && let Ok(file) = File::open(&path)
        {
            self.k10temp_file = Some(file);
            self.k10temp_path = Some(path);
        }
        if let Ok(file) = File::open(GPU_PCI_STATUS) {
            self.pci_status_file = Some(file);
        }
    }

    /// Read an exact slice of bytes at a specified EC offset without dumping the entire 256 bytes
    fn read_ec_bytes_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if let Some(ref file) = self.ec_file {
            match file.read_exact_at(buf, offset) {
                Ok(()) => return Ok(()),
                Err(_) => {
                    self.ec_file = None;
                }
            }
        }
        // Reopen on error or after suspend/resume
        self.init_ec()?;
        let file = self.ec_file.as_ref().unwrap();
        file.read_exact_at(buf, offset)?;
        Ok(())
    }

    /// Read a single byte from a specified EC offset
    fn read_ec_single_byte(&mut self, offset: u64) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.read_ec_bytes_at(offset, &mut buf)?;
        Ok(buf[0])
    }

    /// Write all 7 steps of the fan curve table (steps 0..=6)
    fn write_ec_table(&mut self, base_offset: u64, val: u8) -> io::Result<()> {
        let buf = [val; 7]; // Fills steps 0..=6 so any step evaluated by EC gets target PWM
        if let Some(ref file) = self.ec_file {
            match file.write_all_at(&buf, base_offset) {
                Ok(()) => return Ok(()),
                Err(_) => {
                    self.ec_file = None;
                }
            }
        }
        self.init_ec()?;
        let file = self.ec_file.as_ref().unwrap();
        file.write_all_at(&buf, base_offset)?;
        Ok(())
    }

    /// Write a single byte directly to an EC RAM register
    fn write_ec_single_byte(&mut self, offset: u64, val: u8) -> io::Result<()> {
        let buf = [val];
        if let Some(ref file) = self.ec_file {
            match file.write_all_at(&buf, offset) {
                Ok(()) => return Ok(()),
                Err(_) => {
                    self.ec_file = None;
                }
            }
        }
        self.init_ec()?;
        let file = self.ec_file.as_ref().unwrap();
        file.write_all_at(&buf, offset)?;
        Ok(())
    }

    /// Read high-precision CPU silicon temperature from k10temp (Tctl), fallback to EC socket 0x68
    fn read_cpu_temp(&mut self) -> Option<(i32, TempSource)> {
        let mut buf = [0u8; 16];
        if let Some(ref file) = self.k10temp_file {
            if let Ok(n) = file.read_at(&mut buf, 0)
                && n > 0
                && let Ok(s) = std::str::from_utf8(&buf[..n])
                && let Ok(milli) = s.trim().parse::<i32>()
            {
                let t = milli / 1000;
                if (1..125).contains(&t) {
                    return Some((t, TempSource::K10Temp));
                }
            }
            self.k10temp_file = None;
        }

        // Try to re-find k10temp if previously lost
        if let Some(path) = find_k10temp_path()
            && let Ok(file) = File::open(&path)
            && let Ok(n) = file.read_at(&mut buf, 0)
            && n > 0
            && let Ok(s) = std::str::from_utf8(&buf[..n])
            && let Ok(milli) = s.trim().parse::<i32>()
        {
            let t = milli / 1000;
            if (1..125).contains(&t) {
                self.k10temp_file = Some(file);
                self.k10temp_path = Some(path);
                return Some((t, TempSource::K10Temp));
            }
        }

        // Fallback to EC 0x68 socket temperature (targeted read of 1 byte)
        if let Ok(b) = self.read_ec_single_byte(REG_CPU_TEMP as u64)
            && b > 0
            && b < 120
        {
            return Some((b as i32, TempSource::EcFallback));
        }

        None
    }

    /// Check if discrete GPU (RTX 4050) is suspended in D3cold (0W power)
    fn is_gpu_suspended(&mut self) -> bool {
        let mut buf = [0u8; 32];
        if let Some(ref file) = self.pci_status_file {
            if let Ok(n) = file.read_at(&mut buf, 0)
                && n > 0
            {
                let s = std::str::from_utf8(&buf[..n]).unwrap_or("");
                return s.trim() == "suspended";
            }
            self.pci_status_file = None;
        }
        if let Ok(file) = File::open(GPU_PCI_STATUS)
            && let Ok(n) = file.read_at(&mut buf, 0)
            && n > 0
        {
            let s = std::str::from_utf8(&buf[..n]).unwrap_or("");
            let suspended = s.trim() == "suspended";
            self.pci_status_file = Some(file);
            return suspended;
        }
        false
    }

    /// Read GPU temperature only if awake; never wake sleeping GPU
    fn read_gpu_temp(&mut self, is_suspended: bool) -> Option<i32> {
        if is_suspended {
            return None;
        }
        if let Ok(b) = self.read_ec_single_byte(REG_GPU_TEMP as u64)
            && b > 0
            && b < 120
        {
            return Some(b as i32);
        }
        None
    }

    /// Check if Cooler Boost hardware overdrive is currently active (via sysfs or direct EC register 0x98)
    fn is_cooler_boost_hw_active(&mut self) -> bool {
        let path = Path::new(MSI_EC).join("cooler_boost");
        if let Ok(s) = fs::read_to_string(path) {
            return s.trim() == "on";
        }
        if let Ok(b) = self.read_ec_single_byte(REG_COOLER_BOOST) {
            return (b & COOLER_BOOST_BIT) != 0;
        }
        false
    }

    fn set_cooler_boost(&mut self, on: bool) -> bool {
        let path = Path::new(MSI_EC).join("cooler_boost");
        let val = if on { "on\n" } else { "off\n" };
        if fs::write(&path, val).is_ok() {
            return true;
        }
        eprintln!("[msi-fan-regulator] sysfs cooler_boost failed, falling back to EC 0x98");
        match self.read_ec_single_byte(REG_COOLER_BOOST) {
            Ok(current) => {
                let next = if on { current | COOLER_BOOST_BIT } else { current & !COOLER_BOOST_BIT };
                if let Err(we) = self.write_ec_single_byte(REG_COOLER_BOOST, next) {
                    eprintln!("[msi-fan-regulator] Failed to write EC 0x98: {}", we);
                    false
                } else {
                    true
                }
            }
            Err(re) => {
                eprintln!("[msi-fan-regulator] Cannot read EC 0x98 for safe bitmask update: {}", re);
                false
            }
        }
    }

    fn emergency(&mut self, reason: &str) {
        let hw_on = self.is_cooler_boost_hw_active();
        if !hw_on {
            eprintln!("[msi-fan-regulator] EMERGENCY ({}): Cooler Boost ON", reason);
            if self.set_cooler_boost(true) {
                self.boost_by_us = true;
                let _ = fs::write(BOOST_STATE_FILE, b"1");
            }
        } else if !self.boost_by_us {
            eprintln!("[msi-fan-regulator] EMERGENCY ({}): Cooler Boost already ON (hardware/user)", reason);
        }
    }

    fn release_emergency(&mut self) {
        if self.boost_by_us {
            println!("[msi-fan-regulator] Temperatures normalized: Cooler Boost OFF");
            if self.set_cooler_boost(false) {
                self.boost_by_us = false;
                let _ = fs::remove_file(BOOST_STATE_FILE);
            }
        }
    }

    fn handle_io_error(&mut self, reason: &str) {
        self.ec_fail_count += 1;
        if self.ec_fail_count <= 3 || self.ec_fail_count.is_multiple_of(60) {
            eprintln!(
                "[msi-fan-regulator] I/O warning ({}/3): {}",
                self.ec_fail_count, reason
            );
        }
        if self.ec_fail_count >= 3 && !self.boost_by_us {
            self.emergency(&format!("Persistent I/O failure ({}): {}", self.ec_fail_count, reason));
        }
    }

    fn tick(&mut self) {
        // Track elapsed boottime across suspend/resume or heavy OS stalls (>3s)
        let now = get_boottime();
        let resumed_from_sleep = self
            .last_tick
            .is_some_and(|t| now.saturating_sub(t) > Duration::from_secs(3));
        self.last_tick = Some(now);

        // 1. Ensure fan mode is strictly set to advanced (0x8D)
        let mode = match self.read_ec_single_byte(REG_FAN_MODE as u64) {
            Ok(m) => m,
            Err(e) => {
                self.handle_io_error(&format!("Reading fan mode failed: {}", e));
                return;
            }
        };

        if mode != FAN_MODE_ADVANCED {
            eprintln!(
                "[msi-fan-regulator] Fan mode is 0x{:02x} (not advanced); forcing advanced (0x8D)",
                mode
            );
            if let Err(e) = self.write_ec_single_byte(REG_FAN_MODE as u64, FAN_MODE_ADVANCED) {
                self.handle_io_error(&format!("Writing fan mode failed: {}", e));
                return;
            }
        }

        // 2. Read temperatures
        let cpu_reading = self.read_cpu_temp();
        let gpu_susp = self.is_gpu_suspended();
        let tg = self.read_gpu_temp(gpu_susp);

        let Some((cpu_t, cpu_source)) = cpu_reading else {
            self.handle_io_error("CPU temperature unavailable");
            return;
        };

        // Single sensor pass on resume: reset derivative history. If chips are cold (< IMMEDIATE_TEMP),
        // reset PWM to 0 so physically stopped fans are protected by the 2-tick anti-hunting debounce.
        // If chips are hot, cap speeds with .min() to eliminate slow 18s spool-down while providing instant cooling.
        if resumed_from_sleep {
            eprintln!("[msi-fan-regulator] Time jump detected (suspend/resume), re-syncing state");
            self.prev_cpu_temp = None;
            self.prev_cpu_source = None;
            self.cpu_warm_ticks = 0;
            self.gpu_warm_ticks = 0;

            if cpu_t < IMMEDIATE_TEMP {
                self.cpu_pct = 0;
            } else {
                self.cpu_pct = self.cpu_pct.min(calculate_curve_pct(CPU_CURVE, Some(cpu_t), false));
            }

            let target_gpu_wake = calculate_gpu_target(tg, Some(cpu_t), Some(self.cpu_pct), false);
            if tg.is_none_or(|g| g < IMMEDIATE_TEMP) && cpu_t < 68 {
                self.gpu_pct = 0;
            } else {
                self.gpu_pct = self.gpu_pct.min(target_gpu_wake);
            }
        }

        // 3. Emergency overdrive protection
        // Only release emergency if there are NO active I/O errors (ec_fail_count == 0)
        // and only if emergency boost was engaged by this daemon (self.boost_by_us).
        if cpu_t >= EMERGENCY_CPU || tg.is_some_and(|g| g >= EMERGENCY_GPU) {
            let g_str = tg.map_or("N/A".to_string(), |g| format!("{}°C", g));
            self.emergency(&format!("CPU {}°C, GPU {}", cpu_t, g_str));
        } else if self.ec_fail_count == 0
            && self.boost_by_us
            && cpu_t < RELEASE_CPU
            && tg.is_none_or(|g| g < RELEASE_GPU)
        {
            self.release_emergency();
        }

        // 4. Derivative action (dT/dt anticipation):
        // Only compute delta if temperature source did not switch between k10temp and EC fallback
        // to prevent false derivative spikes on fallback transitions.
        let delta = if self.prev_cpu_source == Some(cpu_source) {
            self.prev_cpu_temp.map(|p| cpu_t - p).unwrap_or(0)
        } else {
            0
        };

        // Preserve baseline temperature on tick 1 of debounce (cpu_warm_ticks == 0)
        // so delta is not lost between tick 1 and tick 2.
        let update_prev_temp = !(self.cpu_pct == 0 && (46..IMMEDIATE_TEMP).contains(&cpu_t) && self.cpu_warm_ticks == 0);
        if update_prev_temp {
            self.prev_cpu_temp = Some(cpu_t);
            self.prev_cpu_source = Some(cpu_source);
        }

        let effective_cpu_t = if delta >= 5 {
            (cpu_t + (delta * 2).min(20)).min(100)
        } else {
            cpu_t
        };

        // 5. Anti-hunting debounce for starting from cold 0 RPM:
        // Debounce is active up to IMMEDIATE_TEMP (58°C) so transient bursts don't spin up fan.
        // Once 2 consecutive warm ticks pass, effective_cpu_t provides anticipatory cooling.
        let target_cpu = if self.cpu_pct == 0 {
            if cpu_t >= IMMEDIATE_TEMP {
                calculate_curve_pct(CPU_CURVE, Some(effective_cpu_t), false)
            } else if cpu_t >= 46 {
                self.cpu_warm_ticks += 1;
                if self.cpu_warm_ticks >= 2 {
                    calculate_curve_pct(CPU_CURVE, Some(effective_cpu_t), false)
                } else {
                    0
                }
            } else {
                self.cpu_warm_ticks = 0;
                0
            }
        } else {
            self.cpu_warm_ticks = 0;
            calculate_curve_pct(CPU_CURVE, Some(effective_cpu_t), true)
        };

        // GPU start debounce:
        let target_gpu = if let Some(g_t) = tg {
            if self.gpu_pct == 0 {
                if g_t >= IMMEDIATE_TEMP {
                    calculate_curve_pct(GPU_CURVE, Some(g_t), false)
                } else if g_t >= 46 {
                    self.gpu_warm_ticks += 1;
                    if self.gpu_warm_ticks >= 2 {
                        calculate_curve_pct(GPU_CURVE, Some(g_t), false)
                    } else {
                        0
                    }
                } else {
                    self.gpu_warm_ticks = 0;
                    0
                }
            } else {
                self.gpu_warm_ticks = 0;
                calculate_curve_pct(GPU_CURVE, Some(g_t), true)
            }
        } else {
            self.gpu_warm_ticks = 0;
            0
        };

        // Cooler Boost 5 shared heatpipes assist:
        // When CPU is under sustained high load (>= 68°C), engage GPU fan at 75% of CPU speed
        // to dissipate heat through the second heatsink even if RTX 4050 is suspended in D3cold.
        let cpu_assist_gpu = if cpu_t >= 68 {
            (target_cpu as u16 * 3 / 4).max(MIN_SPIN_PWM as u16) as u8
        } else if self.gpu_pct >= MIN_SPIN_PWM && cpu_t >= (68 - STOP_HYSTERESIS) {
            MIN_SPIN_PWM
        } else {
            0
        };
        let target_gpu = target_gpu.max(cpu_assist_gpu);

        // 6. Smooth acoustic transitions without stall-zone operation:
        // Forward effective_cpu_t to driving temp when breaking debounce with high dT/dt
        // so derivative anticipation is not clamped down to MIN_SPIN_PWM.
        let cpu_driving_temp = if self.cpu_pct == 0 && self.cpu_warm_ticks >= 2 {
            effective_cpu_t
        } else {
            cpu_t
        };
        let gpu_driving_temp = if cpu_assist_gpu > 0 {
            tg.unwrap_or(0).max(cpu_t)
        } else {
            tg.unwrap_or(0)
        };
        self.cpu_pct = smooth_pct(self.cpu_pct, target_cpu, cpu_driving_temp);
        self.gpu_pct = smooth_pct(self.gpu_pct, target_gpu, gpu_driving_temp);

        // 7. Verify all 7 steps of EC fan tables (0x72..=0x78 CPU, 0x8A..=0x90 GPU)
        let mut cpu_table = [0u8; 7];
        if let Err(e) = self.read_ec_bytes_at(REG_CPU_STEP0, &mut cpu_table) {
            self.handle_io_error(&format!("Reading CPU fan table failed: {}", e));
            return;
        }
        if cpu_table != [self.cpu_pct; 7]
            && let Err(e) = self.write_ec_table(REG_CPU_STEP0, self.cpu_pct)
        {
            self.handle_io_error(&format!("EC CPU table write failed: {}", e));
            return;
        }

        let mut gpu_table = [0u8; 7];
        if let Err(e) = self.read_ec_bytes_at(REG_GPU_STEP0, &mut gpu_table) {
            self.handle_io_error(&format!("Reading GPU fan table failed: {}", e));
            return;
        }
        if gpu_table != [self.gpu_pct; 7]
            && let Err(e) = self.write_ec_table(REG_GPU_STEP0, self.gpu_pct)
        {
            self.handle_io_error(&format!("EC GPU table write failed: {}", e));
            return;
        }

        // Reset I/O error count on complete successful cycle
        self.ec_fail_count = 0;
    }

    fn run(&mut self) {
        setup_signals();
        println!(
            "[msi-fan-regulator] Daemon started (CPU sensor: {}, Safe PWM: {}%)",
            self.k10temp_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "EC 0x68".to_string()),
            SAFE_PCT
        );

        let mut last_log = get_boottime();
        while RUNNING.load(Ordering::SeqCst) {
            self.tick();
            let now = get_boottime();
            if now.saturating_sub(last_log) >= Duration::from_secs(60) {
                println!(
                    "[msi-fan-regulator] cpu_pwm={}%, gpu_pwm={}%, cooler_boost={}",
                    self.cpu_pct, self.gpu_pct, self.boost_by_us
                );
                last_log = now;
            }
            thread::sleep(Duration::from_secs(1));
        }

        // Graceful exit: ensure advanced mode is active and set safe fan speeds
        let safe_cpu = SAFE_PCT.max(self.cpu_pct);
        let safe_gpu = SAFE_PCT.max(self.gpu_pct);
        println!(
            "[msi-fan-regulator] Shutting down, writing safe fan speeds (CPU {}%, GPU {}%)...",
            safe_cpu, safe_gpu
        );
        let _ = self.write_ec_single_byte(REG_FAN_MODE as u64, FAN_MODE_ADVANCED);
        let write_cpu = self.write_ec_table(REG_CPU_STEP0, safe_cpu);
        let write_gpu = self.write_ec_table(REG_GPU_STEP0, safe_gpu);

        // Fail-safe check: detect sensor loss, EC write error, or critical temperatures
        let cpu_temp_opt = self.read_cpu_temp();
        let gpu_susp = self.is_gpu_suspended();
        let gpu_temp_opt = self.read_gpu_temp(gpu_susp);

        let io_failed = write_cpu.is_err() || write_gpu.is_err();
        let sensor_lost = cpu_temp_opt.is_none();
        let critical_overheat = cpu_temp_opt.is_some_and(|(t, _)| t >= EMERGENCY_CPU)
            || gpu_temp_opt.is_some_and(|t| t >= EMERGENCY_GPU);
        let retain_boost = self.boost_by_us
            && (cpu_temp_opt.is_none_or(|(t, _)| t >= RELEASE_CPU)
                || gpu_temp_opt.is_some_and(|t| t >= RELEASE_GPU));

        if io_failed || sensor_lost || critical_overheat || retain_boost {
            let reason = if io_failed {
                "EC table write failed on shutdown"
            } else if sensor_lost {
                "CPU sensor lost on shutdown"
            } else if critical_overheat {
                "Critical temperature on shutdown (>=95°C CPU / >=90°C GPU)"
            } else {
                "Retaining active emergency boost until cooled below 85°C/75°C"
            };
            eprintln!("[msi-fan-regulator] {}: keeping Cooler Boost ON", reason);
            if self.set_cooler_boost(true) {
                self.boost_by_us = true;
                let _ = fs::write(BOOST_STATE_FILE, b"1");
            }
        } else {
            self.release_emergency();
            println!("[msi-fan-regulator] Safe speeds applied. Bye.");
        }
    }

    fn test_status(&mut self) {
        let mut fw_buf = [0u8; 12];
        let fw_str = match self.read_ec_bytes_at(REG_FW, &mut fw_buf) {
            Ok(()) => String::from_utf8_lossy(&fw_buf).into_owned(),
            Err(e) => format!("Error reading FW: {}", e),
        };

        let mode_byte = self.read_ec_single_byte(REG_FAN_MODE as u64).unwrap_or(0);
        let mode_str = match mode_byte {
            0x8D => "advanced",
            0x0D => "auto",
            0x1D => "silent",
            0x4D => "basic",
            _ => "unknown",
        };

        let cpu_reading = self.read_cpu_temp();
        let ec_cpu_temp = self.read_ec_single_byte(REG_CPU_TEMP as u64).unwrap_or(0);

        let gpu_susp = self.is_gpu_suspended();
        let tg = self.read_gpu_temp(gpu_susp);
        let ec_gpu_temp = if !gpu_susp {
            self.read_ec_single_byte(REG_GPU_TEMP as u64).unwrap_or(0)
        } else {
            0
        };

        let mut cpu_tbl = [0u8; 7];
        let mut gpu_tbl = [0u8; 7];
        let _ = self.read_ec_bytes_at(REG_CPU_STEP0, &mut cpu_tbl);
        let _ = self.read_ec_bytes_at(REG_GPU_STEP0, &mut gpu_tbl);

        let cpu_running = cpu_tbl[0] >= MIN_SPIN_PWM;
        let gpu_running = gpu_tbl[0] >= MIN_SPIN_PWM;

        let tc = cpu_reading.map(|(t, _)| t);
        let cpu_target = calculate_curve_pct(CPU_CURVE, tc, cpu_running);
        let gpu_target = calculate_gpu_target(tg, tc, Some(cpu_target), gpu_running);
        let cb_active = self.is_cooler_boost_hw_active();

        println!("=== MSI Bravo 15 C7VE Fan Telemetry & Status ===");
        println!("EC Firmware    : {}", fw_str);
        println!("Fan Mode (0xD4): 0x{:02x} ({})", mode_byte, mode_str);
        println!("Cooler Boost   : {}", if cb_active { "ON (overdrive)" } else { "OFF" });
        println!("CPU Temp       : {} ({}), EC 0x68: {}°C",
            tc.map_or("N/A".to_string(), |t| format!("{}°C", t)),
            match cpu_reading.map(|(_, s)| s) {
                Some(TempSource::K10Temp) => "silicon k10temp Tctl",
                Some(TempSource::EcFallback) => "socket EC fallback",
                None => "unavailable",
            },
            ec_cpu_temp
        );
        println!("GPU Temp       : {} (PCI status: {}), EC 0x80: {}°C",
            tg.map_or("N/A (asleep)".to_string(), |t| format!("{}°C", t)),
            if gpu_susp { "suspended (D3cold)" } else { "active" },
            ec_gpu_temp
        );
        println!("Current EC PWM : CPU step0: {}% (table: {:?}), GPU step0: {}% (table: {:?})",
            cpu_tbl[0], cpu_tbl, gpu_tbl[0], gpu_tbl
        );
        println!("Computed Target: CPU: {}% (running: {}), GPU: {}% (running: {})",
            cpu_target, cpu_running, gpu_target, gpu_running
        );
        println!("Safety Limits  : Emergency Cooler Boost at CPU >= {}°C, GPU >= {}°C", EMERGENCY_CPU, EMERGENCY_GPU);
    }
}

fn print_help() {
    println!("Usage: msi-fan-regulator [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --status, --test, -t   Read sensors and print status without modifying EC");
    println!("  --help, -h             Print this help message");
    println!();
    println!("Running without arguments starts the regulator daemon (requires root).");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    let is_test = args.iter().any(|a| a == "--status" || a == "--test" || a == "-t");

    if inerium_tools::sys::posix_getuid() != 0 {
        eprintln!("Error: msi-fan-regulator requires root privileges to access debugfs EC RAM. Run with sudo (e.g. 'sudo msi-fan-regulator --status').");
        std::process::exit(1);
    }

    let _lock = if !is_test {
        Some(match SingleInstanceLock::acquire() {
            Ok(lock) => lock,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        })
    } else {
        None
    };

    let mut ctrl = match HardwareController::new(is_test) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Fatal error initializing HardwareController: {}", e);
            std::process::exit(1);
        }
    };

    if is_test {
        ctrl.test_status();
    } else {
        ctrl.run();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smooth_pct_stall_prevention() {
        // Cold start below IMMEDIATE_TEMP jumps straight to MIN_SPIN_PWM (32)
        assert_eq!(smooth_pct(0, 32, 48), 32);
        assert_eq!(smooth_pct(0, 40, 48), 32);

        // Cold start above IMMEDIATE_TEMP (58°C) jumps directly to target
        assert_eq!(smooth_pct(0, 68, 65), 68);

        // Smooth acceleration from 32 by UP_STEP (10)
        assert_eq!(smooth_pct(32, 52, 50), 42);
        assert_eq!(smooth_pct(42, 52, 50), 52);

        // Smooth deceleration by DOWN_STEP (3)
        assert_eq!(smooth_pct(52, 32, 45), 49);
        assert_eq!(smooth_pct(35, 32, 45), 32);

        // Direct drop to 0% when target is 0 and at or below MIN_SPIN_PWM
        assert_eq!(smooth_pct(32, 0, 40), 0);
        assert_eq!(smooth_pct(20, 0, 40), 0);

        // Deceleration towards 0 drops to 0 instead of entering stall zone (< 32%)
        assert_eq!(smooth_pct(34, 0, 40), 0);
        assert_eq!(smooth_pct(38, 0, 40), 35);
    }

    #[test]
    fn test_calculate_curve_pct() {
        // Below 42°C (46 - STOP_HYSTERESIS) fan is 0%
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(40), false), 0);
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(40), true), 0);

        // Hysteresis window: between 42°C and 45°C
        // Stopped fan stays stopped (0%)
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(44), false), 0);
        // Spinning fan stays running at 32%
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(44), true), 32);

        // Exact curve points
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(46), false), 32);
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(52), false), 40);
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(84), false), 100);
        assert_eq!(calculate_curve_pct(CPU_CURVE, Some(90), false), 100);
    }

    #[test]
    fn test_sawtooth_prevention_acoustic_ramp() {
        // When real cpu_t is 53°C (< IMMEDIATE_TEMP 58°C), smooth_pct must ramp up
        // by UP_STEP (10) instead of jumping straight to 64%, preventing acoustic saw-tooth spikes.
        let prev = 32;
        let target = 64;
        let cpu_t = 53;
        assert_eq!(smooth_pct(prev, target, cpu_t), 42);

        // When physical cpu_t is genuinely hot (>= 58°C), ramp is immediate
        let hot_cpu_t = 60;
        assert_eq!(smooth_pct(prev, target, hot_cpu_t), 64);
    }

    #[test]
    fn test_cpu_assist_gpu_logic() {
        // Function replicating the assist formula
        let calc_assist = |cpu_t: i32, target_cpu: u8, gpu_pct: u8| -> u8 {
            if cpu_t >= 68 {
                (target_cpu as u16 * 3 / 4).max(MIN_SPIN_PWM as u16) as u8
            } else if gpu_pct >= MIN_SPIN_PWM && cpu_t >= (68 - STOP_HYSTERESIS) {
                MIN_SPIN_PWM
            } else {
                0
            }
        };

        // CPU at 72°C (target 82%): GPU assist engages at 61%
        assert_eq!(calc_assist(72, 82, 0), 61);

        // CPU at 84°C (target 100%): GPU assist engages at 75%
        assert_eq!(calc_assist(84, 100, 61), 75);

        // CPU dropping to 65°C with GPU spinning: holds MIN_SPIN_PWM (32%)
        assert_eq!(calc_assist(65, 68, 61), 32);

        // CPU dropping below 64°C (68 - 4): GPU assist drops to 0%
        assert_eq!(calc_assist(63, 60, 32), 0);
    }

    #[test]
    fn test_boottime_clock() {
        let t1 = get_boottime();
        assert!(t1.as_secs() > 0);
    }

    #[test]
    fn test_gpu_driving_temp_immediate_assist() {
        // When CPU is hot (80°C) and RTX 4050 is in D3cold (tg = None),
        // driving temp must take cpu_t so smooth_pct immediately hits target
        // without 5-second UP_STEP latency.
        let target_gpu = 75;
        let cpu_t = 80;
        let tg: Option<i32> = None;
        let cpu_assist_gpu = 75;

        let gpu_driving_temp = if cpu_assist_gpu > 0 {
            tg.unwrap_or(0).max(cpu_t)
        } else {
            tg.unwrap_or(0)
        };
        assert_eq!(gpu_driving_temp, 80);
        // Starting cold at 0 with 80°C >= IMMEDIATE_TEMP jumps straight to target
        assert_eq!(smooth_pct(0, target_gpu, gpu_driving_temp), 75);
    }

    #[test]
    fn test_resume_min_capping() {
        // Cold resume with 1-tick 47°C spike:
        // Prior speed was 0%, curve for 47°C is 34%.
        // 0.min(34) must remain 0% to protect 2-tick cold-start debounce!
        let cold_prev_cpu = 0u8;
        let wake_target = calculate_curve_pct(CPU_CURVE, Some(47), false);
        assert_eq!(cold_prev_cpu.min(wake_target), 0);

        // Hot-to-cold resume:
        // Went to sleep at 85% during gaming, woke up cold at 35°C.
        // Curve for 35°C is 0%.
        // 85.min(0) must immediately drop to 0% without 18s spool-down lag!
        let hot_prev_cpu = 85u8;
        let cold_target = calculate_curve_pct(CPU_CURVE, Some(35), false);
        assert_eq!(hot_prev_cpu.min(cold_target), 0);
    }

    #[test]
    fn test_calculate_gpu_target() {
        // 1. GPU suspended (D3cold, tg = None) and CPU cool (50°C): 0%
        assert_eq!(calculate_gpu_target(None, Some(50), Some(36), false), 0);

        // 2. GPU suspended, but CPU hot at 72°C (target_cpu = 82%):
        // Shared heatpipe assist engages: 82 * 3 / 4 = 61%
        assert_eq!(calculate_gpu_target(None, Some(72), Some(82), false), 61);

        // 3. GPU suspended, CPU very hot at 84°C (target_cpu = 100%):
        // Assist engages at 100 * 3 / 4 = 75%
        assert_eq!(calculate_gpu_target(None, Some(84), Some(100), false), 75);

        // 4. GPU active at 60°C (GPU curve yields 56%), CPU hot at 84°C (assist 75%):
        // Target must be max(56, 75) = 75%
        assert_eq!(calculate_gpu_target(Some(60), Some(84), Some(100), true), 75);

        // 5. GPU active at 74°C (GPU curve yields 90%), CPU at 70°C (assist 60%):
        // Target must be max(90, 60) = 90%
        assert_eq!(calculate_gpu_target(Some(74), Some(70), Some(80), true), 90);

        // 6. CPU dropping to 65°C with GPU running: hysteresis holds MIN_SPIN_PWM (32%)
        assert_eq!(calculate_gpu_target(None, Some(65), Some(68), true), 32);

        // 7. CPU dropping below 64°C (68 - 4): assist stops completely (0%)
        assert_eq!(calculate_gpu_target(None, Some(63), Some(60), true), 0);
    }

    #[test]
    fn test_debounce_baseline_preservation_and_driving_temp() {
        // Simulate cold idle: 44°C
        let mut prev_temp = Some(44);
        let mut cpu_pct = 0u8;
        let mut warm_ticks = 0u32;

        // Tick 1: rapid thermal spike from 44°C to 55°C
        let cpu_t1 = 55;
        let delta1 = cpu_t1 - prev_temp.unwrap(); // +11°C
        assert_eq!(delta1, 11);

        let update_prev1 = !(cpu_pct == 0 && (46..IMMEDIATE_TEMP).contains(&cpu_t1) && warm_ticks == 0);
        assert!(!update_prev1, "Baseline must NOT update on tick 1 of debounce");
        if update_prev1 {
            prev_temp = Some(cpu_t1);
        }
        warm_ticks += 1; // Debounce tick 1: fan stays 0%
        assert_eq!(prev_temp, Some(44));

        // Tick 2: continues hot at 56°C
        let cpu_t2 = 56;
        let delta2 = cpu_t2 - prev_temp.unwrap(); // 56 - 44 = 12°C (preserved from original baseline!)
        assert_eq!(delta2, 12);

        let update_prev2 = !(cpu_pct == 0 && (46..IMMEDIATE_TEMP).contains(&cpu_t2) && warm_ticks == 0);
        assert!(update_prev2, "Baseline MUST update on tick 2 of debounce");
        if update_prev2 {
            prev_temp = Some(cpu_t2);
        }
        assert_eq!(prev_temp, Some(56));
        warm_ticks += 1; // Debounce tick 2 (>= 2): fan breaks out of debounce!
        assert_eq!(warm_ticks, 2);

        let effective_cpu_t = (cpu_t2 + (delta2 * 2).min(20)).min(100); // 56 + 20 = 76°C
        let target_cpu = calculate_curve_pct(CPU_CURVE, Some(effective_cpu_t), false); // 88%
        assert_eq!(target_cpu, 88);

        // Driving temp forwards effective_cpu_t when breaking debounce with high dT/dt
        let cpu_driving_temp = if cpu_pct == 0 && warm_ticks >= 2 {
            effective_cpu_t
        } else {
            cpu_t2
        };
        assert_eq!(cpu_driving_temp, 76);
        // Because driving temp (76°C) >= IMMEDIATE_TEMP (58°C), smooth_pct jumps directly to target_cpu (88%)
        cpu_pct = smooth_pct(cpu_pct, target_cpu, cpu_driving_temp);
        assert_eq!(cpu_pct, 88);
    }
}

