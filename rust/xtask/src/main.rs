//! `cargo xtask <command>`: the developer commands, so CI and a local run do the same thing.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask <check | golden | screenshots | bench-stt <wav>>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("check") => check(),
        Some("golden") => golden(),
        Some("screenshots") => Err("screenshots arrive with the interface (phase 5)".into()),
        Some("bench-stt") => Err("bench-stt arrives with Lecture Assist (phase 7)".into()),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}

fn app_dir() -> PathBuf {
    workspace().join("crates/autopara-app")
}

/// Run `program args` in `dir`; a non-zero exit is an error naming the command.
fn run(dir: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    println!("> {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|error| format!("could not start {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed ({status})", args.join(" ")))
    }
}

fn npm() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

/// Every gate from the plan (R5), in the order that fails fastest.
fn check() -> Result<(), String> {
    let root = workspace();
    run(&root, "cargo", &["fmt", "--all", "--check"])?;
    run(
        &root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run(&root, "cargo", &["test", "--workspace"])?;
    bindings_are_committed(&root)?;
    let app = app_dir();
    for script in ["typecheck", "lint", "test"] {
        run(&app, npm(), &["run", script])?;
    }
    run(&root, "cargo", &["deny", "check"])
}

/// `cargo test` rewrites the TypeScript bindings; anything changed or new there was stale.
fn bindings_are_committed(root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args([
            "status",
            "--porcelain",
            "--",
            "crates/autopara-app/ui/src/bindings",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not start git: {error}"))?;
    let changes = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && changes.trim().is_empty() {
        Ok(())
    } else {
        Err(format!(
            "TypeScript bindings are out of date; commit them:\n{changes}"
        ))
    }
}

/// Compare the Rust parser and scheduler with the Python golden files.
fn golden() -> Result<(), String> {
    run(
        &workspace(),
        "cargo",
        &["test", "--package", "autopara-core", "--test", "golden"],
    )
}
