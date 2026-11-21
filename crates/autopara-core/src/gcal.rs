//! Google Calendar: sign-in (OAuth for installed apps: loopback redirect + PKCE), token refresh,
//! and reading the primary calendar's upcoming events as calls.
//!
//! The OAuth client comes from build-time env vars (AUTOPARA_GOOGLE_CLIENT_ID / _SECRET).
//! Without them the feature is simply not offered. For a "Desktop app" client Google does not
//! treat the secret as confidential; it still is kept out of the repo.

use chrono::{DateTime, Local, NaiveDate, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::models::{GCAL_KEY_PREFIX, Lesson};
use crate::normalize::{detect_provider, find_urls, pair_slot};

pub const CLIENT_ID: Option<&str> = option_env!("AUTOPARA_GOOGLE_CLIENT_ID");
pub const CLIENT_SECRET: Option<&str> = option_env!("AUTOPARA_GOOGLE_CLIENT_SECRET");

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const EVENTS_URL: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events.readonly";
const KEYRING_SERVICE: &str = "AutoPara";
const KEYRING_USER: &str = "google-calendar";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Error)]
pub enum GcalError {
    #[error("Google sign-in is not configured in this build")]
    NotConfigured,
    #[error("sign-in was cancelled")]
    Denied,
    #[error("sign-in timed out")]
    Timeout,
    #[error("Google access was revoked, sign in again")]
    Revoked,
    #[error("network: {0}")]
    Http(String),
    #[error("{0}")]
    Other(String),
}

pub fn is_configured() -> bool {
    CLIENT_ID.is_some_and(|s| !s.is_empty()) && CLIENT_SECRET.is_some_and(|s| !s.is_empty())
}

fn client() -> Result<(&'static str, &'static str), GcalError> {
    match (CLIENT_ID, CLIENT_SECRET) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Ok((id, secret)),
        _ => Err(GcalError::NotConfigured),
    }
}

// ------------------------------------------------------------------ token storage

fn keyring_entry() -> Result<keyring::Entry, GcalError> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| GcalError::Other(e.to_string()))
}

pub fn load_refresh_token() -> Option<String> {
    keyring_entry().ok()?.get_password().ok().filter(|s| !s.is_empty())
}

pub fn save_refresh_token(token: &str) -> Result<(), GcalError> {
    keyring_entry()?
        .set_password(token)
        .map_err(|e| GcalError::Other(e.to_string()))
}

pub fn clear_refresh_token() {
    if let Ok(e) = keyring_entry() {
        let _ = e.delete_credential();
    }
}

// ------------------------------------------------------------------ sign-in

/// Opens the browser at Google's consent page and waits (up to 5 minutes) for the redirect
/// back to a one-shot local listener. Returns the refresh token.
pub fn sign_in(open_browser: impl FnOnce(&str) -> bool) -> Result<String, GcalError> {
    let (client_id, client_secret) = client()?;
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| GcalError::Other(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| GcalError::Other(e.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = random_hex(32)?;
    let state = random_hex(16)?;
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));

    let url = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &code_challenge={challenge}&code_challenge_method=S256&state={state}\
         &access_type=offline&prompt=consent",
        enc(client_id),
        enc(&redirect_uri),
        enc(SCOPE),
    );
    if !open_browser(&url) {
        return Err(GcalError::Other("could not open the browser".into()));
    }

    let code = wait_for_code(&listener, &state)?;
    let body = post_form(&[
        ("code", code.as_str()),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
        ("code_verifier", verifier.as_str()),
    ])?;
    body.get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| GcalError::Other("Google returned no refresh token".into()))
}

fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String, GcalError> {
    listener
        .set_nonblocking(true)
        .map_err(|e| GcalError::Other(e.to_string()))?;
    let deadline = Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        if Instant::now() > deadline {
            return Err(GcalError::Timeout);
        }
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            Err(e) => return Err(GcalError::Other(e.to_string())),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");
        let params = parse_query(path);
        let get = |k: &str| params.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());

        // favicon and other stray requests: ignore and keep waiting
        if get("code").is_none() && get("error").is_none() {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            continue;
        }
        let ok = get("code").is_some() && get("state").as_deref() == Some(state);
        let page = if ok {
            "AutoPara підключено до Google Календаря. Цю вкладку можна закрити."
        } else {
            "Вхід не виконано. Поверніться в AutoPara і спробуйте ще раз."
        };
        let html = format!(
            "<!doctype html><meta charset=utf-8><title>AutoPara</title>\
             <body style=\"font:16px system-ui;padding:40px;background:#111;color:#eee\">{page}"
        );
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{html}",
            html.len()
        );
        if get("error").is_some() {
            return Err(GcalError::Denied);
        }
        if !ok {
            return Err(GcalError::Other("state mismatch".into()));
        }
        return Ok(get("code").unwrap_or_default());
    }
}

/// A fresh access token (valid ~1 hour) and when to refresh it.
pub fn access_token(refresh_token: &str) -> Result<(String, Instant), GcalError> {
    let (client_id, client_secret) = client()?;
    let body = post_form(&[
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "refresh_token"),
    ])?;
    let token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| GcalError::Other("no access token".into()))?;
    let secs = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
    // refresh a minute early
    let until = Instant::now() + Duration::from_secs(secs.saturating_sub(60));
    Ok((token.to_string(), until))
}

// ------------------------------------------------------------------ events

#[derive(Debug, Clone, PartialEq)]
pub struct CalEvent {
    pub id: String,
    pub summary: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub url: Option<String>,
}

/// Events of the primary calendar between `from` and `to`, recurring ones expanded.
/// Also returns the calendar's name (for the primary calendar, the account e-mail).
pub fn fetch_events(
    access_token: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(Option<String>, Vec<CalEvent>), GcalError> {
    let agent = agent();
    let mut events = Vec::new();
    let mut account = None;
    let mut page_token: Option<String> = None;
    for _ in 0..10 {
        let mut url = format!(
            "{EVENTS_URL}?singleEvents=true&orderBy=startTime&maxResults=250&timeMin={}&timeMax={}",
            enc(&from.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            enc(&to.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        );
        if let Some(t) = &page_token {
            url.push_str(&format!("&pageToken={}", enc(t)));
        }
        let mut resp = agent
            .get(&url)
            .header("Authorization", &format!("Bearer {access_token}"))
            .call()
            .map_err(|e| GcalError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| GcalError::Http(e.to_string()))?;
        if status == 401 {
            return Err(GcalError::Revoked);
        }
        if status != 200 {
            return Err(GcalError::Http(format!("Calendar API {status}")));
        }
        let json: Value =
            serde_json::from_str(&text).map_err(|e| GcalError::Other(e.to_string()))?;
        if account.is_none() {
            account = json.get("summary").and_then(Value::as_str).map(str::to_string);
        }
        events.extend(parse_events(&json));
        page_token = json
            .get("nextPageToken")
            .and_then(Value::as_str)
            .map(str::to_string);
        if page_token.is_none() {
            break;
        }
    }
    Ok((account, events))
}

/// Timed, not cancelled, not declined events. All-day events have no time to open at: skipped.
pub fn parse_events(json: &Value) -> Vec<CalEvent> {
    let Some(items) = json.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|it| {
            if it.get("status").and_then(Value::as_str) == Some("cancelled") {
                return None;
            }
            let declined = it
                .get("attendees")
                .and_then(Value::as_array)
                .is_some_and(|a| {
                    a.iter().any(|p| {
                        p.get("self").and_then(Value::as_bool) == Some(true)
                            && p.get("responseStatus").and_then(Value::as_str) == Some("declined")
                    })
                });
            if declined {
                return None;
            }
            let time = |k: &str| {
                it.get(k)
                    .and_then(|v| v.get("dateTime"))
                    .and_then(Value::as_str)
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&Local))
            };
            Some(CalEvent {
                id: it.get("id").and_then(Value::as_str)?.to_string(),
                summary: it
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                start: time("start")?,
                end: time("end")?,
                url: meeting_url(it),
            })
        })
        .collect()
}

/// Meet link first, then any video entry point (Zoom/Teams add-ons), then a link in
/// the location or the description.
fn meeting_url(it: &Value) -> Option<String> {
    if let Some(u) = it.get("hangoutLink").and_then(Value::as_str) {
        return Some(u.to_string());
    }
    let video = it
        .get("conferenceData")
        .and_then(|c| c.get("entryPoints"))
        .and_then(Value::as_array)
        .and_then(|eps| {
            eps.iter()
                .find(|e| e.get("entryPointType").and_then(Value::as_str) == Some("video"))
        })
        .and_then(|e| e.get("uri"))
        .and_then(Value::as_str);
    if let Some(u) = video {
        return Some(u.to_string());
    }
    ["location", "description"].iter().find_map(|k| {
        let text = it.get(*k).and_then(Value::as_str)?;
        find_urls(text)
            .into_iter()
            .find(|u| detect_provider(Some(u)) != "unknown")
    })
}

/// The call AutoPara stores for an event. `course_id` is filled in by storage.
pub fn to_lesson(ev: &CalEvent, untitled: &str) -> Lesson {
    let start = ev.start.format("%H:%M").to_string();
    // a meeting running past midnight ends at the end of its first day here
    let end = if ev.end.date_naive() == ev.start.date_naive() {
        ev.end.format("%H:%M").to_string()
    } else {
        "23:59".to_string()
    };
    let on_date: NaiveDate = ev.start.date_naive();
    Lesson {
        id: 0,
        course_id: 0,
        day_index: 0,
        pair: pair_slot(&start),
        start_time: start,
        end_time: end,
        subject: if ev.summary.is_empty() {
            untitled.to_string()
        } else {
            ev.summary.clone()
        },
        teacher: String::new(),
        needs_link: ev.url.is_none(),
        provider: detect_provider(ev.url.as_deref()),
        url: ev.url.clone(),
        is_manual: true,
        source_key: format!("{GCAL_KEY_PREFIX}{}", ev.id),
        group_names: Vec::new(),
        on_date: Some(on_date),
        repeat_until: None,
        skip_dates: BTreeSet::new(),
    }
}

// ------------------------------------------------------------------ helpers

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

fn post_form(fields: &[(&str, &str)]) -> Result<Value, GcalError> {
    let mut resp = agent()
        .post(TOKEN_URL)
        .send_form(fields.iter().copied())
        .map_err(|e| GcalError::Http(e.to_string()))?;
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| GcalError::Http(e.to_string()))?;
    let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status == 400 || status == 401 {
        // invalid_grant = the refresh token was revoked or expired
        if json.get("error").and_then(Value::as_str) == Some("invalid_grant") {
            return Err(GcalError::Revoked);
        }
        return Err(GcalError::Other(format!("Google: {text}")));
    }
    if status != 200 {
        return Err(GcalError::Http(format!("token endpoint {status}")));
    }
    Ok(json)
}

fn random_hex(bytes: usize) -> Result<String, GcalError> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| GcalError::Other(e.to_string()))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn base64url(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | (u32::from(b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn dec(s: &str) -> String {
    let bytes = s.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(path: &str) -> Vec<(String, String)> {
    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (dec(k), dec(v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base64url_matches_rfc7636_example() {
        // RFC 7636 appendix B
        let challenge = base64url(&Sha256::digest(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn query_is_decoded() {
        let q = parse_query("/?state=ab&code=4%2F0Adeu5B&scope=a+b");
        assert_eq!(q[1], ("code".into(), "4/0Adeu5B".into()));
        assert_eq!(q[2], ("scope".into(), "a b".into()));
        assert_eq!(dec("100%"), "100%");
    }

    #[test]
    fn events_are_filtered_and_linked() {
        let data = json!({"items": [
            {"id": "a", "summary": "Standup", "hangoutLink": "https://meet.google.com/abc-defg-hij",
             "start": {"dateTime": "2026-03-02T09:00:00+02:00"},
             "end": {"dateTime": "2026-03-02T09:30:00+02:00"}},
            {"id": "b", "status": "cancelled",
             "start": {"dateTime": "2026-03-02T10:00:00+02:00"},
             "end": {"dateTime": "2026-03-02T11:00:00+02:00"}},
            {"id": "c", "summary": "Holiday", "start": {"date": "2026-03-03"}, "end": {"date": "2026-03-04"}},
            {"id": "d", "summary": "Declined",
             "attendees": [{"self": true, "responseStatus": "declined"}],
             "start": {"dateTime": "2026-03-02T12:00:00+02:00"},
             "end": {"dateTime": "2026-03-02T13:00:00+02:00"}},
            {"id": "e", "summary": "Zoom sync", "description": "Join: <a href=\"https://us02web.zoom.us/j/123\">here</a>",
             "start": {"dateTime": "2026-03-02T14:00:00Z"},
             "end": {"dateTime": "2026-03-02T15:00:00Z"}}
        ]});
        let evs = parse_events(&data);
        let ids: Vec<_> = evs.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["a", "e"]);
        assert_eq!(evs[0].url.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert_eq!(evs[1].url.as_deref(), Some("https://us02web.zoom.us/j/123"));

        let l = to_lesson(&evs[0], "(no title)");
        assert_eq!(l.source_key, "gcal:a");
        assert_eq!(l.provider, "google_meet");
        assert!(l.is_from_calendar());
    }
}
