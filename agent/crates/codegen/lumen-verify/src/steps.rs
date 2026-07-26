//! Verification step generation.

use super::config::Config;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A single verification step to execute.
#[derive(Debug, Clone)]
pub struct Step {
    pub language: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Generate verification steps for a set of detected languages.
///
/// Steps are ordered: build → vet → test.  Tools that aren't installed are
/// silently skipped (each step is optional unless marked required).
pub fn generate_steps(
    root: &Path,
    changed_files: &[PathBuf],
    languages: &[String],
    cfg: &Config,
) -> Vec<Step> {
    let mut steps = Vec::new();

    for lang in languages {
        match lang.as_str() {
            "go" => {
                let targets = go_targets(root, changed_files, &cfg.scope);
                steps.push(Step {
                    language: "go".into(),
                    label: "go build".into(),
                    command: "go".into(),
                    args: command_args("build", &targets),
                });
                steps.push(Step {
                    language: "go".into(),
                    label: "go vet".into(),
                    command: "go".into(),
                    args: command_args("vet", &targets),
                });
                steps.push(Step {
                    language: "go".into(),
                    label: "go test".into(),
                    command: "go".into(),
                    args: command_args("test", &targets),
                });
            }
            "python" => {
                // Only run if tools exist (runner checks availability).
                steps.push(Step {
                    language: "python".into(),
                    label: "ruff check".into(),
                    command: "ruff".into(),
                    args: vec!["check".into(), ".".into()],
                });
                steps.push(Step {
                    language: "python".into(),
                    label: "pytest".into(),
                    command: "pytest".into(),
                    args: vec!["-q".into()],
                });
            }
            "typescript" => {
                // Prefer binaries already installed in the project's
                // node_modules/.bin (no network, no package install). The npx
                // fallback exists only for the standalone CLI; the automatic
                // writer hook disables it via cfg.allow_package_runner=false.
                match local_node_bin(root, "tsc") {
                    Some(tsc) => steps.push(Step {
                        language: "typescript".into(),
                        label: "tsc --noEmit (local)".into(),
                        command: tsc.to_string_lossy().into_owned(),
                        args: vec!["--noEmit".into()],
                    }),
                    None if cfg.allow_package_runner => steps.push(Step {
                        language: "typescript".into(),
                        label: "tsc --noEmit".into(),
                        command: "npx".into(),
                        args: vec!["tsc".into(), "--noEmit".into()],
                    }),
                    None => {}
                }
                match local_node_bin(root, "jest") {
                    Some(jest) => steps.push(Step {
                        language: "typescript".into(),
                        label: "jest (local)".into(),
                        command: jest.to_string_lossy().into_owned(),
                        args: vec!["--passWithNoTests".into()],
                    }),
                    None if cfg.allow_package_runner => steps.push(Step {
                        language: "typescript".into(),
                        label: "jest".into(),
                        command: "npx".into(),
                        args: vec!["jest".into(), "--passWithNoTests".into()],
                    }),
                    None => {}
                }
            }
            _ => {}
        }
    }

    steps
}

/// Resolve a binary installed in the project's own `node_modules/.bin`.
/// Returns None when it is not present — callers must not fall back to a
/// package runner unless explicitly allowed.
pub(crate) fn local_node_bin(root: &Path, name: &str) -> Option<PathBuf> {
    let bin_dir = root.join("node_modules").join(".bin");
    let candidates: &[String] = &if cfg!(windows) {
        [format!("{name}.cmd"), format!("{name}.exe"), name.to_string()]
    } else {
        [name.to_string(), format!("{name}.cmd"), format!("{name}.exe")]
    };
    candidates
        .iter()
        .map(|c| bin_dir.join(c))
        .find(|p| p.is_file())
}

fn command_args(command: &str, targets: &[String]) -> Vec<String> {
    std::iter::once(command.to_string())
        .chain(targets.iter().cloned())
        .collect()
}

/// Resolve the smallest set of Go packages containing the changed files.
///
/// The automatic writer hook always passes canonical paths inside `root`; the
/// defensive fallbacks keep the standalone CLI useful for relative paths.
fn go_targets(root: &Path, changed_files: &[PathBuf], scope: &str) -> Vec<String> {
    if scope == "workspace" {
        return vec!["./...".to_string()];
    }

    let mut targets = BTreeSet::new();
    for changed in changed_files {
        if changed.extension().and_then(|ext| ext.to_str()) != Some("go") {
            continue;
        }
        let absolute = if changed.is_absolute() {
            changed.clone()
        } else {
            root.join(changed)
        };
        let Some(parent) = absolute.parent() else {
            continue;
        };
        let Ok(relative) = parent.strip_prefix(root) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            targets.insert(".".to_string());
        } else {
            targets.insert(format!("./{}", relative.to_string_lossy()));
        }
    }

    if targets.is_empty() {
        targets.insert(".".to_string());
    }
    targets.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_changed_pkg_scope_targets_only_changed_packages() {
        let root = Path::new("/workspace");
        let changed = vec![
            root.join("cmd/api/main.go"),
            root.join("cmd/api/handler.go"),
            root.join("pkg/lib/lib.go"),
        ];
        assert_eq!(
            go_targets(root, &changed, "changed-pkg"),
            vec!["./cmd/api", "./pkg/lib"]
        );
    }

    #[test]
    fn go_workspace_scope_keeps_explicit_workspace_target() {
        assert_eq!(
            go_targets(Path::new("/workspace"), &[], "workspace"),
            vec!["./..."]
        );
    }

    #[test]
    fn typescript_without_local_bin_and_no_package_runner_emits_no_steps() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg = Config {
            allow_package_runner: false,
            ..Config::default()
        };
        let steps = generate_steps(
            tmp.path(),
            &[tmp.path().join("index.ts")],
            &["typescript".to_string()],
            &cfg,
        );
        assert!(steps.is_empty(), "no npx fallback allowed: {steps:#?}");
    }

    #[test]
    fn typescript_with_package_runner_falls_back_to_npx() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg = Config::default();
        let steps = generate_steps(
            tmp.path(),
            &[tmp.path().join("index.ts")],
            &["typescript".to_string()],
            &cfg,
        );
        assert!(steps.iter().any(|s| s.command == "npx"));
    }

    #[test]
    fn typescript_prefers_local_node_bin() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bin = tmp.path().join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tsc"), "").unwrap();
        let cfg = Config {
            allow_package_runner: false,
            ..Config::default()
        };
        let steps = generate_steps(
            tmp.path(),
            &[tmp.path().join("index.ts")],
            &["typescript".to_string()],
            &cfg,
        );
        assert_eq!(steps.len(), 1);
        assert!(steps[0].command.contains("node_modules"));
        assert_eq!(steps[0].args, vec!["--noEmit".to_string()]);
    }
}
