//! GitHub Releases auto-updater matching original AutoPara security invariants:
//! - HTTPS-only against `github.com` and `*.githubusercontent.com` (verified on every redirect).
//! - Release assets must live under `<GITHUB_REPO>/releases/download/` with no path traversal.
//! - Verifies downloaded byte size and SHA-256 digest (when published) before renaming `.part`.
//! - Never installs silently without an explicit user click, and never executes outside `updates/`.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

use crate::storage::default_db_path;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Override at build time via `AUTOPARA_GITHUB_REPO=owner/repo` if publishing from a fork.
pub const GITHUB_REPO: &str = match option_env!("AUTOPARA_GITHUB_REPO") {
    Some(r) => r,
    None => "reyjendiz/AutoPara",
};

const MAX_METADATA_BYTES: usize = 2_000_000;
const CHUNK_SIZE: usize = 64 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub page_url: String,
    pub asset: Option<Asset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPlatform {
    Windows,
    MacArm64,
    MacIntel,
    Other,
}

impl TargetPlatform {
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Self::MacArm64
        }
        #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
        {
            Self::MacIntel
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Self::Other
        }
    }

    pub fn installable(self) -> bool {
        matches!(self, Self::Windows | Self::MacArm64 | Self::MacIntel)
    }

    pub fn restarts_on_apply(self) -> bool {
        matches!(self, Self::Windows)
    }
}

pub fn api_url() -> String {
    format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest")
}

pub fn page_prefix() -> String {
    format!("https://github.com/{GITHUB_REPO}/")
}

pub fn download_prefix() -> String {
    format!("https://github.com/{GITHUB_REPO}/releases/download/")
}

pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let trimmed = text.trim();
    let stripped = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let mut parts = stripped.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(new_v), Some(old_v)) => new_v > old_v,
        _ => false,
    }
}

/// HTTPS only, strictly on `github.com` or `*.githubusercontent.com`.
pub fn is_trusted_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    if authority.contains('@') || authority.is_empty() {
        return false;
    }
    let host = authority
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host == "github.com"
        || host.ends_with(".github.com")
        || host == "githubusercontent.com"
        || host.ends_with(".githubusercontent.com")
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && Path::new(name).file_name().and_then(|s| s.to_str()) == Some(name)
}

fn is_windows_setup(name: &str) -> bool {
    name.starts_with("AutoPara-") && name.ends_with("-Setup.exe")
}

fn is_windows_standalone(name: &str) -> bool {
    name.starts_with("AutoPara-") && name.ends_with(".exe") && !name.ends_with("-Setup.exe")
}

fn is_mac_dmg(name: &str) -> bool {
    name.starts_with("AutoPara") && name.ends_with(".dmg")
}

fn is_intel_image(name: &str) -> bool {
    name.to_ascii_lowercase().contains("intel")
}

fn parse_sha256_digest(digest: Option<&str>) -> Option<String> {
    let raw = digest?.trim();
    let lower = raw.to_ascii_lowercase();
    lower.strip_prefix("sha256:").map(|s| s.to_string())
}

fn pick_asset_from(assets: &[GhAsset], platform: TargetPlatform) -> Option<Asset> {
    let dl_prefix = download_prefix();
    let make_asset = |a: &GhAsset| -> Asset {
        Asset {
            name: a.name.clone(),
            url: a.browser_download_url.clone(),
            size: a.size,
            sha256: parse_sha256_digest(a.digest.as_deref()),
        }
    };

    match platform {
        TargetPlatform::Windows => {
            // Prefer NSIS installer (`AutoPara-X.Y.Z-Setup.exe`) like original; fall back to
            // standalone single-file exe (`AutoPara-X.Y.Z-Windows.exe` / `AutoPara-X.Y.Z.exe`).
            for matcher in [is_windows_setup as fn(&str) -> bool, is_windows_standalone] {
                for entry in assets {
                    if matcher(&entry.name)
                        && is_safe_filename(&entry.name)
                        && entry.browser_download_url.starts_with(&dl_prefix)
                        && is_trusted_url(&entry.browser_download_url)
                    {
                        return Some(make_asset(entry));
                    }
                }
            }
            None
        }
        TargetPlatform::MacArm64 | TargetPlatform::MacIntel => {
            let want_intel = platform == TargetPlatform::MacIntel;
            for entry in assets {
                if is_mac_dmg(&entry.name)
                    && is_safe_filename(&entry.name)
                    && is_intel_image(&entry.name) == want_intel
                    && entry.browser_download_url.starts_with(&dl_prefix)
                    && is_trusted_url(&entry.browser_download_url)
                {
                    return Some(make_asset(entry));
                }
            }
            None
        }
        TargetPlatform::Other => None,
    }
}

pub fn parse_release_json(json_text: &str, platform: TargetPlatform) -> Option<Release> {
    let payload: GhRelease = serde_json::from_str(json_text).ok()?;
    if payload.draft || payload.prerelease {
        return None;
    }
    let (maj, min, pat) = parse_version(&payload.tag_name)?;
    let page_url = if payload.html_url.starts_with(&page_prefix()) {
        payload.html_url
    } else {
        String::new()
    };
    Some(Release {
        version: format!("{maj}.{min}.{pat}"),
        tag: payload.tag_name,
        notes: payload.body.unwrap_or_default().trim().to_string(),
        page_url,
        asset: pick_asset_from(&payload.assets, platform),
    })
}

/// First non-empty plain text line from release notes (<= 140 chars), matching original banner.
pub fn summary_line(notes: &str) -> String {
    for line in notes.lines() {
        let cleaned = line
            .trim()
            .trim_start_matches(['#', '*', '-', ' '])
            .trim();
        if !cleaned.is_empty() {
            return cleaned.chars().take(140).collect();
        }
    }
    String::new()
}

fn http_agent(timeout_secs: u64) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

fn get_trusted(
    url: &str,
    accept: &str,
    timeout_secs: u64,
) -> Result<ureq::http::Response<ureq::Body>, UpdateError> {
    let agent = http_agent(timeout_secs);
    let mut current = url.to_string();
    let ua = format!("AutoPara/{CURRENT_VERSION}");

    for _ in 0..=MAX_REDIRECTS {
        if !is_trusted_url(&current) {
            return Err(UpdateError::Message(format!(
                "refused untrusted URL: {current}"
            )));
        }
        let resp = agent
            .get(&current)
            .header("Accept", accept)
            .header("User-Agent", &ua)
            .call()
            .map_err(|e| UpdateError::Message(format!("could not reach GitHub: {e}")))?;

        let status = resp.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let loc = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| UpdateError::Message("redirect without Location".into()))?;
            if !is_trusted_url(loc) {
                return Err(UpdateError::Message(format!(
                    "refused redirect to untrusted host: {loc}"
                )));
            }
            current = loc.to_string();
            continue;
        }
        return Ok(resp);
    }

    Err(UpdateError::Message("too many redirects".into()))
}

/// Fetch the latest published GitHub release, or `Ok(None)` if no release exists yet.
pub fn fetch_latest(platform: TargetPlatform) -> Result<Option<Release>, UpdateError> {
    let resp = get_trusted(&api_url(), "application/vnd.github+json", 12)?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(UpdateError::Message(format!("GitHub answered HTTP {status}")));
    }

    let mut raw = Vec::new();
    resp.into_body()
        .into_reader()
        .take((MAX_METADATA_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|e| UpdateError::Message(format!("failed reading GitHub response: {e}")))?;
    if raw.len() > MAX_METADATA_BYTES {
        return Err(UpdateError::Message("release metadata too large".into()));
    }
    let text = String::from_utf8(raw)
        .map_err(|_| UpdateError::Message("invalid UTF-8 from GitHub".into()))?;
    Ok(parse_release_json(&text, platform))
}

pub fn updates_dir(db_path: Option<&Path>) -> PathBuf {
    let base = db_path
        .map(PathBuf::from)
        .unwrap_or_else(default_db_path);
    let dir = base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("updates");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verifies(path: &Path, asset: &Asset) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if asset.size > 0 && meta.len() != asset.size {
        return false;
    }
    match &asset.sha256 {
        None => true,
        Some(expected) => sha256_of(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected)),
    }
}

/// Download `asset` into `directory` via `<name>.part`, verify size & SHA-256, then atomically rename.
pub fn download<F>(
    asset: &Asset,
    directory: &Path,
    mut on_progress: F,
) -> Result<PathBuf, UpdateError>
where
    F: FnMut(u64, u64),
{
    if !asset.url.starts_with(&download_prefix())
        || !is_trusted_url(&asset.url)
        || !is_safe_filename(&asset.name)
    {
        return Err(UpdateError::Message(
            "asset does not belong to this project's GitHub releases".into(),
        ));
    }

    fs::create_dir_all(directory)
        .map_err(|e| UpdateError::Message(format!("cannot create updates folder: {e}")))?;

    let target = directory.join(&asset.name);
    if verifies(&target, asset) {
        on_progress(asset.size, asset.size);
        return Ok(target);
    }

    let part = directory.join(format!("{}.part", asset.name));
    let resp = get_trusted(&asset.url, "application/octet-stream", 120)?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(UpdateError::Message(format!("download answered HTTP {status}")));
    }

    let header_len = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let total = if header_len > 0 { header_len } else { asset.size };

    let mut reader = resp.into_body().into_reader();
    let mut out = File::create(&part)
        .map_err(|e| UpdateError::Message(format!("cannot create part file: {e}")))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut buf = [0u8; CHUNK_SIZE];

    let copy_res = (|| -> Result<(), UpdateError> {
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| UpdateError::Message(format!("download interrupted: {e}")))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| UpdateError::Message(format!("disk write error: {e}")))?;
            hasher.update(&buf[..n]);
            received += n as u64;
            on_progress(received, total);
        }
        out.flush()
            .map_err(|e| UpdateError::Message(format!("disk flush error: {e}")))?;
        Ok(())
    })();

    drop(out);
    if let Err(err) = copy_res {
        let _ = fs::remove_file(&part);
        return Err(err);
    }

    if asset.size > 0 && received != asset.size {
        let _ = fs::remove_file(&part);
        return Err(UpdateError::Message(format!(
            "expected {} bytes, received {received}",
            asset.size
        )));
    }
    if let Some(expected) = &asset.sha256 {
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&part);
            return Err(UpdateError::Message(
                "downloaded file failed SHA-256 verification".into(),
            ));
        }
    }

    if target.exists() {
        let _ = fs::remove_file(&target);
    }
    fs::rename(&part, &target)
        .map_err(|e| UpdateError::Message(format!("cannot finalize update file: {e}")))?;
    Ok(target)
}

fn extract_semver_in_name(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    for i in 0..bytes.len() {
        if !bytes[i].is_ascii_digit() {
            continue;
        }
        let rest = &name[i..];
        let mut dots = 0;
        let mut end = 0;
        for (idx, ch) in rest.char_indices() {
            if ch.is_ascii_digit() {
                end = idx + 1;
            } else if ch == '.' && dots < 2 {
                dots += 1;
                end = idx + 1;
            } else {
                break;
            }
        }
        if dots == 2 {
            let cand = &rest[..end];
            if parse_version(cand).is_some() {
                return Some(cand.to_string());
            }
        }
    }
    None
}

/// Remove `.part` leftovers and installers for versions <= `current`.
pub fn tidy(directory: &Path, current: &str) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("part") {
            let _ = fs::remove_file(&path);
            continue;
        }
        if let Some(fname) = path.file_name().and_then(|s| s.to_str())
            && let Some(ver) = extract_semver_in_name(fname)
            && !is_newer(&ver, current)
        {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Launch a verified downloaded update from `directory`.
/// Returns `Ok(true)` when the app should immediately exit so Windows can replace the running binary.
pub fn apply(installer: &Path, directory: &Path) -> Result<bool, UpdateError> {
    let canonical_file = fs::canonicalize(installer)
        .map_err(|e| UpdateError::Message(format!("update file missing: {e}")))?;
    let canonical_dir = fs::canonicalize(directory)
        .map_err(|e| UpdateError::Message(format!("updates dir missing: {e}")))?;
    if !canonical_file.starts_with(&canonical_dir) {
        return Err(UpdateError::Message(
            "refusing to run update file outside updates folder".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

        let exe_path = std::env::current_exe()
            .map_err(|e| UpdateError::Message(format!("cannot locate current exe: {e}")))?;
        let fname = canonical_file
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let inst_str = canonical_file.display().to_string();
        let inst_clean = inst_str.strip_prefix(r"\\?\").unwrap_or(&inst_str);
        let prog_str = exe_path.display().to_string();
        let prog_clean = prog_str.strip_prefix(r"\\?\").unwrap_or(&prog_str);

        let cmdline = if is_windows_setup(fname) {
            format!(
                "\"ping -n 3 127.0.0.1 >nul & \"{inst_clean}\" /S & start \"\" \"{prog_clean}\"\""
            )
        } else {
            format!(
                "\"ping -n 3 127.0.0.1 >nul & move /Y \"{inst_clean}\" \"{prog_clean}\" & start \"\" \"{prog_clean}\"\""
            )
        };

        std::process::Command::new("cmd")
            .arg("/S")
            .arg("/C")
            .raw_arg(&cmdline)
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|e| UpdateError::Message(format!("could not start updater: {e}")))?;
        Ok(true)
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&canonical_file)
            .spawn()
            .map_err(|e| UpdateError::Message(format!("could not open disk image: {e}")))?;
        Ok(false)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err(UpdateError::Message("auto-apply unsupported on this OS".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_versions() {
        assert_eq!(parse_version("v2.1.0"), Some((2, 1, 0)));
        assert_eq!(parse_version("2.0.10"), Some((2, 0, 10)));
        assert_eq!(parse_version("2.0"), None);
        assert!(is_newer("2.0.1", "2.0.0"));
        assert!(is_newer("v2.10.0", "2.9.9"));
        assert!(!is_newer("2.0.0", "2.0.0"));
        assert!(!is_newer("1.9.9", "2.0.0"));
    }

    #[test]
    fn enforces_trusted_github_hosts() {
        assert!(is_trusted_url("https://api.github.com/repos/reyjendiz/AutoPara/releases/latest"));
        assert!(is_trusted_url("https://objects.githubusercontent.com/file.dmg"));
        assert!(!is_trusted_url("http://github.com/reyjendiz/AutoPara"));
        assert!(!is_trusted_url("https://evil-github.com/file.exe"));
        assert!(!is_trusted_url("https://github.com@evil.com/file.exe"));
    }

    #[test]
    fn picks_right_platform_and_silicon_asset() {
        let dl = download_prefix();
        let json = format!(
            r###"{{
                "tag_name": "v2.1.0",
                "html_url": "{page}releases/tag/v2.1.0",
                "body": "## What's new\n- Faster calendar",
                "assets": [
                    {{"name": "AutoPara-2.1.0-Windows.exe", "browser_download_url": "{dl}v2.1.0/AutoPara-2.1.0-Windows.exe", "size": 9000}},
                    {{"name": "AutoPara-2.1.0.dmg", "browser_download_url": "{dl}v2.1.0/AutoPara-2.1.0.dmg", "size": 8000, "digest": "sha256:abcd"}},
                    {{"name": "AutoPara-Intel-2.1.0.dmg", "browser_download_url": "{dl}v2.1.0/AutoPara-Intel-2.1.0.dmg", "size": 8100}}
                ]
            }}"###,
            page = page_prefix(),
        );

        let win = parse_release_json(&json, TargetPlatform::Windows).unwrap();
        assert_eq!(win.version, "2.1.0");
        assert_eq!(win.asset.unwrap().name, "AutoPara-2.1.0-Windows.exe");

        let arm = parse_release_json(&json, TargetPlatform::MacArm64).unwrap();
        let arm_asset = arm.asset.unwrap();
        assert_eq!(arm_asset.name, "AutoPara-2.1.0.dmg");
        assert_eq!(arm_asset.sha256.as_deref(), Some("abcd"));

        let intel = parse_release_json(&json, TargetPlatform::MacIntel).unwrap();
        assert_eq!(intel.asset.unwrap().name, "AutoPara-Intel-2.1.0.dmg");
    }

    #[test]
    fn verifies_and_tidies_downloads() {
        let dir = std::env::temp_dir().join(format!("autopara_upd_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let f_old = dir.join("AutoPara-2.0.0.dmg");
        let f_new = dir.join("AutoPara-2.1.0.dmg");
        let f_part = dir.join("AutoPara-2.2.0.dmg.part");
        fs::write(&f_old, b"old").unwrap();
        fs::write(&f_new, b"hello").unwrap();
        fs::write(&f_part, b"partial").unwrap();

        let digest = sha256_of(&f_new).unwrap();
        let good = Asset {
            name: "AutoPara-2.1.0.dmg".into(),
            url: String::new(),
            size: 5,
            sha256: Some(digest),
        };
        let bad_sha = Asset {
            sha256: Some("0000".into()),
            ..good.clone()
        };
        assert!(verifies(&f_new, &good));
        assert!(!verifies(&f_new, &bad_sha));

        tidy(&dir, "2.0.0");
        assert!(!f_old.exists());
        assert!(!f_part.exists());
        assert!(f_new.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
