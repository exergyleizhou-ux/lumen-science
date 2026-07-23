//! Configuration for verify-after-edit.

use anyhow::{Context, Result, bail};

const MAX_REPAIR_LIMIT: u32 = 100;
const MAX_TIMEOUT_SECS: u64 = 3_600;

/// Verify configuration (mirrors user `config.toml` `[verify]`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Whether verify-after-edit is enabled.
    pub enabled: bool,
    /// Maximum repair cycles before giving up (default 3).
    pub max_repair: u32,
    /// Per-step timeout in seconds (default 30).
    pub timeout_secs: u64,
    /// Verification scope: "changed-pkg" or "workspace".
    pub scope: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            max_repair: 3,
            timeout_secs: 30,
            scope: "changed-pkg".to_string(),
        }
    }
}

impl Config {
    /// Parse the `[verify]` table from a complete user config document.
    ///
    /// Missing tables and fields use the documented defaults. Invalid policy
    /// values fail closed so an edit cannot silently run with a different
    /// verification budget or scope than the user requested.
    pub fn from_toml_str(input: &str) -> Result<Self> {
        let root = toml::from_str::<toml::Value>(input)
            .map_err(|error| anyhow::anyhow!("invalid config.toml syntax: {}", error.message()))?;
        Self::from_toml_value(&root)
    }

    /// Decode the `[verify]` table from an already safely loaded user config.
    pub fn from_toml_value(root: &toml::Value) -> Result<Self> {
        let Some(section) = root.get("verify") else {
            return Ok(Self::default());
        };
        let cfg = section
            .clone()
            .try_into::<Self>()
            .map_err(|error| anyhow::anyhow!("invalid [verify] configuration: {}", error.message()))
            .context("read user verify policy")?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if !(1..=MAX_REPAIR_LIMIT).contains(&self.max_repair) {
            bail!(
                "verify.max_repair must be between 1 and {MAX_REPAIR_LIMIT}, got {}",
                self.max_repair
            );
        }
        if !(1..=MAX_TIMEOUT_SECS).contains(&self.timeout_secs) {
            bail!(
                "verify.timeout_secs must be between 1 and {MAX_TIMEOUT_SECS}, got {}",
                self.timeout_secs
            );
        }
        if !matches!(self.scope.as_str(), "changed-pkg" | "workspace") {
            bail!("verify.scope must be \"changed-pkg\" or \"workspace\"");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_complete_verify_section_from_user_config() {
        let cfg = Config::from_toml_str(
            r#"
                [models]
                default = "deepseek"

                [verify]
                enabled = false
                max_repair = 7
                timeout_secs = 91
                scope = "workspace"
            "#,
        )
        .expect("valid [verify] config");

        assert!(!cfg.enabled);
        assert_eq!(cfg.max_repair, 7);
        assert_eq!(cfg.timeout_secs, 91);
        assert_eq!(cfg.scope, "workspace");
    }

    #[test]
    fn missing_verify_fields_keep_safe_defaults() {
        let cfg = Config::from_toml_str("[verify]\ntimeout_secs = 12\n")
            .expect("partial [verify] config");

        assert!(cfg.enabled);
        assert_eq!(cfg.max_repair, 3);
        assert_eq!(cfg.timeout_secs, 12);
        assert_eq!(cfg.scope, "changed-pkg");
    }

    #[test]
    fn invalid_verify_policy_is_rejected() {
        let error = Config::from_toml_str("[verify]\nmax_repair = 0\n")
            .expect_err("invalid repair budget must fail closed");

        assert!(error.to_string().contains("max_repair"));

        let error = Config::from_toml_str("[verify]\nscope = \"everything\"\n")
            .expect_err("unknown scope must fail closed");
        assert!(error.to_string().contains("scope"));
    }
}
