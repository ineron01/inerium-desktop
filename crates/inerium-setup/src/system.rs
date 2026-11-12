use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::model::{
    FirstLoginStep, HyprMonitor, IdleConfig, InputOverrides, MonitorConfig, SetupPaths, StepStatus,
    atomic_write, generate_hypridle_conf, generate_hypridle_dropin, generate_inputs_lua,
    generate_monitors_lua, list_executable_steps, mark_step_done, parse_hypr_monitors_json,
    step_done_marker,
};

pub fn query_hypr_monitors() -> Result<Vec<HyprMonitor>, String> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .map_err(|e| format!("Failed executing hyprctl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "hyprctl monitors -j failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    parse_hypr_monitors_json(&raw)
}

pub fn hyprctl_reload() -> Result<(), String> {
    let output = Command::new("hyprctl")
        .arg("reload")
        .output()
        .map_err(|e| format!("Failed executing hyprctl reload: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "hyprctl reload failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn validate_lua_syntax(content: &str) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("inerium_setup_check_{}.lua", std::process::id()));
    fs::write(&tmp, content).map_err(|e| format!("Failed writing temp Lua file: {e}"))?;
    let output = Command::new("luac").arg("-p").arg(&tmp).output();
    let _ = fs::remove_file(&tmp);

    let output = output.map_err(|e| format!("Failed executing luac -p: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Lua syntax error: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn is_managed_monitors_overlay(paths: &SetupPaths) -> bool {
    let path = paths.monitors_local_path();
    let is_symlink = fs::symlink_metadata(&path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if is_symlink {
        return true;
    }
    fs::read_to_string(&path)
        .map(|s| s.contains("INERIUM_WORKSPACES_OVERRIDDEN"))
        .unwrap_or(false)
}

pub fn save_and_reload_monitors(
    paths: &SetupPaths,
    monitors: &[MonitorConfig],
) -> Result<(), String> {
    if is_managed_monitors_overlay(paths) {
        return Err(
            "monitors_local.lua управляется оверлеем (симлинк / правила воркспейсов); правка из GUI отключена"
                .to_string(),
        );
    }
    let lua = generate_monitors_lua(monitors);
    validate_lua_syntax(&lua)?;
    atomic_write(&paths.monitors_local_path(), &lua)?;
    hyprctl_reload()
}

pub fn reset_monitors_to_auto(paths: &SetupPaths) -> Result<(), String> {
    if is_managed_monitors_overlay(paths) {
        return Err(
            "monitors_local.lua is managed by an overlay (symlink / workspace rules); GUI reset is disabled"
                .to_string(),
        );
    }
    let path = paths.monitors_local_path();
    if fs::symlink_metadata(&path).is_ok() {
        fs::remove_file(&path).map_err(|e| format!("Failed removing {}: {e}", path.display()))?;
    }
    hyprctl_reload()
}

pub fn apply_wallpaper(path: &Path) -> Result<(), String> {
    let bin = if let Some(home_bin) = std::env::var_os("HOME")
        .map(|h| Path::new(&h).join(".local/bin/set-wallpaper"))
        .filter(|p| p.exists())
    {
        home_bin
    } else {
        Path::new("set-wallpaper").to_path_buf()
    };

    let output = Command::new(bin)
        .arg(path)
        .arg("all")
        .output()
        .map_err(|e| format!("Failed executing set-wallpaper: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "set-wallpaper failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn save_inputs_and_reload(paths: &SetupPaths, cfg: &InputOverrides) -> Result<(), String> {
    let lua = generate_inputs_lua(cfg);
    validate_lua_syntax(&lua)?;
    atomic_write(&paths.inputs_local_path(), &lua)?;
    hyprctl_reload()
}

fn run_systemctl_user(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| format!("Failed executing systemctl --user: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn apply_hypridle(paths: &SetupPaths, cfg: &IdleConfig) -> Result<(), String> {
    let conf_path = paths.hypridle_local_conf_path();
    let dropin_path = paths.hypridle_dropin_path();
    let conf_content = generate_hypridle_conf(cfg);
    atomic_write(&conf_path, &conf_content)?;

    if cfg.enabled {
        let dropin_content = generate_hypridle_dropin(&conf_path);
        atomic_write(&dropin_path, &dropin_content)?;
        run_systemctl_user(&["daemon-reload"])?;
        run_systemctl_user(&["enable", "--now", "hypridle.service"])?;
        run_systemctl_user(&["restart", "hypridle.service"])?;
    } else {
        if fs::symlink_metadata(&dropin_path).is_ok() {
            let _ = fs::remove_file(&dropin_path);
        }
        let _ = run_systemctl_user(&["daemon-reload"]);
        let _ = run_systemctl_user(&["disable", "--now", "hypridle.service"]);
    }
    Ok(())
}

pub fn reset_interface_overrides(paths: &SetupPaths) -> Result<(), String> {
    for p in [
        paths.inputs_local_path(),
        paths.hypridle_local_conf_path(),
        paths.hypridle_dropin_path(),
    ] {
        if fs::symlink_metadata(&p).is_ok() {
            fs::remove_file(&p).map_err(|e| format!("Failed removing {}: {e}", p.display()))?;
        }
    }
    let _ = run_systemctl_user(&["daemon-reload"]);
    let _ = run_systemctl_user(&["enable", "--now", "hypridle.service"]);
    let _ = run_systemctl_user(&["restart", "hypridle.service"]);
    hyprctl_reload()
}

pub fn run_step_check(step_path: &Path) -> bool {
    Command::new(step_path)
        .arg("--check")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn evaluate_steps(paths: &SetupPaths) -> Vec<FirstLoginStep> {
    let steps_dir = paths.first_login_steps_dir();
    let state_dir = paths.first_login_state_dir();
    let mut steps = list_executable_steps(&steps_dir);

    for step in &mut steps {
        let marker = step_done_marker(&state_dir, &step.id);
        if marker.exists() {
            step.status = StepStatus::Configured;
        } else if run_step_check(&step.path) {
            let _ = mark_step_done(&state_dir, &step.id);
            step.status = StepStatus::Configured;
        } else {
            step.status = StepStatus::Pending;
        }
    }

    steps
}

pub fn should_open_autostart(paths: &SetupPaths) -> (bool, bool) {
    if paths.setup_done_path().exists() {
        return (false, false);
    }
    (true, false)
}

pub fn remove_onboarding_files(exe_path: Option<&Path>, desktop_path: &Path) -> Result<(), String> {
    if let Some(exe) = exe_path
        && fs::symlink_metadata(exe).is_ok()
    {
        fs::remove_file(exe).map_err(|e| format!("Failed removing {}: {e}", exe.display()))?;
    }
    if fs::symlink_metadata(desktop_path).is_ok() {
        fs::remove_file(desktop_path)
            .map_err(|e| format!("Failed removing {}: {e}", desktop_path.display()))?;
    }
    Ok(())
}

pub fn self_remove_onboarding(paths: &SetupPaths) -> Result<(), String> {
    let exe = std::env::current_exe().ok();
    let desktop = paths.setup_desktop_entry_path();
    remove_onboarding_files(exe.as_deref(), &desktop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mark_setup_done;

    #[test]
    fn autostart_opens_only_before_setup_done() {
        let base = std::env::temp_dir().join(format!("inerium_autostart_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let paths = SetupPaths {
            home: base.clone(),
            config_home: base.join(".config"),
            state_home: base.join(".local/state"),
        };

        // Even with a pending step in first-login.d, once setup.done exists, autostart stays closed
        let step_dir = paths.first_login_steps_dir();
        std::fs::create_dir_all(&step_dir).unwrap();
        let step = step_dir.join("99-test-step");
        std::fs::write(&step, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&step, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(should_open_autostart(&paths), (true, false));

        mark_setup_done(&paths).unwrap();
        assert_eq!(should_open_autostart(&paths), (false, false));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn remove_onboarding_files_removes_binary_and_desktop_safely() {
        let base = std::env::temp_dir().join(format!("inerium_self_rm_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let bin_dir = base.join(".local/bin");
        let apps_dir = base.join(".local/share/applications");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&apps_dir).unwrap();

        let fake_exe = bin_dir.join("inerium-setup");
        let fake_desktop = apps_dir.join("inerium-setup.desktop");
        std::fs::write(&fake_exe, b"binary").unwrap();
        std::fs::write(&fake_desktop, b"[Desktop Entry]\n").unwrap();

        remove_onboarding_files(Some(&fake_exe), &fake_desktop).unwrap();
        assert!(!fake_exe.exists());
        assert!(!fake_desktop.exists());

        // Missing files / None exe_path are non-fatal
        remove_onboarding_files(None, &fake_desktop).unwrap();
        remove_onboarding_files(Some(&fake_exe), &fake_desktop).unwrap();

        let _ = std::fs::remove_dir_all(&base);
    }
}
