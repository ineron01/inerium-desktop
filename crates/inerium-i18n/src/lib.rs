use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En = 0,
    Ru = 1,
}

static CURRENT_LANG: AtomicU8 = AtomicU8::new(255);

impl Lang {
    #[inline]
    pub fn get() -> Self {
        match CURRENT_LANG.load(Ordering::Relaxed) {
            0 => Self::En,
            1 => Self::Ru,
            _ => Self::init(),
        }
    }

    #[inline]
    pub fn set(lang: Self) {
        CURRENT_LANG.store(lang as u8, Ordering::Relaxed);
    }

    #[inline]
    pub fn reinit() -> Self {
        let detected = Self::detect();
        CURRENT_LANG.store(detected as u8, Ordering::Relaxed);
        detected
    }

    fn init() -> Self {
        let detected = Self::detect();
        CURRENT_LANG.store(detected as u8, Ordering::Relaxed);
        detected
    }

    pub fn detect() -> Self {
        // 1. User config: $HOME/.config/locale.conf
        if let Some(home) = std::env::var_os("HOME") {
            let conf_path = PathBuf::from(home).join(".config/locale.conf");
            if let Some(lang) = parse_locale_file(&conf_path) {
                return lang;
            }
        }

        // 2. Process environment variables with standard POSIX priority
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var)
                && let Some(lang) = parse_locale_str(&val)
            {
                return lang;
            }
        }

        // 3. System-wide fallback: /etc/locale.conf
        let etc_conf = Path::new("/etc/locale.conf");
        if let Some(lang) = parse_locale_file(etc_conf) {
            return lang;
        }

        // 4. Inerium default
        Self::En
    }
}

fn parse_locale_str(val: &str) -> Option<Lang> {
    let s = val.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("ru") {
        Some(Lang::Ru)
    } else if s.starts_with("en") || s.starts_with("c") {
        Some(Lang::En)
    } else {
        None
    }
}

fn parse_locale_file(path: &Path) -> Option<Lang> {
    let content = fs::read_to_string(path).ok()?;
    let mut lc_all = None;
    let mut lc_messages = None;
    let mut lang = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim();
            let val = v.trim().trim_matches('"').trim_matches('\'');
            match key {
                "LC_ALL" => lc_all = parse_locale_str(val),
                "LC_MESSAGES" => lc_messages = parse_locale_str(val),
                "LANG" => lang = parse_locale_str(val),
                _ => {}
            }
        }
    }

    lc_all.or(lc_messages).or(lang)
}

#[inline]
pub fn tr(en: &'static str, ru: &'static str) -> &'static str {
    match Lang::get() {
        Lang::En => en,
        Lang::Ru => ru,
    }
}

#[inline]
pub fn tr_plural(
    n: usize,
    en_one: &'static str,
    en_other: &'static str,
    ru_one: &'static str,
    ru_few: &'static str,
    ru_many: &'static str,
) -> &'static str {
    match Lang::get() {
        Lang::En => {
            if n == 1 {
                en_one
            } else {
                en_other
            }
        }
        Lang::Ru => {
            let n10 = n % 10;
            let n100 = n % 100;
            if n10 == 1 && n100 != 11 {
                ru_one
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                ru_few
            } else {
                ru_many
            }
        }
    }
}

#[macro_export]
macro_rules! tr_fmt {
    ($en:literal, $ru:literal $(, $($arg:tt)*)?) => {
        match $crate::Lang::get() {
            $crate::Lang::En => format!($en $(, $($arg)*)?),
            $crate::Lang::Ru => format!($ru $(, $($arg)*)?),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_switching() {
        Lang::set(Lang::En);
        assert_eq!(Lang::get(), Lang::En);
        assert_eq!(tr("Power", "Питание"), "Power");

        Lang::set(Lang::Ru);
        assert_eq!(Lang::get(), Lang::Ru);
        assert_eq!(tr("Power", "Питание"), "Питание");
    }

    #[test]
    fn test_pluralization() {
        Lang::set(Lang::En);
        assert_eq!(tr_plural(1, "update", "updates", "обновление", "обновления", "обновлений"), "update");
        assert_eq!(tr_plural(5, "update", "updates", "обновление", "обновления", "обновлений"), "updates");

        Lang::set(Lang::Ru);
        assert_eq!(tr_plural(1, "update", "updates", "обновление", "обновления", "обновлений"), "обновление");
        assert_eq!(tr_plural(2, "update", "updates", "обновление", "обновления", "обновлений"), "обновления");
        assert_eq!(tr_plural(5, "update", "updates", "обновление", "обновления", "обновлений"), "обновлений");
        assert_eq!(tr_plural(11, "update", "updates", "обновление", "обновления", "обновлений"), "обновлений");
        assert_eq!(tr_plural(21, "update", "updates", "обновление", "обновления", "обновлений"), "обновление");
    }

    #[test]
    fn test_locale_file_priority() {
        let content = "LANG=en_US.UTF-8\nLC_MESSAGES=ru_RU.UTF-8\n";
        let tmp = std::env::temp_dir().join(format!("test_locale_{}.conf", std::process::id()));
        fs::write(&tmp, content).unwrap();
        let lang = parse_locale_file(&tmp);
        let _ = fs::remove_file(&tmp);
        assert_eq!(lang, Some(Lang::Ru));
    }
}
