//! Lecture Assist in the app (plan step 7.9): consent, the engine's lifetime, saving its text,
//! and the tray. It runs beside the scheduler and shares nothing with it but the database, so an
//! assistant failure never touches joining.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use autopara_core::db::AssistSegment;
use autopara_core::i18n::{t, tf};
use autopara_core::model::Lesson;
use autopara_core::time::TimeOfDay;
use chrono::{NaiveDateTime, Timelike};
use lecture_assist::audio::AudioSource;
use lecture_assist::audio::capture::{self, Capture, Device};
use lecture_assist::llm::ChatProvider;
use lecture_assist::llm::chain::ProviderChain;
use lecture_assist::llm::openai::OpenAiCompatible;
use lecture_assist::llm::presets::preset;
use lecture_assist::models::{self, Model, ModelStore};
use lecture_assist::stt::{Recognizer, Whisper};
use lecture_assist::vad::{EnergyVad, SileroVad, Vad};
use lecture_assist::{
    AskKind, AssistConfig, AssistEngine, AssistEvent, ClassContext, FlagKind, Handle, Language,
    Parts, State,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use ts_rs::TS;

use crate::commands::{AppState, CommandResult};
use crate::secrets;

/// Engine events reach both windows under this name.
pub const ASSIST_EVENT: &str = "assist";
/// Model download progress: `[id, done, total]`.
pub const DOWNLOAD_EVENT: &str = "assist-download";
/// The class has ended (plus grace); the interface asks whether to keep listening.
pub const CLASS_OVER_EVENT: &str = "assist-class-over";

const GRACE_MINUTES: i64 = 10;
/// Without an answer to "keep listening?", listening stops.
const ANSWER_SECONDS: u64 = 120;

struct Running {
    handle: Handle,
    keep: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Assist {
    running: Mutex<Option<Running>>,
    /// Whether the engine is capturing, for the tray.
    capturing: AtomicBool,
}

impl Assist {
    fn with<R>(&self, f: impl FnOnce(&mut Option<Running>) -> R) -> R {
        f(&mut self.running.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn is_running(&self) -> bool {
        self.with(|r| r.is_some())
    }

    pub fn capturing(&self) -> bool {
        self.capturing.load(Ordering::Relaxed)
    }
}

/// The assistant's settings, kept as plain keys in the settings table. The API key is not here:
/// it lives in the keychain and only `has_key` comes back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AssistSettings {
    pub consent: bool,
    /// An id from the device list; empty for the default device.
    pub device: String,
    pub model: String,
    pub language: Language,
    /// Comma-separated terms for the recogniser's prompt.
    pub glossary: String,
    /// A preset id, `custom`, or empty for transcript only.
    pub provider: String,
    pub base_url: String,
    pub chat_model: String,
    pub fallback: bool,
    pub retention_days: u32,
    pub has_key: bool,
}

fn setting(state: &AppState, key: &str) -> String {
    state
        .storage()
        .get_setting(key)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn load_settings(state: &AppState) -> AssistSettings {
    let get = |key| setting(state, key);
    let provider = get("assist_provider");
    let model = Some(get("assist_model"))
        .filter(|m| models::model(m).is_some())
        .unwrap_or_else(|| models::default_speech_model().to_string());
    AssistSettings {
        consent: get("assist_consent") == "1",
        device: get("assist_device"),
        model,
        language: serde_json::from_value(serde_json::Value::String(get("assist_language")))
            .unwrap_or_default(),
        glossary: get("assist_glossary"),
        has_key: !provider.is_empty() && secrets::has_api_key(&provider),
        provider,
        base_url: get("assist_base_url"),
        chat_model: get("assist_chat_model"),
        fallback: get("assist_fallback") != "0",
        retention_days: get("assist_retention_days").parse().unwrap_or(30),
    }
}

#[tauri::command]
pub fn assist_settings(state: tauri::State<'_, AppState>) -> AssistSettings {
    load_settings(&state)
}

/// `api_key`: `None` leaves the saved key alone, an empty string forgets it.
#[tauri::command]
pub fn save_assist_settings(
    state: tauri::State<'_, AppState>,
    input: AssistSettings,
    api_key: Option<String>,
) -> CommandResult<AssistSettings> {
    let language = serde_json::to_value(input.language)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let values = [
        ("assist_device", input.device.clone()),
        ("assist_model", input.model.clone()),
        ("assist_language", language),
        ("assist_glossary", input.glossary.clone()),
        ("assist_provider", input.provider.clone()),
        ("assist_base_url", input.base_url.clone()),
        ("assist_chat_model", input.chat_model.clone()),
        (
            "assist_fallback",
            if input.fallback { "1" } else { "0" }.into(),
        ),
        (
            "assist_retention_days",
            input.retention_days.clamp(1, 3650).to_string(),
        ),
    ];
    {
        let storage = state.storage();
        for (key, value) in values {
            storage
                .set_setting(key, &value)
                .map_err(|e| e.to_string())?;
        }
    }
    if let (Some(key), false) = (api_key, input.provider.is_empty()) {
        secrets::set_api_key(&input.provider, &key)?;
    }
    Ok(load_settings(&state))
}

/// Accept the first-run consent, or revoke it (which also stops listening).
#[tauri::command]
pub fn set_assist_consent(app: AppHandle, accepted: bool) -> CommandResult<()> {
    let state = app.state::<AppState>();
    state
        .storage()
        .set_setting("assist_consent", if accepted { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    if !accepted {
        stop(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn assist_devices() -> Vec<Device> {
    capture::devices()
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ModelStatus {
    pub model: Model,
    pub installed: bool,
}

fn store(state: &AppState) -> ModelStore {
    ModelStore::new(state.data_root.join("models"))
}

#[tauri::command]
pub fn assist_models(state: tauri::State<'_, AppState>) -> Vec<ModelStatus> {
    let store = store(&state);
    models::MODELS
        .iter()
        .map(|m| ModelStatus {
            model: *m,
            installed: store.installed(m),
        })
        .collect()
}

#[tauri::command]
pub async fn assist_download_model(app: AppHandle, id: String) -> CommandResult<()> {
    let model = models::model(&id).ok_or("not_found")?;
    let store = store(&app.state::<AppState>());
    let progress = |done: u64, total: u64| {
        let _ = app.emit(DOWNLOAD_EVENT, (&id, done, total));
    };
    store
        .download(model, progress)
        .await
        .map(|_| ())
        .map_err(|e| e.code().to_string())
}

#[tauri::command]
pub fn assist_delete_model(state: tauri::State<'_, AppState>, id: String) -> CommandResult<()> {
    let model = models::model(&id).ok_or("not_found")?;
    store(&state)
        .delete(model)
        .map_err(|e| e.code().to_string())
}

#[tauri::command]
pub fn assist_delete_all(state: tauri::State<'_, AppState>) -> CommandResult<()> {
    state
        .storage()
        .delete_all_assist()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The class happening now in the selected group, if any.
fn current_lesson(state: &AppState, now: NaiveDateTime) -> Option<Lesson> {
    let storage = state.storage();
    let group = storage.settings().ok()?.selected_group_id?;
    let time = TimeOfDay::from_hm(now.hour() as u8, now.minute() as u8)?;
    storage
        .lessons_for_group(group)
        .ok()?
        .into_iter()
        .find(|l| l.occurs_on(now.date()) && l.start <= time && time < l.end)
}

/// Start listening to `lesson_id`, or to the class happening now.
#[tauri::command]
pub async fn assist_start(app: AppHandle, lesson_id: Option<i64>) -> CommandResult<()> {
    start(&app, lesson_id)
}

pub fn start(app: &AppHandle, lesson_id: Option<i64>) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let assist = app.state::<Assist>();
    if assist.is_running() {
        return Ok(());
    }
    let settings = load_settings(&state);
    if !settings.consent {
        return Err("consent_required".into());
    }
    let store = store(&state);
    let model = models::model(&settings.model).ok_or("model_missing")?;
    if !store.installed(model) {
        return Err("model_missing".into());
    }
    let now = state.clock.now();
    let lesson = match lesson_id {
        Some(id) => state.storage().lesson(id).ok().flatten(),
        None => current_lesson(&state, now),
    };
    let date = now.date();
    let at = |time: TimeOfDay| format!("{date}T{time}");
    let context = ClassContext {
        subject: lesson
            .as_ref()
            .map(|l| l.subject.clone())
            .unwrap_or_default(),
        teacher: lesson
            .as_ref()
            .map(|l| l.teacher.clone())
            .unwrap_or_default(),
        group: String::new(),
        start: lesson.as_ref().map(|l| at(l.start)),
        end: lesson.as_ref().map(|l| at(l.end)),
    };
    let session = state
        .storage()
        .start_assist_session(
            lesson.as_ref().map(|l| l.id),
            Some(date),
            &context.subject,
            now,
        )
        .map_err(|e| e.to_string())?;

    let model_path = store.path(model);
    let vad_path = models::MODELS
        .iter()
        .find(|m| m.vad && store.installed(m))
        .map(|m| store.path(m));
    let language = settings.language;
    let device = Some(settings.device.clone()).filter(|d| !d.is_empty());
    let parts = Parts {
        source: Box::new(move || Ok(Box::new(Capture::open(device)) as Box<dyn AudioSource>)),
        recognizer: Box::new(move || {
            let gpu = cfg!(any(feature = "cuda", feature = "metal"));
            Ok(Box::new(Whisper::load(&model_path, language, gpu)?) as Box<dyn Recognizer>)
        }),
        vad: Box::new(move || {
            Ok(match vad_path.map(|p| SileroVad::new(&p)) {
                Some(Ok(silero)) => Box::new(silero) as Box<dyn Vad>,
                _ => Box::new(EnergyVad::default()),
            })
        }),
        chat: chat(&settings),
    };
    let config = AssistConfig {
        language,
        glossary: settings
            .glossary
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        ..AssistConfig::default()
    };
    let (handle, events) = AssistEngine::start(config, context, parts);
    let keep = Arc::new(AtomicBool::new(false));
    assist.with(|r| {
        *r = Some(Running {
            handle,
            keep: keep.clone(),
        })
    });
    tauri::async_runtime::spawn(forward(app.clone(), session, events));
    let end_time =
        |l: Lesson| chrono::NaiveTime::from_hms_opt(l.end.hour().into(), l.end.minute().into(), 0);
    if let Some(end) = lesson.and_then(end_time).map(|time| date.and_time(time)) {
        tauri::async_runtime::spawn(class_over(app.clone(), end, keep));
    }
    Ok(())
}

/// The user's provider, then a local one when asked to fall back.
fn chat(settings: &AssistSettings) -> Option<Arc<dyn ChatProvider>> {
    if settings.provider.is_empty() {
        return None;
    }
    let chosen = preset(&settings.provider);
    let base_url = Some(settings.base_url.clone())
        .filter(|u| !u.is_empty())
        .or_else(|| chosen.map(|p| p.base_url.to_string()))?;
    let model = Some(settings.chat_model.clone())
        .filter(|m| !m.is_empty())
        .or_else(|| chosen.map(|p| p.model.to_string()))?;
    let timeout = Duration::from_secs(30);
    let key = secrets::api_key(&settings.provider);
    let mut providers: Vec<Arc<dyn ChatProvider>> = vec![Arc::new(
        OpenAiCompatible::new(base_url, model, key, 0.3, timeout).ok()?,
    )];
    let local = chosen.is_some_and(|p| p.local);
    if settings.fallback
        && !local
        && let Some(ollama) = preset("ollama")
    {
        let fallback =
            OpenAiCompatible::new(ollama.base_url, ollama.model, None, 0.3, timeout).ok()?;
        providers.push(Arc::new(fallback));
    }
    Some(Arc::new(ProviderChain(providers)))
}

/// Pass events on to the windows and keep the text: segments, notes and the summary.
async fn forward(
    app: AppHandle,
    session: i64,
    mut events: tokio::sync::mpsc::UnboundedReceiver<AssistEvent>,
) {
    let mut notes = String::from("{}");
    let mut terms = String::from("[]");
    let mut summary = String::new();
    while let Some(event) = events.recv().await {
        let state = app.state::<AppState>();
        let now = state.clock.now();
        let saved = match &event {
            AssistEvent::TranscriptFinal { segment } => {
                let flag = segment.flag.map(|f| match f {
                    FlagKind::Question => "question".to_string(),
                    FlagKind::Task => "task".to_string(),
                });
                let row = AssistSegment {
                    start_ms: segment.start_ms as i64,
                    end_ms: segment.end_ms as i64,
                    text: segment.text.clone(),
                    flag,
                };
                state.storage().add_assist_segment(session, &row)
            }
            AssistEvent::NotesUpdated { notes: n } => {
                notes = serde_json::to_string(n).unwrap_or_default();
                state
                    .storage()
                    .save_assist_notes(session, &notes, &terms, now)
            }
            AssistEvent::TermsUpdated { terms: list } => {
                terms = serde_json::to_string(list).unwrap_or_default();
                state
                    .storage()
                    .save_assist_notes(session, &notes, &terms, now)
            }
            AssistEvent::HelperReply {
                kind: AskKind::Summary,
                text,
                ..
            } => {
                summary.push_str(text);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = saved {
            tracing::warn!(%error, "could not save assistant text");
        }
        let _ = app.emit(ASSIST_EVENT, &event);
        if let AssistEvent::StateChanged { state: s } = &event {
            let assist = app.state::<Assist>();
            assist.capturing.store(s.capturing(), Ordering::Relaxed);
            crate::tray::set_listening(&app, assist.is_running(), s.capturing());
            if matches!(s, State::Off | State::Error(_)) {
                if let Err(error) = state
                    .storage()
                    .finish_assist_session(session, &summary, now)
                {
                    tracing::warn!(%error, "could not close the assistant session");
                }
                assist.with(|r| *r = None);
                crate::tray::set_listening(&app, false, false);
                break;
            }
        }
    }
}

/// Ten minutes after the class ends, ask whether to keep listening; without an answer, stop.
async fn class_over(app: AppHandle, end: NaiveDateTime, keep: Arc<AtomicBool>) {
    let due = end + chrono::Duration::minutes(GRACE_MINUTES);
    let wait = (due - app.state::<AppState>().clock.now())
        .to_std()
        .unwrap_or_default();
    tokio::time::sleep(wait).await;
    let still = app
        .state::<Assist>()
        .with(|r| r.as_ref().is_some_and(|r| Arc::ptr_eq(&r.keep, &keep)));
    if !still {
        return;
    }
    let _ = app.emit(CLASS_OVER_EVENT, ());
    let _ = app
        .notification()
        .builder()
        .title(t("assist_class_over_title"))
        .body(tf("assist_class_over_body", &[("subject", "")]))
        .show();
    tokio::time::sleep(Duration::from_secs(ANSWER_SECONDS)).await;
    if !keep.load(Ordering::Relaxed) {
        stop(&app);
    }
}

#[tauri::command]
pub fn assist_keep_listening(app: AppHandle) {
    app.state::<Assist>().with(|r| {
        if let Some(r) = r {
            r.keep.store(true, Ordering::Relaxed);
        }
    });
}

pub fn stop(app: &AppHandle) {
    app.state::<Assist>().with(|r| {
        if let Some(r) = r {
            r.handle.stop();
        }
    });
}

#[tauri::command]
pub fn assist_stop(app: AppHandle) {
    stop(&app);
}

#[tauri::command]
pub fn assist_pause(app: AppHandle) {
    app.state::<Assist>()
        .with(|r| r.as_ref().map(|r| r.handle.pause()));
}

#[tauri::command]
pub fn assist_resume(app: AppHandle) {
    app.state::<Assist>()
        .with(|r| r.as_ref().map(|r| r.handle.resume()));
}

#[tauri::command]
pub fn assist_ask(app: AppHandle, kind: AskKind, text: String) {
    app.state::<Assist>()
        .with(|r| r.as_ref().map(|r| r.handle.ask(kind, text)));
}

#[cfg(test)]
mod tests {
    use lecture_assist::AssistError;

    use super::*;

    /// `t` asserts in debug builds when a key is missing.
    #[test]
    fn every_assistant_message_has_a_string() {
        let errors = [
            AssistError::Audio(String::new()),
            AssistError::SourceEnded,
            AssistError::Model(String::new()),
            AssistError::Recognition(String::new()),
            AssistError::Chat(String::new()),
            AssistError::ChatUnavailable(String::new()),
            AssistError::RateLimited,
            AssistError::Io(std::io::Error::other("x")),
        ];
        for error in errors {
            assert!(!t(error.code()).is_empty());
        }
        for key in [
            "assist_error_crash",
            "assist_md_key_points",
            "assist_md_definitions",
            "assist_md_deadlines",
            "assist_md_flagged",
            "tray_stop_listening",
            "assist_recording",
        ] {
            assert!(!t(key).is_empty(), "{key}");
        }
    }

    #[test]
    fn no_provider_means_no_chat() {
        let settings = AssistSettings {
            consent: true,
            device: String::new(),
            model: "small".into(),
            language: Language::Auto,
            glossary: String::new(),
            provider: String::new(),
            base_url: String::new(),
            chat_model: String::new(),
            fallback: true,
            retention_days: 30,
            has_key: false,
        };
        assert!(chat(&settings).is_none());
        let ollama = AssistSettings {
            provider: "ollama".into(),
            ..settings
        };
        assert!(chat(&ollama).is_some());
    }
}
