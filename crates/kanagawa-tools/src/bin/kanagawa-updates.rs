//! Kanagawa Dragon — Native Rust Updates Checker & Cache Manager
//! - Checks official repos via `checkupdates` and AUR via `paru -Qua`
//! - Fast cache in /run/user/<uid>/kanagawa-updates-cache.json
//! - Instant read (< 1ms) for Center of Control and scripts

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use kanagawa_tools::ipc::PidRegistry;
use kanagawa_tools::sys::posix::{posix_getuid, SIGUSR1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatesCache {
    pub total: usize,
    pub repo: usize,
    pub aur: usize,
    pub timestamp: u64,
    pub sample_packages: Vec<String>,
}

pub fn get_cache_path() -> PathBuf {
    let uid = posix_getuid();
    PathBuf::from(format!("/run/user/{}/kanagawa-updates-cache.json", uid))
}

pub fn read_cache() -> Option<UpdatesCache> {
    let path = get_cache_path();
    if let Ok(data) = fs::read_to_string(&path)
        && let Ok(cache) = serde_json::from_str::<UpdatesCache>(&data)
    {
        return Some(cache);
    }
    None
}

pub fn perform_check() -> UpdatesCache {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut repo_pkgs = Vec::new();
    if let Ok(output) = Command::new("checkupdates").output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let pkg = trimmed.split_whitespace().next().unwrap_or(trimmed);
                repo_pkgs.push(pkg.to_string());
            }
        }
    }

    let mut aur_pkgs = Vec::new();
    if let Ok(output) = Command::new("paru").args(["-Qua"]).output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let pkg = trimmed.split_whitespace().next().unwrap_or(trimmed);
                aur_pkgs.push(pkg.to_string());
            }
        }
    }

    let repo_count = repo_pkgs.len();
    let aur_count = aur_pkgs.len();
    let total = repo_count + aur_count;

    let mut sample = Vec::new();
    for p in repo_pkgs.iter().take(5) {
        sample.push(p.clone());
    }
    for p in aur_pkgs.iter().take(3) {
        sample.push(format!("{} (aur)", p));
    }

    let cache = UpdatesCache {
        total,
        repo: repo_count,
        aur: aur_count,
        timestamp: now,
        sample_packages: sample,
    };

    let cache_path = get_cache_path();
    if let Ok(serialized) = serde_json::to_string(&cache) {
        let _ = fs::write(&cache_path, serialized);
    }

    PidRegistry::new("updates").notify_all(SIGUSR1);

    cache
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--check" || a == "-c") {
        let res = perform_check();
        println!("{}", res.total);
        return;
    }

    if args.iter().any(|a| a == "--json") {
        let cache = read_cache().unwrap_or_else(|| {
            // If cache doesn't exist, trigger check in background or synchronously
            perform_check()
        });
        if let Ok(s) = serde_json::to_string_pretty(&cache) {
            println!("{}", s);
        }
        return;
    }

    // Default: read cache or print 0 if missing (non-blocking)
    if let Some(c) = read_cache() {
        println!("{}", c.total);
    } else {
        println!("0");
        // Start background check so next time it's available
        let _ = Command::new(&args[0]).arg("--check").spawn();
    }
}
