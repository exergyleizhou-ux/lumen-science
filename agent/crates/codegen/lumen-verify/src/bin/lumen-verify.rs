//! CLI: lumen-verify --root <dir> --changed a.go,b.go
use anyhow::{bail, Context, Result};
use lumen_verify::config::Config;
use lumen_verify::{format_diagnostics, run};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("lumen-verify: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<ExitCode> {
    let mut args = env::args().skip(1);
    let mut root = PathBuf::from(".");
    let mut changed: Vec<PathBuf> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => {
                root = PathBuf::from(args.next().context("--root needs path")?);
            }
            "--changed" => {
                let list = args.next().context("--changed needs comma list")?;
                changed = list
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .collect();
            }
            "-h" | "--help" => {
                println!(
                    "Usage: lumen-verify --root <dir> --changed a.go,b.py\n\
                     Exit 0 if verify steps pass; 1 if failures; 2 on usage error."
                );
                return Ok(ExitCode::SUCCESS);
            }
            other => bail!("unknown arg: {other}"),
        }
    }
    if changed.is_empty() {
        bail!("--changed is required (comma-separated relative paths)");
    }
    let cfg = Config::default();
    let result = run(&root, &changed, &cfg)?;
    let feedback = format_diagnostics(&result.step_results);
    if result.ok {
        println!("OK: verification passed\n{feedback}");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("FAIL: verification found errors\n{feedback}");
        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(ExitCode::from(1))
    }
}
