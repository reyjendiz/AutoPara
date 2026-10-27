//! `cargo xtask <command>`: the developer commands, so CI and a local run do the same thing.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str =
    "usage: cargo xtask <check | golden | demo-data | screenshots | bench-stt <wav>>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("check") => check(),
        Some("golden") => golden(),
        Some("demo-data") => demo_data(),
        Some("screenshots") => screenshots(),
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

/// The sample timetable as the interface's demo backend serves it (`ui/src/demo/sample.json`):
/// parsed and stored by the real core, so the demo shows exactly what the app would.
fn demo_data() -> Result<(), String> {
    use autopara_core::db::{Storage, parse_datetime};
    use autopara_core::parser::parse_file_default;
    use serde_json::json;

    let root = workspace();
    let sample = root.join("fixtures/golden/sample_timetable.docx");
    let fail = |error: &dyn std::fmt::Display| error.to_string();
    let courses = parse_file_default(&sample).map_err(|e| fail(&e))?;
    let mut storage = Storage::open_in_memory().map_err(|e| fail(&e))?;
    let imported = parse_datetime("2026-01-01T00:00:00").ok_or("bad time")?;
    storage
        .import_courses(&courses, imported)
        .map_err(|e| fail(&e))?;
    let first = storage
        .courses()
        .map_err(|e| fail(&e))?
        .into_iter()
        .next()
        .ok_or("no course")?;
    let group = storage
        .groups(first.id)
        .map_err(|e| fail(&e))?
        .into_iter()
        .next()
        .ok_or("no group")?;
    storage
        .set_setting("selected_course_id", &first.id.to_string())
        .map_err(|e| fail(&e))?;
    storage
        .set_setting("selected_group_id", &group.id.to_string())
        .map_err(|e| fail(&e))?;

    let mut summaries = Vec::new();
    let mut lessons = serde_json::Map::new();
    for course in storage.courses().map_err(|e| fail(&e))? {
        let mut groups = Vec::new();
        for group in storage.groups(course.id).map_err(|e| fail(&e))? {
            let of_group = storage.lessons_for_group(group.id).map_err(|e| fail(&e))?;
            groups.push(json!({ "group": group, "lessons": of_group.len() }));
            lessons.insert(group.id.to_string(), json!(of_group));
        }
        summaries.push(json!({ "course": course, "groups": groups }));
    }
    let data = json!({
        "settings": storage.settings().map_err(|e| fail(&e))?,
        "courses": summaries,
        "lessons": lessons,
    });
    let path = root.join("crates/autopara-app/ui/src/demo/sample.json");
    std::fs::create_dir_all(path.parent().ok_or("no parent")?).map_err(|e| fail(&e))?;
    let text = serde_json::to_string_pretty(&data).map_err(|e| fail(&e))?;
    std::fs::write(
        &path,
        text + "
",
    )
    .map_err(|e| fail(&e))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// The reference pictures (PORT_SPEC §K), rendered from the demo backend — the sample timetable
/// with the clock pinned to Monday 2026-03-02 09:40:20 — by a headless browser at 1240×800, into
/// `target/screenshots`. Compare them with `docs/*.png`; the differences are in `PARITY_UI.md`.
fn screenshots() -> Result<(), String> {
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    const PORT: u16 = 4173;
    const SHOTS: [(&str, &str); 6] = [
        ("week-grid", "theme=light&view=week"),
        ("week-grid-dark", "theme=dark&view=week"),
        ("month-view", "theme=light&view=month"),
        ("day-view", "theme=light&view=day"),
        ("edit-dialog", "theme=light&view=week&dialog=edit"),
        ("settings", "theme=light&view=week&dialog=settings"),
    ];

    let browser = browser().ok_or("no Edge or Chrome found for the headless screenshots")?;
    let app = app_dir();
    run(&app, npm(), &["run", "build"])?;
    let out = workspace().join("target/screenshots");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;

    // Vite's own script under node, not through npx: killing a wrapper would leave the server up.
    let mut server = Command::new("node")
        .args([
            "node_modules/vite/bin/vite.js",
            "preview",
            "--host",
            "127.0.0.1",
            "--port",
            &PORT.to_string(),
            "--strictPort",
        ])
        .current_dir(&app)
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start vite preview: {e}"))?;
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", PORT)).is_err() {
        if started.elapsed() > Duration::from_secs(30) {
            let _ = server.kill();
            return Err("vite preview did not start".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let mut result = Ok(());
    for (name, query) in SHOTS {
        let file = out.join(format!("{name}.png"));
        let url = format!("http://127.0.0.1:{PORT}/?demo&font=inter&still&{query}");
        let shot = format!("--screenshot={}", file.display());
        // A profile of its own: without it the browser hands the job to a copy already running.
        let profile = format!("--user-data-dir={}", out.join(".profile").display());
        let status = Command::new(&browser)
            .args([
                "--headless=new",
                "--no-first-run",
                &profile,
                "--disable-gpu",
                "--hide-scrollbars",
                "--force-device-scale-factor=1",
                "--window-size=1240,800",
                "--virtual-time-budget=4000",
                &shot,
                &url,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match status {
            Ok(status) if status.success() && file.is_file() => {
                println!("wrote {}", file.display())
            }
            Ok(status) => result = Err(format!("{name}: the browser exited with {status}")),
            Err(error) => result = Err(format!("{name}: {error}")),
        }
    }
    let _ = server.kill();
    result
}

/// A Chromium browser for headless screenshots: Edge (always on Windows) or Chrome.
fn browser() -> Option<PathBuf> {
    [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}
