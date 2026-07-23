//! L0: strip / detect zero-width and bidi / invisible Unicode.

/// True if `s` contains any known injection-friendly invisible rune.
pub fn contains_hidden_chars(s: &str) -> bool {
    s.chars().any(is_hidden)
}

/// Remove zero-width / bidi / invisible characters used for evasion.
pub fn strip_hidden_chars(s: &str) -> String {
    s.chars().filter(|c| !is_hidden(*c)).collect()
}

fn is_hidden(r: char) -> bool {
    matches!(
        r,
        '\u{200B}' // ZWSP
            | '\u{200C}' // ZWNJ
            | '\u{200D}' // ZWJ
            | '\u{FEFF}' // BOM / ZWNBSP
            | '\u{200E}' // LRM
            | '\u{200F}' // RLM
            | '\u{202A}'..='\u{202E}' // bidi embedding/override
            | '\u{2060}'..='\u{2064}' // word joiner .. invisible plus
            | '\u{180E}' // MONGOLIAN VOWEL SEPARATOR (spec)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_removes_zwsp() {
        assert_eq!(strip_hidden_chars("rm\u{200B} -rf /"), "rm -rf /");
    }

    #[test]
    fn contains_detects_bom() {
        assert!(contains_hidden_chars("a\u{FEFF}b"));
        assert!(!contains_hidden_chars("ab"));
    }
}
