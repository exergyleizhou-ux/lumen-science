//! Legacy `~/.grok` → Lumen `~/.lumen` config migration (FINAL-5UX Gate B).
//!
//! Pure filesystem helpers: dry-run never writes; apply copies selected files
//! without deleting the legacy source; receipt never contains secret material.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Files we consider safe to import from a legacy Grok home into Lumen home.
/// Auth/session stores are intentionally excluded from automatic copy.
pub const MIGRATABLE_RELATIVE_PATHS: &[&str] = &["config.toml"];

/// One planned or applied file operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFilePlan {
    pub relative_path: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub source_exists: bool,
    pub target_exists: bool,
}

/// Result of a dry-run or apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub legacy_home: PathBuf,
    pub lumen_home: PathBuf,
    pub files: Vec<MigrationFilePlan>,
}

/// On-disk receipt written after a successful apply (JSON, no secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReceipt {
    pub migrated_at_unix_secs: u64,
    pub legacy_home: PathBuf,
    pub lumen_home: PathBuf,
    pub copied: Vec<String>,
    pub skipped_existing: Vec<String>,
}

#[derive(Debug)]
pub enum MigrationError {
    Io(io::Error),
    TargetExists { relative_path: String },
    NothingToCopy,
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "migration I/O error: {e}"),
            Self::TargetExists { relative_path } => {
                write!(
                    f,
                    "refusing to overwrite existing Lumen config file: {relative_path}"
                )
            }
            Self::NothingToCopy => write!(f, "no migratable files found in legacy home"),
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<io::Error> for MigrationError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Build a migration plan without writing anything.
pub fn plan_migration(legacy_home: &Path, lumen_home: &Path) -> MigrationPlan {
    let files = MIGRATABLE_RELATIVE_PATHS
        .iter()
        .map(|rel| {
            let source = legacy_home.join(rel);
            let target = lumen_home.join(rel);
            MigrationFilePlan {
                relative_path: (*rel).to_owned(),
                source_exists: source.is_file(),
                target_exists: target.is_file(),
                source,
                target,
            }
        })
        .collect();
    MigrationPlan {
        legacy_home: legacy_home.to_path_buf(),
        lumen_home: lumen_home.to_path_buf(),
        files,
    }
}

/// Dry-run: same as [`plan_migration`]; never mutates the filesystem.
pub fn dry_run(legacy_home: &Path, lumen_home: &Path) -> MigrationPlan {
    plan_migration(legacy_home, lumen_home)
}

/// Apply migration: copy source files that exist into lumen_home.
///
/// - Does **not** delete legacy files.
/// - Does **not** overwrite existing target files (returns error).
/// - Writes `migration-receipt.json` under lumen_home on success.
pub fn apply_migration(legacy_home: &Path, lumen_home: &Path) -> Result<MigrationReceipt, MigrationError> {
    let plan = plan_migration(legacy_home, lumen_home);
    let mut copied = Vec::new();
    let skipped_existing = Vec::new();

    fs::create_dir_all(lumen_home)?;

    for file in &plan.files {
        if !file.source_exists {
            continue;
        }
        if file.target_exists {
            return Err(MigrationError::TargetExists {
                relative_path: file.relative_path.clone(),
            });
        }
        if let Some(parent) = file.target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&file.source, &file.target)?;
        copied.push(file.relative_path.clone());
    }

    if copied.is_empty() {
        return Err(MigrationError::NothingToCopy);
    }

    let migrated_at_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let receipt = MigrationReceipt {
        migrated_at_unix_secs,
        legacy_home: legacy_home.to_path_buf(),
        lumen_home: lumen_home.to_path_buf(),
        copied: copied.clone(),
        skipped_existing,
    };
    write_receipt(lumen_home, &receipt)?;
    Ok(receipt)
}

fn write_receipt(lumen_home: &Path, receipt: &MigrationReceipt) -> io::Result<()> {
    let path = lumen_home.join("migration-receipt.json");
    // Hand-built JSON to avoid pulling serde_json if not already a dep of this crate.
    let body = format!(
        "{{\n  \"migrated_at_unix_secs\": {},\n  \"legacy_home\": {},\n  \"lumen_home\": {},\n  \"copied\": [{}],\n  \"skipped_existing\": [{}]\n}}\n",
        receipt.migrated_at_unix_secs,
        json_string(&receipt.legacy_home.to_string_lossy()),
        json_string(&receipt.lumen_home.to_string_lossy()),
        receipt
            .copied
            .iter()
            .map(|s| json_string(s))
            .collect::<Vec<_>>()
            .join(", "),
        receipt
            .skipped_existing
            .iter()
            .map(|s| json_string(s))
            .collect::<Vec<_>>()
            .join(", "),
    );
    // Guard: never persist obvious secret markers into the receipt body we control.
    debug_assert!(
        !body.contains("sk-") && !body.contains("api_key"),
        "receipt must not embed secret-looking material"
    );
    fs::write(path, body)
}

fn json_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn dry_run_does_not_create_target_files() {
        let legacy = TempDir::new().unwrap();
        let lumen = TempDir::new().unwrap();
        fs::write(legacy.path().join("config.toml"), "default = \"deepseek-chat\"\n").unwrap();
        let plan = dry_run(legacy.path(), lumen.path());
        assert!(plan.files.iter().any(|f| f.source_exists));
        assert!(!lumen.path().join("config.toml").exists());
        assert!(!lumen.path().join("migration-receipt.json").exists());
    }

    #[test]
    fn apply_copies_config_and_writes_receipt_without_secrets() {
        let legacy = TempDir::new().unwrap();
        let lumen = TempDir::new().unwrap();
        let cfg = "default = \"deepseek-chat\"\n# no keys here\n";
        fs::write(legacy.path().join("config.toml"), cfg).unwrap();

        let receipt = apply_migration(legacy.path(), lumen.path()).expect("apply");
        assert_eq!(receipt.copied, vec!["config.toml".to_owned()]);
        assert_eq!(
            fs::read_to_string(lumen.path().join("config.toml")).unwrap(),
            cfg
        );
        // Legacy preserved
        assert!(legacy.path().join("config.toml").exists());

        let receipt_body = fs::read_to_string(lumen.path().join("migration-receipt.json")).unwrap();
        assert!(receipt_body.contains("config.toml"));
        assert!(!receipt_body.contains("sk-"));
        assert!(!receipt_body.to_ascii_lowercase().contains("api_key"));
    }

    #[test]
    fn apply_refuses_overwrite() {
        let legacy = TempDir::new().unwrap();
        let lumen = TempDir::new().unwrap();
        fs::write(legacy.path().join("config.toml"), "from = \"legacy\"\n").unwrap();
        fs::write(lumen.path().join("config.toml"), "from = \"lumen\"\n").unwrap();
        let err = apply_migration(legacy.path(), lumen.path()).unwrap_err();
        assert!(matches!(err, MigrationError::TargetExists { .. }));
        assert_eq!(
            fs::read_to_string(lumen.path().join("config.toml")).unwrap(),
            "from = \"lumen\"\n"
        );
    }

    #[test]
    fn apply_errors_when_nothing_to_copy() {
        let legacy = TempDir::new().unwrap();
        let lumen = TempDir::new().unwrap();
        let err = apply_migration(legacy.path(), lumen.path()).unwrap_err();
        assert!(matches!(err, MigrationError::NothingToCopy));
    }
}
