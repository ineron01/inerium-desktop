use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub cmd: Vec<String>,
    pub env: Vec<String>,
}

const SESSION_DIRS: &[&str] = &["/usr/share/wayland-sessions"];

/// Разбирает строку Exec= из .desktop файла по правилам Freedesktop/POSIX
pub fn parse_exec_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    let chars: Vec<char> = line.trim().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' && !in_single_quote {
            escaped = true;
        } else if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if ch.is_whitespace() && !in_single_quote && !in_double_quote {
            if !current.is_empty() {
                // Игнорируем Freedesktop specifiers вроде %u, %f, %F, %U
                if !is_freedesktop_specifier(&current) {
                    tokens.push(current);
                }
                current = String::new();
            }
        } else {
            current.push(ch);
        }
        i += 1;
    }

    if !current.is_empty() && !is_freedesktop_specifier(&current) {
        tokens.push(current);
    }

    tokens
}

fn is_freedesktop_specifier(token: &str) -> bool {
    token.starts_with('%') && token.len() == 2
}

pub fn read_sessions_filtered(hide: &[String], default_session: &str) -> Vec<Session> {
    let mut sessions = Vec::new();

    for dir in SESSION_DIRS {
        let path = Path::new(dir);
        if !path.is_dir() {
            continue;
        }

        let mut entries = match fs::read_dir(path) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "desktop"))
                .collect::<Vec<_>>(),
            Err(_) => continue,
        };

        entries.sort();

        for file_path in entries {
            let fname = file_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if hide.iter().any(|h| h == fname) {
                continue;
            }
            let file_stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if let Ok(content) = fs::read_to_string(&file_path)
                && let Some(session) = parse_desktop_file(&content, file_stem)
                && !hide.iter().any(|h| h == &session.name)
            {
                sessions.push(session);
            }
        }
    }

    if !default_session.is_empty()
        && let Some(pos) = sessions.iter().position(|s| s.name == default_session)
        && pos > 0
    {
        let def = sessions.remove(pos);
        sessions.insert(0, def);
    }

    if sessions.is_empty() {
        // Запасной вариант на случай отсутствия сессий
        sessions.push(Session {
            name: "Hyprland".to_string(),
            cmd: vec!["/usr/bin/start-hyprland".to_string()],
            env: vec![
                "XDG_SESSION_TYPE=wayland".to_string(),
                "XDG_CURRENT_DESKTOP=Hyprland".to_string(),
                "XDG_SESSION_DESKTOP=Hyprland".to_string(),
                "DESKTOP_SESSION=hyprland".to_string(),
            ],
        });
    }

    sessions
}

fn parse_desktop_file(content: &str, file_stem: &str) -> Option<Session> {
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut desktop_names = None;
    let mut hidden = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }

        if let Some(val) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(val.trim().to_string());
            }
        } else if let Some(val) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(val.trim().to_string());
            }
        } else if let Some(val) = line.strip_prefix("DesktopNames=") {
            if desktop_names.is_none() {
                desktop_names = Some(val.trim().to_string());
            }
        } else if line == "Hidden=true" || line == "NoDisplay=true" {
            hidden = true;
        }
    }

    if hidden {
        return None;
    }

    match (name, exec) {
        (Some(n), Some(e)) => {
            let cmd = parse_exec_line(&e);
            if !cmd.is_empty() {
                let mut env = vec!["XDG_SESSION_TYPE=wayland".to_string()];
                if let Some(dn) = desktop_names {
                    env.push(format!("XDG_CURRENT_DESKTOP={dn}"));
                    let primary = dn.split(':').next().unwrap_or(&dn).trim();
                    if !primary.is_empty() {
                        env.push(format!("XDG_SESSION_DESKTOP={primary}"));
                    }
                } else {
                    env.push(format!("XDG_CURRENT_DESKTOP={n}"));
                    env.push(format!("XDG_SESSION_DESKTOP={n}"));
                }
                if !file_stem.is_empty() {
                    env.push(format!("DESKTOP_SESSION={file_stem}"));
                }
                Some(Session { name: n, cmd, env })
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exec_line() {
        assert_eq!(
            parse_exec_line("env HYPR_PROFILE=waybar /usr/bin/start-hyprland"),
            vec!["env", "HYPR_PROFILE=waybar", "/usr/bin/start-hyprland"]
        );
        assert_eq!(
            parse_exec_line("sh -c \"echo hello\" %u"),
            vec!["sh", "-c", "echo hello"]
        );
    }

    #[test]
    fn test_parse_desktop_file() {
        let content = r#"
[Desktop Entry]
Name=Hyprland (UWSM-Waybar)
Comment=Hyprland with Waybar shell
Exec=env HYPR_PROFILE=waybar /usr/bin/start-hyprland
Type=Application
DesktopNames=Hyprland
"#;
        let s = parse_desktop_file(content, "hyprland-waybar").unwrap();
        assert_eq!(s.name, "Hyprland (UWSM-Waybar)");
        assert_eq!(s.cmd, vec!["env", "HYPR_PROFILE=waybar", "/usr/bin/start-hyprland"]);
        assert!(s.env.contains(&"XDG_SESSION_TYPE=wayland".to_string()));
        assert!(s.env.contains(&"XDG_CURRENT_DESKTOP=Hyprland".to_string()));
        assert!(s.env.contains(&"XDG_SESSION_DESKTOP=Hyprland".to_string()));
        assert!(s.env.contains(&"DESKTOP_SESSION=hyprland-waybar".to_string()));

        let gnome_content = r#"
[Desktop Entry]
Name=GNOME
Exec=/usr/bin/gnome-session
DesktopNames=GNOME
"#;
        let gnome_s = parse_desktop_file(gnome_content, "gnome").unwrap();
        assert_eq!(gnome_s.name, "GNOME");
        assert_eq!(gnome_s.cmd, vec!["/usr/bin/gnome-session"]);
        assert!(gnome_s.env.contains(&"XDG_SESSION_TYPE=wayland".to_string()));
        assert!(gnome_s.env.contains(&"XDG_CURRENT_DESKTOP=GNOME".to_string()));
        assert!(gnome_s.env.contains(&"DESKTOP_SESSION=gnome".to_string()));

        let nodisplay = r#"
[Desktop Entry]
Name=Hyprland
Exec=/usr/bin/start-hyprland
NoDisplay=true
"#;
        assert!(parse_desktop_file(nodisplay, "hyprland").is_none());
    }
}
