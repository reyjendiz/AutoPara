//! The "Відкрита пара" tab: a class's Google Meet or Zoom page inside the main window, as a second
//! webview over the area the interface keeps for it (`ui/src/ui/meeting.ts`).
//!
//! The page is remote. Tauri refuses every command from a remote origin unless a capability names
//! that origin, and none does, so the page has no way into the app.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use lecture_assist::AssistError;
use lecture_assist::audio::AudioSource;

use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, State, Url,
    WebviewBuilder, WebviewUrl, webview::PageLoadEvent,
};
use tauri_plugin_opener::OpenerExt;

use crate::commands::{AppState, CommandResult, failed};

const LABEL: &str = "meeting";
/// Tells the interface a class page opened (payload: its site), so it switches to the tab.
const OPENED_EVENT: &str = "meeting-opened";
const GOOGLE_SIGN_IN: &str =
    "https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmeet.google.com%2F";
const ZOOM_SIGN_IN: &str = "https://zoom.us/signin";
/// What the interface last said about the tab, and the page it shows.
pub struct Meeting {
    place: Mutex<Place>,
    /// The page's volume from the slider (0..=1, as `f32` bits), shared with the assistant.
    volume: Arc<AtomicU32>,
    /// The `open_in_app` setting, kept here because links open while the storage is locked.
    in_app: AtomicBool,
}

impl Default for Meeting {
    fn default() -> Self {
        Self {
            place: Mutex::default(),
            volume: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            in_app: AtomicBool::new(true),
        }
    }
}

#[derive(Default)]
struct Place {
    bounds: (f64, f64, f64, f64),
    /// The interface's border and background colours, which the page's corners are painted in.
    colours: (String, String),
    page: Option<Url>,
}

/// A webview is always a rectangle (and `unsafe` window regions are not allowed), so the page gets
/// one style rule: a click-through ring fixed to its viewport that paints the tab's rounded border
/// and the background beyond it over the page's corners. It holds however the site scrolls.
fn corners(border: &str, background: &str) -> String {
    let rule = format!(
        "html::after{{content:'';position:fixed;inset:0;border-radius:13px;pointer-events:none;\
         z-index:2147483647;box-shadow:0 0 0 1px {border},0 0 0 40px {background}}}"
    );
    format!(
        r#"(() => {{
  const put = () => {{
    let style = document.getElementById("autopara-corners");
    if (!style) {{
      style = document.createElement("style");
      style.id = "autopara-corners";
      document.documentElement.append(style);
    }}
    style.textContent = "{rule}";
  }};
  if (document.documentElement) put();
  else document.addEventListener("readystatechange", put, {{ once: true }});
}})();"#
    )
}

/// `join.js`: camera and microphone off on every class page, and with "Входити в пару одразу"
/// (`join`) the join itself. The guest name goes in as a JSON string, which is also a JavaScript
/// string literal, so no name can break out of it.
fn join_script(name: &str, join: bool) -> String {
    let literal = serde_json::to_string(name).unwrap_or_else(|_| "\"\"".to_owned());
    include_str!("join.js")
        .replace("__NAME__", &literal)
        .replace("__JOIN__", if join { "true" } else { "false" })
}

/// Tells the page (`volume.js`) its volume.
fn volume_call(volume: f32) -> String {
    format!("window.__autoparaVolume?.({volume})")
}

/// A colour as the interface's computed style gives it (`rgb(17, 17, 19)`): nothing that could
/// end the string or the rule it goes into.
fn is_colour(value: &str) -> bool {
    value.len() < 40
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "(),.% #".contains(c))
}

/// The page AutoPara shows for a class link, or `None` when the link belongs in the browser:
/// Meet and Zoom's web client (`/wc/...`) as they are, and a Zoom `/j/<id>` link as the web
/// client, so no app is handed the link.
pub fn page(link: &str) -> Option<Url> {
    let mut url = Url::parse(link).ok()?;
    let host = url.host_str()?.to_owned();
    if host == "meet.google.com" {
        return Some(url);
    }
    if host == "zoom.us" || host.ends_with(".zoom.us") {
        if url.path().starts_with("/wc/") {
            return Some(url);
        }
        let id = url.path().strip_prefix("/j/")?.to_owned();
        url.set_path(&format!("/wc/join/{id}"));
        return Some(url);
    }
    None
}

/// Every webview of the app shares one WebView2 environment, so all get the same arguments: wry's
/// defaults plus the audio service inside the browser process. Out of process (Chromium's default)
/// the class's sound is not part of AutoPara's process tree, and the assistant's process loopback
/// (`lecture_assist::audio::app`) hears silence; measured with Edge, 2026-11-03.
pub const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,AudioServiceOutOfProcess";

/// Show `page` in the tab, replacing whatever it showed. Creating a webview waits on the main
/// thread, so this is never called from it (a synchronous command would deadlock WebView2).
pub fn open(app: &AppHandle, page: Url) -> tauri::Result<()> {
    let site = page.host_str().unwrap_or_default().to_owned();
    let state = app.state::<Meeting>();
    let ((x, y, width, height), (border, background)) = {
        let place = state.place();
        (place.bounds, place.colours.clone())
    };
    match app.get_webview(LABEL) {
        Some(webview) => webview.navigate(page.clone())?,
        None => {
            let window = app.get_window("main").ok_or(tauri::Error::WindowNotFound)?;
            window.add_child(
                WebviewBuilder::new(LABEL, WebviewUrl::External(page.clone()))
                    .additional_browser_args(BROWSER_ARGS)
                    .initialization_script(corners(&border, &background))
                    .initialization_script_for_all_frames(include_str!("volume.js"))
                    // The script above has the colours of when the tab was made; a page that
                    // loads after a theme change gets today's.
                    .on_page_load(|webview, payload| {
                        if payload.event() == PageLoadEvent::Finished {
                            let (border, background) =
                                webview.state::<Meeting>().place().colours.clone();
                            let _ = webview.eval(corners(&border, &background));
                            let settings = webview.state::<AppState>().storage().settings();
                            let (name, join) = settings
                                .map(|s| (s.join_name, s.auto_join))
                                .unwrap_or_default();
                            let _ = webview.eval(join_script(&name, join));
                            let volume = webview.state::<Meeting>().volume();
                            let _ = webview.eval(volume_call(volume));
                        }
                    }),
                LogicalPosition::new(x, y),
                LogicalSize::new(width.max(1.0), height.max(1.0)),
            )?;
        }
    }
    state.place().page = Some(page);
    crate::show_main_window(app);
    app.emit_to(EventTarget::webview("main"), OPENED_EVENT, site)
}

impl Meeting {
    fn place(&self) -> std::sync::MutexGuard<'_, Place> {
        self.place.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_in_app(&self, on: bool) {
        self.in_app.store(on, Ordering::Relaxed);
    }

    /// The page for a class link while classes open in the tab, else `None` (the browser).
    pub fn page(&self, link: &str) -> Option<Url> {
        page(link).filter(|_| self.in_app.load(Ordering::Relaxed))
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// `source` as it was before the page's volume: the assistant hears the class through the
    /// page, so a class turned down for the student would otherwise reach it quieter too.
    pub fn full_volume(&self, source: Box<dyn AudioSource>) -> Box<dyn AudioSource> {
        Box::new(FullVolume {
            source,
            volume: Arc::clone(&self.volume),
        })
    }
}

struct FullVolume {
    source: Box<dyn AudioSource>,
    volume: Arc<AtomicU32>,
}

impl AudioSource for FullVolume {
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<f32>>, AssistError> {
        let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));
        let mut samples = self.source.read(timeout)?;
        // Below 5 % too little is left to bring back; the assistant hears it as it is.
        if (0.05..1.0).contains(&volume) {
            for sample in samples.iter_mut().flatten() {
                *sample /= volume;
            }
        }
        Ok(samples)
    }

    fn set_paused(&mut self, paused: bool) {
        self.source.set_paused(paused);
    }
}

/// Whether a class page is open in the tab.
pub fn is_open(app: &AppHandle) -> bool {
    app.state::<Meeting>().place().page.is_some()
}

/// Where the tab's area is (logical pixels), whether the page may show (only while its tab is
/// chosen and no dialog is open, since the page would cover the interface's own dialogs), and the
/// interface's colours for the page's corners.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // the command's arguments are its IPC message
pub fn meeting_place(
    app: AppHandle,
    state: State<'_, Meeting>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
    border: String,
    background: String,
) -> CommandResult<()> {
    if !is_colour(&border) || !is_colour(&background) {
        return Err("invalid_colour".into());
    }
    let recoloured = {
        let mut place = state.place();
        place.bounds = (x, y, width, height);
        let colours = (border, background);
        let changed = place.colours != colours;
        place.colours = colours;
        changed
    };
    let Some(webview) = app.get_webview(LABEL) else {
        return Ok(());
    };
    if recoloured {
        let (border, background) = state.place().colours.clone();
        webview
            .eval(corners(&border, &background))
            .map_err(failed)?;
    }
    webview
        .set_position(LogicalPosition::new(x, y))
        .and_then(|()| webview.set_size(LogicalSize::new(width.max(1.0), height.max(1.0))))
        .and_then(|()| {
            if visible {
                webview.show()
            } else {
                webview.hide()
            }
        })
        .map_err(failed)
}

/// The slider in the tab's bar: the class page's volume, 0 (silent) to 1 (as the site plays it).
#[tauri::command]
pub fn meeting_volume(app: AppHandle, state: State<'_, Meeting>, volume: f32) -> CommandResult<()> {
    if !(0.0..=1.0).contains(&volume) {
        return Err("invalid_volume".into());
    }
    state.volume.store(volume.to_bits(), Ordering::Relaxed);
    match app.get_webview(LABEL) {
        Some(webview) => webview.eval(volume_call(volume)).map_err(failed),
        None => Ok(()),
    }
}

/// "Увійти в Google" / "Увійти в Zoom": the sign-in page in the tab. The webview keeps its cookies
/// in AutoPara's own profile, so one sign-in lasts.
#[tauri::command]
pub async fn meeting_sign_in(app: AppHandle, service: String) -> CommandResult<()> {
    let link = match service.as_str() {
        "google" => GOOGLE_SIGN_IN,
        "zoom" => ZOOM_SIGN_IN,
        _ => return Err("invalid_service".into()),
    };
    let page = Url::parse(link).map_err(failed)?;
    open(&app, page).map_err(failed)
}

/// "Відкрити в браузері": the class page in the system browser instead, and the tab closed.
#[tauri::command]
pub fn meeting_in_browser(app: AppHandle, state: State<'_, Meeting>) -> CommandResult<()> {
    let page = state.place().page.clone();
    if let Some(page) = page {
        app.opener()
            .open_url(page.as_str(), None::<&str>)
            .map_err(failed)?;
    }
    meeting_close(app, state)
}

/// "Закрити пару": leaves the meeting by closing its page.
#[tauri::command]
pub fn meeting_close(app: AppHandle, state: State<'_, Meeting>) -> CommandResult<()> {
    state.place().page = None;
    match app.get_webview(LABEL) {
        Some(webview) => webview.close().map_err(failed),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meet_links_open_as_they_are() {
        let link = "https://meet.google.com/vdc-yvjh-bxc";
        assert_eq!(page(link).map(String::from), Some(link.to_owned()));
    }

    #[test]
    fn zoom_join_links_become_the_web_client() {
        let link = "https://us04web.zoom.us/j/78964062532?pwd=abc.1";
        assert_eq!(
            page(link).map(String::from),
            Some("https://us04web.zoom.us/wc/join/78964062532?pwd=abc.1".to_owned())
        );
    }

    #[test]
    fn zoom_web_client_links_open_as_they_are() {
        for link in [
            "https://zoom.us/wc/join/78964062532?pwd=abc.1",
            "https://us04web.zoom.us/wc/78964062532/join?pwd=abc.1",
        ] {
            assert_eq!(page(link).map(String::from), Some(link.to_owned()));
        }
    }

    #[test]
    fn the_guest_name_stays_a_string() {
        let script = join_script("Іван\"; alert(1); \"", true);
        assert!(script.contains(r#"const NAME = "Іван\"; alert(1); \"";"#));
        assert!(!script.contains("__NAME__"));
    }

    #[test]
    fn devices_go_off_with_or_without_joining() {
        assert!(join_script("", true).contains("const JOIN = true;"));
        assert!(join_script("", false).contains("const JOIN = false;"));
        assert!(!join_script("", false).contains("__JOIN__"));
    }

    #[test]
    fn the_assistant_hears_the_class_at_full_volume() {
        use lecture_assist::audio::fake::FakeSource;
        let meeting = Meeting::default();
        meeting.volume.store(0.25f32.to_bits(), Ordering::Relaxed);
        let mut source = meeting.full_volume(Box::new(FakeSource::new(vec![0.1, -0.2], 2)));
        let samples = source.read(Duration::from_secs(1)).unwrap().unwrap();
        assert!((samples[0] - 0.4).abs() < 1e-6 && (samples[1] + 0.8).abs() < 1e-6);
        assert_eq!(volume_call(0.25), "window.__autoparaVolume?.(0.25)");
        assert_eq!(volume_call(1.0), "window.__autoparaVolume?.(1)");
    }

    #[test]
    fn only_plain_colours_reach_the_page() {
        assert!(is_colour("rgb(17, 17, 19)"));
        assert!(is_colour("#1c1c1f"));
        assert!(!is_colour("red\";alert(1)//"));
        assert!(!is_colour("red}body{display:none"));
    }

    #[test]
    fn classes_go_to_the_browser_when_asked() {
        let meeting = Meeting::default();
        let link = "https://meet.google.com/vdc-yvjh-bxc";
        assert!(meeting.page(link).is_some());
        meeting.set_in_app(false);
        assert_eq!(meeting.page(link), None);
    }

    #[test]
    fn other_links_stay_in_the_browser() {
        assert_eq!(page("https://us04web.zoom.us/my/teacher"), None);
        assert_eq!(page("https://teams.microsoft.com/l/meetup-join/x"), None);
        assert_eq!(page("https://evilzoom.us/j/1"), None);
        assert_eq!(page("nonsense"), None);
    }
}
