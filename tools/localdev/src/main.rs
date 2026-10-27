//! localdev: hand the next worker task in PLAN.md to a local OpenAI-compatible model, apply the
//! files it returns, run the task's checks, and commit, or revert after the last failed attempt.
//!
//! Usage, from anywhere in the repository: `localdev [TASK_ID]`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Config {
    base_url: String,
    model: String,
    max_context_tokens: usize,
    max_output_tokens: usize,
    temperature: f32,
    max_retries: usize,
    request_timeout_secs: u64,
    format: Format,
    plan: String,
    rules: String,
    verify_cwd: String,
    #[serde(default)]
    pre_verify: Vec<String>,
    #[serde(default)]
    post_verify: Vec<String>,
    error_lines: usize,
    #[serde(default)]
    env: BTreeMap<String, String>,
    /// Merged into every chat request, for server-specific options.
    #[serde(default)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Format {
    Files,
    Diff,
}

#[derive(Debug, Default)]
struct Task {
    id: String,
    title: String,
    done: bool,
    worker: bool,
    edit: Vec<String>,
    read: Vec<String>,
    result: String,
    verify: String,
    /// Overrides the configured reply format (`diff` suits large existing files).
    format: Option<Format>,
}

/// What one reply asked for, once applied.
struct Applied {
    commit: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("localdev: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let root = PathBuf::from(git(Path::new("."), &["rev-parse", "--show-toplevel"])?.trim());
    let config: Config = toml::from_str(&read(&root.join("localdev.toml"))?)
        .map_err(|e| format!("localdev.toml: {e}"))?;
    let plan_path = root.join(&config.plan);
    let plan = read(&plan_path)?;
    let tasks = parse_plan(&plan);
    let args: Vec<String> = std::env::args().skip(1).collect();
    // --dry-run writes the request to logs/ and stops, for tuning prompts.
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let wanted = args.iter().find(|a| !a.starts_with("--"));
    let task = match &wanted {
        Some(id) => tasks.iter().find(|t| &&t.id == id),
        None => tasks.iter().find(|t| t.worker && !t.done),
    }
    .ok_or("no such task, or no unchecked worker task left")?;
    if task.edit.is_empty() || task.verify.is_empty() {
        return Err(format!(
            "{} has no `edit` files or no `verify` command",
            task.id
        ));
    }
    for path in &task.edit {
        safe_path(&root, path)?;
    }
    let rules = read(&root.join(&config.rules))?;
    let config = Config {
        format: task.format.unwrap_or(config.format),
        ..config
    };
    let system = system_prompt(config.format, &rules);
    let budget = config
        .max_context_tokens
        .saturating_sub(config.max_output_tokens + tokens(&system));
    let first = task_prompt(&root, task, budget)?;
    let logs = root.join("tools/localdev/logs");
    fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    if dry_run {
        let request =
            json!({"system": system, "user": first, "tokens": tokens(&system) + tokens(&first)});
        let file = logs.join(format!("{}.request.json", task.id));
        fs::write(&file, request.to_string()).map_err(|e| e.to_string())?;
        println!("{}: request written to {}", task.id, file.display());
        return Ok(ExitCode::SUCCESS);
    }
    if !git(&root, &["status", "--porcelain"])?.trim().is_empty() {
        return Err("the working tree has changes; commit them first".into());
    }
    let mut log = format!("# {} {}\n\n## prompt\n\n{first}\n", task.id, task.title);

    let mut feedback: Option<(String, String)> = None;
    let mut last_error = String::new();
    for attempt in 1..=config.max_retries {
        let mut messages = vec![
            json!({"role": "system", "content": system}),
            json!({"role": "user", "content": first}),
        ];
        // Only the latest round goes back, so retries stay inside the context budget.
        if let Some((reply, error)) = &feedback {
            messages.push(json!({"role": "assistant", "content": reply}));
            messages.push(json!({"role": "user", "content": retry_prompt(config.format, error)}));
        }
        eprintln!("{}: attempt {attempt}/{}", task.id, config.max_retries);
        let (reply, thinking) = chat(&config, &messages)?;
        log.push_str(&format!("\n## reply {attempt}\n\n{reply}\n"));
        if reply.trim().len() < 40 {
            log.push_str(&format!("\n## reasoning tail {attempt}\n\n{thinking}\n"));
        }

        let outcome = apply(&config, &root, task, &reply)
            .and_then(|applied| verify(&config, &root, task).map(|out| (applied, out)));
        match outcome {
            Ok((applied, output)) => {
                fs::write(logs.join(format!("{}.md", task.id)), &log).map_err(|e| e.to_string())?;
                let ticked = plan.replacen(
                    &format!("- [ ] **{}**", task.id),
                    &format!("- [x] **{}**", task.id),
                    1,
                );
                fs::write(&plan_path, ticked).map_err(|e| e.to_string())?;
                let message = commit_message(applied.commit.as_deref(), &task.title);
                git(&root, &["add", "-A"])?;
                git(&root, &["commit", "-q", "-m", &message])?;
                report_success(&root, task, attempt, &config, &message, &output)?;
                return Ok(ExitCode::SUCCESS);
            }
            Err(error) => {
                log.push_str(&format!("\n## failure {attempt}\n\n{error}\n"));
                revert(&root)?;
                last_error = error.clone();
                feedback = Some((reply, error));
            }
        }
    }
    fs::write(logs.join(format!("{}.md", task.id)), &log).map_err(|e| e.to_string())?;
    println!(
        "{} {}: FAILED after {} attempts; working tree reverted to HEAD",
        task.id,
        title(&task.title),
        config.max_retries
    );
    println!("last error:");
    for line in last_error.lines().take(20) {
        println!("  {line}");
    }
    Ok(ExitCode::FAILURE)
}

// ---- PLAN.md ----

/// Tasks are `- [ ] **T02** Title (worker)` followed by `  - key: value` lines; values may wrap
/// onto deeper-indented lines. Paths and commands are the backticked parts.
fn parse_plan(text: &str) -> Vec<Task> {
    let mut tasks: Vec<Task> = Vec::new();
    let mut key = String::new();
    for line in text.lines() {
        let open = line.strip_prefix("- [ ] **").map(|rest| (false, rest));
        let header = open.or_else(|| line.strip_prefix("- [x] **").map(|rest| (true, rest)));
        if let Some((done, rest)) = header {
            let (id, title) = rest.split_once("**").unwrap_or((rest, ""));
            tasks.push(Task {
                id: id.to_string(),
                title: title.trim().to_string(),
                done,
                worker: title.contains("(worker)"),
                ..Task::default()
            });
            key.clear();
            continue;
        }
        let Some(task) = tasks.last_mut() else {
            continue;
        };
        if let Some(field) = line.strip_prefix("  - ") {
            let (k, v) = field.split_once(':').unwrap_or((field, ""));
            key = k.trim().to_string();
            add_field(task, &key, v);
        } else if line.starts_with("    ") && !key.is_empty() {
            add_field(task, &key, line);
        } else if !line.starts_with(' ') {
            key.clear();
        }
    }
    tasks
}

fn add_field(task: &mut Task, key: &str, value: &str) {
    match key {
        "edit" => task.edit.extend(backticked(value)),
        "read" => task.read.extend(backticked(value)),
        "verify" if task.verify.is_empty() => {
            task.verify = backticked(value).into_iter().next().unwrap_or_default()
        }
        "format" => {
            task.format = match backticked(value).first().map(String::as_str) {
                Some("diff") => Some(Format::Diff),
                Some("files") => Some(Format::Files),
                _ => None,
            }
        }
        "result" => {
            task.result.push(' ');
            task.result.push_str(value.trim());
        }
        _ => {}
    }
}

fn backticked(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn title(raw: &str) -> String {
    raw.replace("(worker)", "").trim().to_string()
}

// ---- prompts ----

fn system_prompt(format: Format, rules: &str) -> String {
    let shape = match format {
        Format::Files => {
            "For every file you change, write:\n\
             === FILE: <path relative to the repository root>\n\
             <the complete new content of the file>\n\
             === END FILE\n\
             Always write whole files, never fragments, placeholders or \"...\". \
             Only change files marked (editable)."
        }
        Format::Diff => {
            "Write one unified diff against the files exactly as shown:\n\
             === DIFF\n\
             --- a/<path>\n+++ b/<path>\n@@ ... @@\n<hunks>\n\
             === END DIFF\n\
             Only change files marked (editable)."
        }
    };
    format!(
        "You are a careful developer working in an existing repository. Follow the project \
         rules exactly.\n\nReply format (anything else is ignored):\n{shape}\n\
         Finish with one line:\n=== COMMIT: <short imperative commit message, under 60 \
         characters, describing the change>\n\n# Project rules\n\n{rules}"
    )
}

/// The task and its files. Editable files go in whole or the task is refused; read-only files
/// are cut to what is left of the budget.
fn task_prompt(root: &Path, task: &Task, budget: usize) -> Result<String, String> {
    let mut prompt = format!(
        "Task {}: {}\nExpected result:{}\nIt must pass: `{}` and clippy with -D warnings.\n",
        task.id,
        title(&task.title),
        task.result,
        task.verify
    );
    for path in &task.edit {
        let body = fs::read_to_string(root.join(path)).unwrap_or_else(|_| "(new file)".into());
        prompt.push_str(&format!("\n--- {path} (editable)\n{body}\n"));
    }
    if tokens(&prompt) > budget {
        return Err(format!(
            "{}: editable files alone exceed the context budget; split the task",
            task.id
        ));
    }
    for path in &task.read {
        let body = read(&root.join(path))?;
        let left = budget.saturating_sub(tokens(&prompt) + 50) * 3;
        let cut: String = body.chars().take(left).collect();
        let note = if cut.len() < body.len() {
            "\n[... truncated]"
        } else {
            ""
        };
        prompt.push_str(&format!("\n--- {path} (read-only)\n{cut}{note}\n"));
    }
    Ok(prompt)
}

fn retry_prompt(format: Format, error: &str) -> String {
    let again = match format {
        Format::Files => "Reply again in the same format with the full corrected files.",
        Format::Diff => "Write a new diff against the original files shown in the task.",
    };
    format!("That failed and your change was reverted.\n\n{error}\n\n{again}")
}

/// A rough count that errs high: code and Cyrillic run near three characters a token.
fn tokens(text: &str) -> usize {
    text.chars().count() / 3
}

// ---- the model ----

/// The reply, and the end of the model's hidden reasoning for the log.
fn chat(config: &Config, messages: &[Value]) -> Result<(String, String), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(config.request_timeout_secs)))
        .build()
        .into();
    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "temperature": config.temperature,
        "max_tokens": config.max_output_tokens,
        "stream": false,
    });
    if let Some(map) = body.as_object_mut() {
        map.extend(config.extra.clone());
    }
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut response = agent
        .post(&url)
        .send_json(&body)
        .map_err(|e| format!("{url}: {e}"))?;
    let value: Value = response
        .body_mut()
        .read_json()
        .map_err(|e| format!("{url}: unreadable reply: {e}"))?;
    let choice = &value["choices"][0];
    let content = choice["message"]["content"].as_str().unwrap_or_default();
    let reasoning = choice["message"]["reasoning_content"]
        .as_str()
        .unwrap_or_default();
    let skip = reasoning.chars().count().saturating_sub(3000);
    let thinking: String = reasoning.chars().skip(skip).collect();
    if choice["finish_reason"] == "length" {
        // Let the attempt fail visibly instead of applying a cut-off file.
        return Ok((
            format!("{content}\n(cut off at max_output_tokens)"),
            thinking,
        ));
    }
    Ok((content.to_string(), thinking))
}

// ---- applying ----

fn apply(config: &Config, root: &Path, task: &Task, reply: &str) -> Result<Applied, String> {
    let commit = reply
        .lines()
        .find_map(|l| l.trim().strip_prefix("=== COMMIT:"))
        .map(|m| m.trim().to_string());
    match config.format {
        Format::Files => {
            let files = file_blocks(reply)?;
            for (path, body) in &files {
                allowed(root, task, path)?;
                let target = root.join(path);
                if let Some(dir) = target.parent() {
                    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                fs::write(&target, body).map_err(|e| format!("{path}: {e}"))?;
            }
        }
        Format::Diff => {
            let start = reply
                .find("=== DIFF")
                .ok_or("no `=== DIFF` block in the reply")?;
            let rest = &reply[start + "=== DIFF".len()..];
            let end = rest
                .find("=== END DIFF")
                .ok_or("the diff has no `=== END DIFF`")?;
            let patch = strip_fence(&rest[..end]);
            for path in patch.lines().filter_map(|l| l.strip_prefix("+++ b/")) {
                allowed(root, task, path.trim())?;
            }
            let file = root.join("tools/localdev/logs/last.patch");
            fs::write(&file, format!("{patch}\n")).map_err(|e| e.to_string())?;
            let file = file.to_string_lossy().to_string();
            git(root, &["apply", "--recount", "--whitespace=nowarn", &file])?;
        }
    }
    Ok(Applied { commit })
}

/// `=== FILE: path` … `=== END FILE` blocks, tolerating a Markdown fence around the body.
fn file_blocks(reply: &str) -> Result<Vec<(String, String)>, String> {
    let mut files = Vec::new();
    let mut rest = reply;
    while let Some(start) = rest.find("=== FILE:") {
        let after = &rest[start + "=== FILE:".len()..];
        let (path, body) = after.split_once('\n').ok_or("a FILE header with no body")?;
        let end = body
            .find("=== END FILE")
            .ok_or_else(|| format!("{}: no `=== END FILE` (reply cut off?)", path.trim()))?;
        let mut content = strip_fence(&body[..end]).to_string();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        files.push((path.trim().trim_matches('`').to_string(), content));
        rest = &body[end..];
    }
    if files.is_empty() {
        return Err("no `=== FILE:` blocks in the reply".into());
    }
    Ok(files)
}

fn strip_fence(body: &str) -> &str {
    let trimmed = body.trim_matches('\n');
    match trimmed.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.split_once('\n').map_or("", |(_, r)| r);
            rest.trim_end()
                .strip_suffix("```")
                .unwrap_or(rest)
                .trim_end_matches('\n')
        }
        None => trimmed,
    }
}

/// Inside the repository, and one of the task's `edit` files.
fn allowed(root: &Path, task: &Task, path: &str) -> Result<(), String> {
    safe_path(root, path)?;
    let path = path.replace('\\', "/");
    if task.edit.iter().any(|e| e == &path) {
        Ok(())
    } else {
        Err(format!(
            "`{path}` is not an editable file of this task; edit only {:?}",
            task.edit
        ))
    }
}

fn safe_path(root: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    let plain = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if path.is_empty() || !plain {
        return Err(format!("refusing path outside the repository: {path}"));
    }
    Ok(root.join(relative))
}

// ---- verifying ----

/// Pre-steps may fail; the task's command and the post-steps must pass.
fn verify(config: &Config, root: &Path, task: &Task) -> Result<String, String> {
    let dir = root.join(&config.verify_cwd);
    for command in &config.pre_verify {
        let _ = shell(config, &dir, command);
    }
    let mut output = String::new();
    for command in std::iter::once(&task.verify).chain(&config.post_verify) {
        let (ok, out) = shell(config, &dir, command)?;
        if !ok {
            return Err(format!(
                "`{command}` failed:\n{}",
                trim_errors(&out, config.error_lines)
            ));
        }
        output.push_str(&out);
    }
    Ok(output)
}

fn shell(config: &Config, dir: &Path, command: &str) -> Result<(bool, String), String> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    for (key, value) in &config.env {
        let value = expand(value);
        if key == "PATH_PREPEND" {
            let path = std::env::var("PATH").unwrap_or_default();
            let sep = if cfg!(windows) { ";" } else { ":" };
            cmd.env("PATH", format!("{value}{sep}{path}"));
        } else {
            cmd.env(key, value);
        }
    }
    let out = cmd
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run `{command}`: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok((out.status.success(), text))
}

/// `${NAME}` from the environment; unknown names are left as they are.
fn expand(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                out.push_str(&std::env::var(name).unwrap_or_else(|_| format!("${{{name}}}")));
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// From the first compiler error or test failure on, else the tail.
fn trim_errors(output: &str, limit: usize) -> String {
    let lines: Vec<&str> = output
        .lines()
        .filter(|l| !l.trim_start().starts_with("Compiling ") && !l.trim().is_empty())
        .collect();
    let first = lines.iter().position(|l| {
        l.starts_with("error") || l.contains("panicked") || l.starts_with("failures:")
    });
    let start = first.unwrap_or(lines.len().saturating_sub(limit));
    lines[start..]
        .iter()
        .take(limit)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- git and reporting ----

fn revert(root: &Path) -> Result<(), String> {
    git(root, &["reset", "-q", "--hard", "HEAD"])?;
    // Not -x: build output and other ignored files stay.
    git(root, &["clean", "-q", "-fd"])?;
    Ok(())
}

/// The model's message if it reads like a person's, else the task title.
fn commit_message(proposed: Option<&str>, task_title: &str) -> String {
    const BANNED: &[&str] = &[
        "ai",
        "llm",
        "claude",
        "gpt",
        "assistant",
        "generated",
        "co-authored-by",
        "bot",
    ];
    let clean = |m: &str| {
        let lower = m.to_lowercase();
        !m.is_empty()
            && m.len() <= 72
            && !lower
                .split(|c: char| !c.is_alphanumeric() && c != '-')
                .any(|w| BANNED.contains(&w))
    };
    match proposed.map(|m| m.trim().trim_matches(['"', '`', '.'])) {
        Some(m) if clean(m) => m.to_string(),
        _ => {
            let t = title(task_title);
            let t = t.split(" (").next().unwrap_or(&t);
            format!("Add {}", t.to_lowercase())
        }
    }
}

fn report_success(
    root: &Path,
    task: &Task,
    attempt: usize,
    config: &Config,
    message: &str,
    output: &str,
) -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(
        out,
        "{} {}: done on attempt {attempt}/{}",
        task.id,
        title(&task.title),
        config.max_retries
    );
    let _ = writeln!(out, "commit: {message}");
    let _ = write!(
        out,
        "{}",
        git(root, &["show", "--stat", "--format=", "HEAD"])?
    );
    let results: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("test result:") || l.contains("Finished"))
        .collect();
    for line in results.iter().rev().take(3).rev() {
        let _ = writeln!(out, "  {}", line.trim());
    }
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "\
## A
- [x] **T00** Groundwork (lead)
  - edit: `a/**`
- [ ] **T02** SSE parser (worker)
  - edit: `rust/x/sse.rs`
  - read: `rust/x/mod.rs` (context),
    `rust/x/lib.rs`
  - result: one line
    and another.
  - verify: `cargo test -p x sse`
  - format: `diff`
";

    #[test]
    fn plan_tasks_are_parsed() {
        let tasks = parse_plan(PLAN);
        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].done && !tasks[0].worker);
        let t = &tasks[1];
        assert_eq!(t.id, "T02");
        assert!(t.worker && !t.done);
        assert_eq!(t.edit, ["rust/x/sse.rs"]);
        assert_eq!(t.read, ["rust/x/mod.rs", "rust/x/lib.rs"]);
        assert_eq!(t.result.trim(), "one line and another.");
        assert_eq!(t.verify, "cargo test -p x sse");
        assert!(t.format == Some(Format::Diff));
    }

    #[test]
    fn file_blocks_tolerate_fences() {
        let reply = "Sure.\n=== FILE: a/b.rs\n```rust\nfn a() {}\n```\n=== END FILE\n\
                     === FILE: c.rs\nx\n=== END FILE\n=== COMMIT: Add a\n";
        let files = file_blocks(reply).unwrap();
        assert_eq!(files[0], ("a/b.rs".into(), "fn a() {}\n".into()));
        assert_eq!(files[1], ("c.rs".into(), "x\n".into()));
        assert!(file_blocks("=== FILE: a.rs\ncut off").is_err());
    }

    #[test]
    fn paths_stay_inside_the_task() {
        let task = Task {
            edit: vec!["rust/x/sse.rs".into()],
            ..Task::default()
        };
        let root = Path::new(".");
        assert!(allowed(root, &task, "rust/x/sse.rs").is_ok());
        assert!(allowed(root, &task, "rust/x/../x/sse.rs").is_err());
        assert!(allowed(root, &task, "/etc/passwd").is_err());
        assert!(allowed(root, &task, "C:\\x").is_err());
        assert!(allowed(root, &task, "rust/x/other.rs").is_err());
    }

    #[test]
    fn commit_messages_read_like_a_person() {
        assert_eq!(
            commit_message(Some("Add an SSE parser."), "SSE parser (worker)"),
            "Add an SSE parser"
        );
        assert_eq!(
            commit_message(Some("AI: add parser"), "SSE parser (worker)"),
            "Add sse parser"
        );
        assert_eq!(
            commit_message(None, "Requests-per-minute guard (worker)"),
            "Add requests-per-minute guard"
        );
    }

    #[test]
    fn errors_start_at_the_first_error() {
        let out = "Compiling x\nwarning: w\nerror[E0308]: mismatched\n --> a.rs:1\nmore\n";
        assert_eq!(trim_errors(out, 2), "error[E0308]: mismatched\n --> a.rs:1");
    }

    #[test]
    fn variables_expand() {
        let path = std::env::var("PATH").unwrap_or_default();
        assert_eq!(expand("${PATH}/x"), format!("{path}/x"));
        assert_eq!(expand("${NO_SUCH_VAR_LOCALDEV}"), "${NO_SUCH_VAR_LOCALDEV}");
    }
}
