use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/greetd/inerium-greeter.json";
pub const DEFAULT_STATE_PATH: &str = "/var/lib/inerium-greeter/state.json";
pub const DEMO_STATE_PATH: &str = "/tmp/inerium-greeter-demo-state.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_user")]
    pub user: String,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_session")]
    pub default_session: String,
    #[serde(default)]
    pub hide_sessions: Vec<String>,
}

fn default_user() -> String {
    if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4
                && let Ok(uid) = parts[2].parse::<u32>()
                && (1000..65534).contains(&uid)
            {
                return parts[0].to_string();
            }
        }
    }
    String::new()
}

fn default_background() -> String {
    "/usr/share/backgrounds/inerium-greeter.jpg".to_string()
}

fn default_session() -> String {
    "Hyprland (UWSM-Waybar)".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            user: default_user(),
            background: default_background(),
            default_session: default_session(),
            hide_sessions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub user: Option<String>,
    pub session: Option<String>,
}

pub fn load_config(path: &str) -> Config {
    if let Ok(content) = fs::read_to_string(path)
        && let Ok(cfg) = serde_json::from_str(&content)
    {
        return cfg;
    }
    Config::default()
}

pub fn load_state(path: &str) -> State {
    if let Ok(content) = fs::read_to_string(path)
        && let Ok(st) = serde_json::from_str(&content)
    {
        return st;
    }
    State::default()
}

pub fn save_state(path: &str, user: &str, session: &str) -> Result<(), String> {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let state = State {
        user: Some(user.to_string()),
        session: Some(session.to_string()),
    };

    let data = serde_json::to_vec_pretty(&state)
        .map_err(|e| format!("Ошибка сериализации состояния: {}", e))?;

    let tmp_path = format!("{}.tmp", path);
    let mut file = fs::File::create(&tmp_path)
        .map_err(|e| format!("Не удалось создать временный файл состояния: {}", e))?;

    file.write_all(&data)
        .map_err(|e| format!("Не удалось записать состояние: {}", e))?;
    file.sync_all()
        .map_err(|e| format!("Ошибка сброса состояния на диск: {}", e))?;
    drop(file);

    fs::rename(&tmp_path, path)
        .map_err(|e| format!("Не удалось атомарно переименовать файл состояния: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_fallback() {
        let cfg = load_config("/nonexistent/path/inerium.json");
        assert_eq!(cfg.user, default_user());
        assert_eq!(cfg.default_session, "Hyprland (UWSM-Waybar)");
        assert!(cfg.hide_sessions.is_empty());
    }

    #[test]
    fn test_save_and_load_state() {
        let test_state_path = "/tmp/test-inerium-greeter-state.json";
        let _ = fs::remove_file(test_state_path);

        assert!(save_state(test_state_path, "testuser", "TestSession").is_ok());

        let loaded = load_state(test_state_path);
        assert_eq!(loaded.user.as_deref(), Some("testuser"));
        assert_eq!(loaded.session.as_deref(), Some("TestSession"));

        let _ = fs::remove_file(test_state_path);
    }
}
