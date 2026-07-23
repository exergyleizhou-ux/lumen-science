//! Language detection from changed file extensions.

use std::collections::BTreeSet;
use std::path::Path;

/// Map file extension → language identifier.
fn classify(ext: &str) -> Option<&'static str> {
    match ext {
        "go" => Some("go"),
        "py" => Some("python"),
        "ts" | "tsx" | "js" | "jsx" => Some("typescript"),
        _ => None,
    }
}

/// Detect the set of languages present in a list of changed files.
/// Returns a sorted, deduplicated list of language identifiers ("go", "python", "typescript").
pub fn detect_languages(changed_files: &[impl AsRef<Path>]) -> Vec<String> {
    let mut langs = BTreeSet::new();
    for f in changed_files {
        let p = f.as_ref();
        if let Some(ext) = p.extension().and_then(|e| e.to_str())
            && let Some(lang) = classify(ext)
        {
            langs.insert(lang.to_string());
        }
    }
    langs.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_go_mixed() {
        let files: Vec<PathBuf> = vec![
            "main.go".into(),
            "util.go".into(),
            "README.md".into(),
        ];
        let langs = detect_languages(&files);
        assert_eq!(langs, vec!["go"]);
    }

    #[test]
    fn test_multi_lang() {
        let files: Vec<PathBuf> = vec!["app.py".into(), "lib.ts".into(), "helper.go".into()];
        let langs = detect_languages(&files);
        assert!(langs.contains(&"go".to_string()));
        assert!(langs.contains(&"python".to_string()));
        assert!(langs.contains(&"typescript".to_string()));
    }

    #[test]
    fn test_ts_variants() {
        let files: Vec<PathBuf> = vec!["x.ts".into(), "y.tsx".into(), "z.js".into(), "w.jsx".into()];
        let langs = detect_languages(&files);
        assert_eq!(langs, vec!["typescript"]);
    }
}
