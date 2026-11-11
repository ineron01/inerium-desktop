use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub cmd: Vec<String>,
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

pub fn read_sessions() -> Vec<Session> {
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
            if let Ok(content) = fs::read_to_string(&file_path)
                && let Some(session) = parse_desktop_file(&content)
            {
                sessions.push(session);
            }
        }
    }

    if sessions.is_empty() {
        // Запасной вариант на случай отсутствия сессий
        sessions.push(Session {
            name: "Hyprland".to_string(),
            cmd: vec!["/usr/bin/start-hyprland".to_string()],
        });
    }

    sessions
}

fn parse_desktop_file(content: &str) -> Option<Session> {
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
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
                Some(Session { name: n, cmd })
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
Name=Hyprland (Waybar)
Comment=Hyprland with Waybar shell
Exec=env HYPR_PROFILE=waybar /usr/bin/start-hyprland
Type=Application
"#;
        let s = parse_desktop_file(content).unwrap();
        assert_eq!(s.name, "Hyprland (Waybar)");
        assert_eq!(s.cmd, vec!["env", "HYPR_PROFILE=waybar", "/usr/bin/start-hyprland"]);

        let nodisplay = r#"
[Desktop Entry]
Name=Hyprland
Exec=/usr/bin/start-hyprland
NoDisplay=true
"#;
        assert!(parse_desktop_file(nodisplay).is_none());
    }
}
