//! Lumen verify-after-edit crate.
//!
//! Detects the language/project type from changed files, generates verification
//! steps (build / vet / test), executes them with a timeout, parses diagnostics,
//! and tracks a repair cycle state machine (max 3 cycles by default).
//!
//! # CLI
//!
//! ```text
//! lumen-verify --root <dir> --changed a.go,b.go
//! ```
//!
//! Exit code 0 = all steps passed.  Non-zero = failures found (diagnostics on
//! stderr / JSON on stdout).

pub mod config;
pub mod detect;
pub mod repair;
pub mod runner;
pub mod steps;

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Full result of a verification run.
#[derive(Debug, serde::Serialize)]
pub struct VerifyResult {
    /// Whether all steps passed.
    pub ok: bool,
    /// Individual step results.
    pub step_results: Vec<StepResult>,
    /// Current repair cycle index (0-based).
    pub repair_cycle: u32,
}

/// Result of a single verification step.
#[derive(Debug, serde::Serialize)]
pub struct StepResult {
    pub language: String,
    pub command: String,
    pub ok: bool,
    pub output: String,
    /// Parsed diagnostics (if any).
    pub diagnostics: Vec<Diagnostic>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// True when the executable was unavailable and the step was intentionally skipped.
    pub skipped: bool,
    /// True when the verifier killed the command after its configured deadline.
    pub timed_out: bool,
}

/// A parsed diagnostic (compiler / linter message).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub severity: String,
    pub message: String,
}

/// Run verification for a set of changed files under a project root.
///
/// Returns `VerifyResult`.  If `max_repair` cycles have been exhausted the
/// `repair_cycle` field will equal `max_repair`.
pub fn run(root: &Path, changed_files: &[PathBuf], cfg: &config::Config) -> Result<VerifyResult> {
    if !cfg.enabled {
        return Ok(VerifyResult {
            ok: true,
            step_results: vec![],
            repair_cycle: 0,
        });
    }
    let languages = detect::detect_languages(changed_files);
    if languages.is_empty() {
        return Ok(VerifyResult {
            ok: true,
            step_results: vec![],
            repair_cycle: 0,
        });
    }

    let all_steps = steps::generate_steps(root, changed_files, &languages, cfg);
    let mut step_results = Vec::new();
    let mut all_ok = true;

    for step in &all_steps {
        let step_result = runner::run_step(root, step, cfg.timeout_secs)?;
        if !step_result.ok {
            all_ok = false;
        }
        step_results.push(step_result);
    }

    Ok(VerifyResult {
        ok: all_ok,
        step_results,
        repair_cycle: 0, // caller increments
    })
}

/// Run the automatic, writer-triggered verifier for one changed file.
///
/// This entry point is deliberately narrower than the standalone CLI. The
/// agent hook currently auto-runs only for Go files inside the active
/// workspace and only when a nearest `go.mod` can be found. That keeps the
/// automatic path on a fixed build/vet/test allowlist and avoids running a
/// package manager such as `npx` merely because a file was edited.
pub fn run_after_edit(
    workspace_root: &Path,
    changed_file: &Path,
    cfg: &config::Config,
) -> Result<Option<VerifyResult>> {
    if !cfg.enabled || changed_file.extension().and_then(|ext| ext.to_str()) != Some("go") {
        return Ok(None);
    }

    let workspace_root = dunce::canonicalize(workspace_root)
        .with_context(|| format!("canonicalize workspace {}", workspace_root.display()))?;
    let changed_file = dunce::canonicalize(changed_file)
        .with_context(|| format!("canonicalize changed file {}", changed_file.display()))?;
    if !changed_file.starts_with(&workspace_root) {
        bail!(
            "refusing to verify edited file outside workspace: {}",
            changed_file.display()
        );
    }

    let Some(project_root) = nearest_go_module(&workspace_root, &changed_file) else {
        return Ok(None);
    };
    run(&project_root, &[changed_file], cfg).map(Some)
}

fn nearest_go_module(workspace_root: &Path, changed_file: &Path) -> Option<PathBuf> {
    let mut dir = changed_file.parent()?;
    loop {
        if dir.join("go.mod").is_file() {
            return Some(dir.to_path_buf());
        }
        if dir == workspace_root {
            return None;
        }
        dir = dir.parent()?;
        if !dir.starts_with(workspace_root) {
            return None;
        }
    }
}

/// Format diagnostics into a human-readable string suitable for model feedback.
pub fn format_diagnostics(results: &[StepResult]) -> String {
    let mut out = String::new();
    for sr in results {
        if sr.diagnostics.is_empty() && sr.ok {
            continue;
        }
        out.push_str(&format!("\n## {} — {}\n", sr.language, sr.command));
        if sr.ok {
            out.push_str("  ✓ passed\n");
        } else {
            out.push_str(&format!("  ✗ FAILED ({}ms)\n", sr.duration_ms));
            for d in &sr.diagnostics {
                let loc = match (d.line, d.col) {
                    (Some(l), Some(c)) => format!("{}:{}:{}", d.file, l, c),
                    (Some(l), None) => format!("{}:{}", d.file, l),
                    _ => d.file.clone(),
                };
                out.push_str(&format!("    {}: {}: {}\n", d.severity, loc, d.message));
            }
        }
    }
    if out.is_empty() {
        out.push_str("All verification steps passed.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_go_from_extension() {
        let files = vec![PathBuf::from("main.go")];
        let langs = detect::detect_languages(&files);
        assert!(langs.contains(&"go".to_string()));
    }

    #[test]
    fn test_detect_py_from_extension() {
        let files = vec![PathBuf::from("app.py")];
        let langs = detect::detect_languages(&files);
        assert!(langs.contains(&"python".to_string()));
    }

    #[test]
    fn test_detect_ts_from_extension() {
        let files = vec![PathBuf::from("index.ts")];
        let langs = detect::detect_languages(&files);
        assert!(langs.contains(&"typescript".to_string()));
    }

    #[test]
    fn test_detect_empty_no_langs() {
        let empty: &[PathBuf] = &[];
        let langs = detect::detect_languages(empty);
        assert!(langs.is_empty());
    }

    #[test]
    fn test_format_diagnostics_empty() {
        let results = vec![StepResult {
            language: "go".into(),
            command: "go build ./...".into(),
            ok: true,
            output: String::new(),
            diagnostics: vec![],
            duration_ms: 123,
            skipped: false,
            timed_out: false,
        }];
        let s = format_diagnostics(&results);
        assert!(s.contains("passed"));
    }

    #[test]
    fn test_format_diagnostics_with_errors() {
        let results = vec![StepResult {
            language: "go".into(),
            command: "go build ./...".into(),
            ok: false,
            output: String::new(),
            diagnostics: vec![Diagnostic {
                file: "main.go".into(),
                line: Some(10),
                col: Some(5),
                severity: "ERROR".into(),
                message: "undefined: foo".into(),
            }],
            duration_ms: 234,
            skipped: false,
            timed_out: false,
        }];
        let s = format_diagnostics(&results);
        assert!(s.contains("FAILED"));
        assert!(s.contains("main.go:10:5"));
        assert!(s.contains("undefined: foo"));
    }

    #[test]
    fn automatic_go_verify_runs_broken_then_fixed_cycle() {
        if which::which("go").is_err() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("go.mod"),
            "module example.com/verifyfixture\n",
        )
        .unwrap();
        let main = tmp.path().join("main.go");
        std::fs::write(&main, "package main\n\nfunc main() { missingSymbol() }\n").unwrap();

        let cfg = config::Config {
            timeout_secs: 15,
            ..config::Config::default()
        };
        let broken = run_after_edit(tmp.path(), &main, &cfg)
            .unwrap()
            .expect("Go module should trigger automatic verification");
        assert!(!broken.ok);
        assert!(format_diagnostics(&broken.step_results).contains("missingSymbol"));

        std::fs::write(&main, "package main\n\nfunc main() {}\n").unwrap();
        let fixed = run_after_edit(tmp.path(), &main, &cfg)
            .unwrap()
            .expect("Go module should still trigger verification");
        assert!(fixed.ok, "fixed Go fixture should pass: {fixed:#?}");
        assert!(fixed.step_results.iter().all(|step| step.ok));
    }

    #[test]
    fn automatic_verify_refuses_go_file_outside_workspace() {
        let workspace = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("go.mod"), "module example.com/outside\n").unwrap();
        let changed = outside.path().join("main.go");
        std::fs::write(&changed, "package main\nfunc main() {}\n").unwrap();

        let error = run_after_edit(workspace.path(), &changed, &config::Config::default())
            .expect_err("outside-workspace verification must fail closed");
        assert!(error.to_string().contains("outside workspace"));
    }

    #[test]
    fn automatic_verify_ignores_non_go_file_without_running_commands() {
        let workspace = tempfile::TempDir::new().unwrap();
        let changed = workspace.path().join("notes.txt");
        std::fs::write(&changed, "not source code\n").unwrap();
        assert!(
            run_after_edit(workspace.path(), &changed, &config::Config::default())
                .unwrap()
                .is_none()
        );
    }
}
