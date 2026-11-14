#[allow(unused_imports)]
pub use inerium_i18n::{Lang, tr_fmt, tr_plural};

/// Compatibility 3-argument translation function for existing inerium-setup UI modules.
#[inline]
pub const fn tr(lang: Lang, en: &'static str, ru: &'static str) -> &'static str {
    match lang {
        Lang::En => en,
        Lang::Ru => ru,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lang_is_english_and_tr_switches() {
        assert_eq!(Lang::default(), Lang::En);
        assert_eq!(tr(Lang::En, "Settings", "Настройки"), "Settings");
        assert_eq!(tr(Lang::Ru, "Settings", "Настройки"), "Настройки");
    }
}
