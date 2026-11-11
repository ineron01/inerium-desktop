//! High-performance string escaping for JSON output and Pango markup.

/// Escapes a string to be safely embedded inside a JSON string literal.
pub fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if (c as u32) < 32 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Escapes XML/Pango markup special characters: `&`, `<`, `>`, `"`, `'`.
pub fn escape_pango(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json() {
        assert_eq!(escape_json("hello \"world\"\n"), "hello \\\"world\\\"\\n");
        assert_eq!(escape_json("c:\\path\\file"), "c:\\\\path\\\\file");
    }

    #[test]
    fn test_escape_pango() {
        assert_eq!(escape_pango("<test & 'quote'>"), "&lt;test &amp; &apos;quote&apos;&gt;");
    }
}
