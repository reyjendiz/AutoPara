//! The "Відкрита пара" tab: a class's Google Meet or Zoom page inside the main window, as a second
//! webview over the area the interface keeps for it (`ui/src/ui/meeting.ts`).
//!
//! The page is remote. Tauri refuses every command from a remote origin unless a capability names
//! that origin, and none does, so the page has no way into the app.

use std::sync::{Mutex, PoisonError};

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
#[derive(Default)]
pub struct Meeting(Mutex<Place>);

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

/// "Входити в пару одразу": `join.js`, with the guest name as a JSON string, which is also a
/// JavaScript string literal, so no name can break out of it.
fn join_script(name: &str) -> String {
    let literal = serde_json::to_string(name).unwrap_or_else(|_| "\"\"".to_owned());
    include_str!("join.js").replace("__NAME__", &literal)
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
/// Meet as it is, and a Zoom `/j/<id>` link as Zoom's web client, so no app is handed the link.
pub fn page(link: &str) -> Option<Url> {
    let mut url = Url::parse(link).ok()?;
    let host = url.host_str()?.to_owned();
    if host == "meet.google.com" {
        return Some(url);
    }
    if host == "zoom.us" || host.ends_with(".zoom.us") {
        let id = url.path().strip_prefix("/j/")?.to_owned();
        url.set_path(&format!("/wc/join/{id}"));
        return Some(url);
    }
    None
}

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
                    .initialization_script(corners(&border, &background))
                    // The script above has the colours of when the tab was made; a page that
                    // loads after a theme change gets today's.
                    .on_page_load(|webview, payload| {
                        if payload.event() == PageLoadEvent::Finished {
                            let (border, background) =
                                webview.state::<Meeting>().place().colours.clone();
                            let _ = webview.eval(corners(&border, &background));
                            let settings = webview.state::<AppState>().storage().settings();
                            if let Ok(settings) = settings
                                && settings.auto_join
                            {
                                let _ = webview.eval(join_script(&settings.join_name));
                            }
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
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
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
    fn the_guest_name_stays_a_string() {
        let script = join_script("Іван\"; alert(1); \"");
        assert!(script.contains(r#"const NAME = "Іван\"; alert(1); \"";"#));
        assert!(!script.contains("__NAME__"));
    }

    #[test]
    fn only_plain_colours_reach_the_page() {
        assert!(is_colour("rgb(17, 17, 19)"));
        assert!(is_colour("#1c1c1f"));
        assert!(!is_colour("red\";alert(1)//"));
        assert!(!is_colour("red}body{display:none"));
    }

    #[test]
    fn other_links_stay_in_the_browser() {
        assert_eq!(page("https://us04web.zoom.us/my/teacher"), None);
        assert_eq!(page("https://teams.microsoft.com/l/meetup-join/x"), None);
        assert_eq!(page("https://evilzoom.us/j/1"), None);
        assert_eq!(page("nonsense"), None);
    }
}
