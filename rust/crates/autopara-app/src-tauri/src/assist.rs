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
use lecture_assist::llm::chain::ProviderChain;
use lecture_assist::llm::local::{self, LocalModel};
use lecture_assist::llm::openai::OpenAiCompatible;
use lecture_assist::llm::presets::preset;
use lecture_assist::llm::{ChatProvider, Message, complete};
use lecture_assist::models::{self, Model, ModelStore};
use lecture_assist::stt::{Recognizer, Whisper};
use lecture_assist::vad::{EnergyVad, SileroVad, Vad};
use lecture_assist::{
    AskKind, AssistConfig, AssistEngine, AssistError, AssistEvent, ClassContext, FlagKind, Handle,
    Language, Parts, State,
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
    subject: String,
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
    /// An id from the device list, `system` for everything the computer plays, or empty for the
    /// class in AutoPara's tab (everything when the class is not there).
    pub device: String,
    pub model: String,
    pub language: Language,
    /// Comma-separated terms for the recogniser's prompt.
    pub glossary: String,
    /// `builtin` (the default), a preset id, `custom`, or `none` for transcript only.
    pub provider: String,
    pub base_url: String,
    pub chat_model: String,
    /// The built-in model's id, used when `provider` is `builtin`.
    pub local_model: String,
    pub fallback: bool,
    pub retention_days: u32,
    /// Start listening whenever AutoPara opens a class (off unless the user turns it on).
    pub auto_listen: bool,
    /// Answer the teacher's questions in the assistant's panel as they are heard (on unless turned off).
    pub answer_questions: bool,
    pub has_key: bool,
    /// Changed only through `set_assist_hotkey`, which checks for a conflict first.
    pub hotkey: String,
}

/// The provider id of the built-in model.
const BUILTIN: &str = "builtin";
/// The audio source that is everything the computer plays.
const SYSTEM_SOUND: &str = "system";

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
    let provider = Some(get("assist_provider"))
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| BUILTIN.to_string());
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
        has_key: secrets::has_api_key(&provider),
        provider,
        base_url: get("assist_base_url"),
        chat_model: get("assist_chat_model"),
        local_model: Some(get("assist_local_model"))
            .filter(|m| local::chat_model(m).is_some())
            .unwrap_or_else(|| local::DEFAULT_CHAT_MODEL.to_string()),
        fallback: get("assist_fallback") != "0",
        retention_days: get("assist_retention_days").parse().unwrap_or(30),
        auto_listen: get("assist_auto_listen") == "1",
        // On unless turned off (owner, 2026-11-03). A new key: the old `assist_answer_questions`
        // was saved as "0" by every save of the dialog, chosen or not.
        answer_questions: get("assist_answers") != "0",
        hotkey: crate::hotkey::saved(state),
    }
}

#[tauri::command]
pub fn assist_settings(state: tauri::State<'_, AppState>) -> AssistSettings {
    load_settings(&state)
}

/// `api_key`: `None` leaves the saved key alone, an empty string forgets it.
#[tauri::command]
pub fn save_assist_settings(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    input: AssistSettings,
    api_key: Option<String>,
) -> CommandResult<AssistSettings> {
    if !is_safe_endpoint(&input.base_url) {
        return Err("assist_error_url".into());
    }
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
        ("assist_local_model", input.local_model.clone()),
        (
            "assist_fallback",
            if input.fallback { "1" } else { "0" }.into(),
        ),
        (
            "assist_auto_listen",
            if input.auto_listen { "1" } else { "0" }.into(),
        ),
        (
            "assist_answers",
            if input.answer_questions { "1" } else { "0" }.into(),
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
    // A newly chosen model arrives by itself.
    prepare(&app);
    Ok(load_settings(&state))
}

/// The key and the class's text go to this address: `https://`, or plain `http://` only to this
/// computer (Ollama, LM Studio). Empty means the preset's own address.
fn is_safe_endpoint(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.is_empty()
        || url.starts_with("https://")
        || ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|local| {
                url.strip_prefix(local)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
            })
}

/// Accept the first-run consent, or revoke it (which also stops listening).
#[tauri::command]
pub fn set_assist_consent(app: AppHandle, accepted: bool) -> CommandResult<()> {
    let state = app.state::<AppState>();
    state
        .storage()
        .set_setting("assist_consent", if accepted { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    if accepted {
        prepare(&app);
    } else {
        stop(&app);
    }
    Ok(())
}

/// False when another program holds the saved shortcut.
#[tauri::command]
pub fn assist_hotkey_active(app: AppHandle) -> bool {
    crate::hotkey::active(&app)
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

/// The built-in chat models; "installed" also needs the server that runs them.
#[tauri::command]
pub fn assist_chat_models(state: tauri::State<'_, AppState>) -> Vec<ModelStatus> {
    let store = store(&state);
    let runtime = local::runtime_installed(&store);
    let offered = if local::RUNTIME.is_some() {
        local::CHAT_MODELS
    } else {
        &[]
    };
    offered
        .iter()
        .map(|m| ModelStatus {
            model: *m,
            installed: runtime && store.installed(m),
        })
        .collect()
}

#[tauri::command]
pub async fn assist_download_model(app: AppHandle, id: String) -> CommandResult<()> {
    fetch(&app, &id).await.map_err(|e| e.code().to_string())
}

/// One download at a time: the settings' buttons and [`prepare`] never write the same file.
static DOWNLOADS: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());

/// A speech model, or a chat model together with the server on its first download (progress
/// counts both as one). What is already here is not fetched again.
async fn fetch(app: &AppHandle, id: &str) -> Result<(), AssistError> {
    let _one_at_a_time = DOWNLOADS.lock().await;
    let store = store(&app.state::<AppState>());
    let progress = |done: u64, total: u64| {
        let _ = app.emit(DOWNLOAD_EVENT, (id, done, total));
    };
    let unknown = || AssistError::Model(format!("{id}: not in the catalogue"));
    if let Some(model) = models::model(id) {
        if !store.installed(model) {
            store.download(model, progress).await?;
        }
        return Ok(());
    }
    let model = local::chat_model(id).ok_or_else(unknown)?;
    let runtime = local::RUNTIME.ok_or_else(unknown)?;
    let total = runtime.size + model.size;
    if !local::runtime_installed(&store) {
        local::install_runtime(&store, |done, _| progress(done, total)).await?;
    }
    if !store.installed(model) {
        store
            .download(model, |done, _| progress(runtime.size + done, total))
            .await?;
    }
    Ok(())
}

/// At start: stop a built-in model server an earlier AutoPara left running when it was killed
/// (an upgrade's installer does that) instead of quitting.
pub fn reap_left_server(app: &AppHandle) {
    if let Some(server) = local::server_path(&store(&app.state::<AppState>())) {
        local::reap(&server);
    }
}

/// Once the user has agreed to the assistant, fetch what listening needs and is not here yet: the
/// chosen speech model, the voice detector and, for the built-in provider, its server and model.
/// Runs at start, on consent and after the settings change; a failure waits for the next time.
pub fn prepare(app: &AppHandle) {
    let settings = load_settings(&app.state::<AppState>());
    if !settings.consent {
        return;
    }
    let mut wanted = vec![settings.model];
    wanted.extend(
        models::MODELS
            .iter()
            .filter(|m| m.vad)
            .map(|m| m.id.to_string()),
    );
    if settings.provider == BUILTIN && local::RUNTIME.is_some() {
        wanted.push(settings.local_model);
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for id in wanted {
            if let Err(error) = fetch(&app, &id).await {
                tracing::warn!(%error, id, "could not prepare the assistant");
                break;
            }
        }
    });
}

#[tauri::command]
pub fn assist_delete_model(state: tauri::State<'_, AppState>, id: String) -> CommandResult<()> {
    let store = store(&state);
    let failed = |e: AssistError| e.code().to_string();
    if let Some(model) = models::model(&id) {
        return store.delete(model).map_err(failed);
    }
    let model = local::chat_model(&id).ok_or("not_found")?;
    // Windows keeps a file the server has open from being deleted.
    local::shutdown();
    store.delete(model).map_err(failed)?;
    if !local::CHAT_MODELS.iter().any(|m| store.installed(m)) {
        local::delete_runtime(&store).map_err(failed)?;
    }
    Ok(())
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
    if !lecture_assist::stt::backend_available() {
        return Err("assist_error_gpu".into());
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
    // The default hears only AutoPara when the class is in its tab, so nothing else the computer
    // plays gets in; `system` is the whole output, as before.
    // A class AutoPara is opening in the tab counts too: listening may start before its page exists.
    #[cfg(windows)]
    let class_only = settings.device.is_empty()
        && (crate::meeting::is_open(app)
            || lesson
                .as_ref()
                .and_then(|l| l.url.as_deref())
                .is_some_and(|link| app.state::<crate::meeting::Meeting>().page(link).is_some()));
    let device = Some(settings.device.clone()).filter(|d| !d.is_empty() && d != SYSTEM_SOUND);
    #[cfg(windows)]
    let app_for_source = app.clone();
    let parts = Parts {
        source: Box::new(move || {
            #[cfg(windows)]
            if class_only {
                let own = lecture_assist::audio::app::AppAudio::open(std::process::id());
                let meeting = app_for_source.state::<crate::meeting::Meeting>();
                return Ok(meeting.full_volume(Box::new(own)));
            }
            // On a Mac, the default is everything it plays; a chosen device (BlackHole) wins.
            #[cfg(target_os = "macos")]
            if device.is_none() {
                match lecture_assist::audio::macos::ScreenAudio::open() {
                    Ok(screen) => return Ok(Box::new(screen) as Box<dyn AudioSource>),
                    Err(error) => tracing::warn!(%error, "falling back to an input device"),
                }
            }
            Ok(Box::new(Capture::open(device)) as Box<dyn AudioSource>)
        }),
        recognizer: Box::new(move || {
            let gpu = cfg!(any(feature = "cuda", feature = "vulkan", feature = "metal"));
            Ok(Box::new(Whisper::load(&model_path, language, gpu)?) as Box<dyn Recognizer>)
        }),
        vad: Box::new(move || {
            Ok(match vad_path.map(|p| SileroVad::new(&p)) {
                Some(Ok(silero)) => Box::new(silero) as Box<dyn Vad>,
                _ => Box::new(EnergyVad::default()),
            })
        }),
        chat: chat(&settings, &store),
    };
    let config = AssistConfig {
        language,
        glossary: settings
            .glossary
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        answer_questions: settings.answer_questions,
        ..AssistConfig::default()
    };
    // The engine runs chat requests on the async runtime, but the tray, the shortcut and an
    // opened class call this from the main thread, which is outside it.
    let _runtime = tauri::async_runtime::handle().inner().enter();
    let (handle, events) = AssistEngine::start(config, context.clone(), parts);
    let keep = Arc::new(AtomicBool::new(false));
    assist.with(|r| {
        *r = Some(Running {
            handle,
            subject: context.subject.clone(),
            keep: keep.clone(),
        })
    });
    tauri::async_runtime::spawn(forward(app.clone(), session, events));
    // The assistant's panel is in the main window, beside the class.
    crate::show_main_window(app);
    let end_time =
        |l: Lesson| chrono::NaiveTime::from_hms_opt(l.end.hour().into(), l.end.minute().into(), 0);
    if let Some(end) = lesson.and_then(end_time).map(|time| date.and_time(time)) {
        tauri::async_runtime::spawn(class_over(app.clone(), end, keep));
    }
    Ok(())
}

/// One short request to the saved provider, without the fallback, so the answer is about it.
#[tauri::command]
pub async fn assist_test_connection(app: AppHandle) -> CommandResult<()> {
    let settings = AssistSettings {
        fallback: false,
        ..load_settings(&app.state::<AppState>())
    };
    let store = store(&app.state::<AppState>());
    let provider = chat(&settings, &store).ok_or("assist_error_chat")?;
    let messages = vec![Message::user("Reply with the single word: ok")];
    complete(provider.as_ref(), messages)
        .await
        .map(|_| ())
        .map_err(|e| e.code().to_string())
}

/// The user's provider, then, when asked to fall back, the built-in model (or Ollama when the
/// built-in model is the user's choice).
fn chat(settings: &AssistSettings, store: &ModelStore) -> Option<Arc<dyn ChatProvider>> {
    let timeout = Duration::from_secs(30);
    let mut providers: Vec<Arc<dyn ChatProvider>> = Vec::new();
    match settings.provider.as_str() {
        "none" => return None,
        // Nothing downloaded yet means no notes, not requests to a missing Ollama.
        BUILTIN => providers.push(builtin_model(settings, store)?),
        id => {
            let chosen = preset(id);
            let base_url = Some(settings.base_url.clone())
                .filter(|u| !u.is_empty())
                .or_else(|| chosen.map(|p| p.base_url.to_string()))?;
            let model = Some(settings.chat_model.clone())
                .filter(|m| !m.is_empty())
                .or_else(|| chosen.map(|p| p.model.to_string()))?;
            let key = secrets::api_key(id);
            let client = OpenAiCompatible::new(base_url, model, key, 0.3, timeout).ok()?;
            providers.push(Arc::new(client));
        }
    }
    if settings.fallback {
        let fallback = if settings.provider == BUILTIN {
            preset("ollama")
                .and_then(|p| OpenAiCompatible::new(p.base_url, p.model, None, 0.3, timeout).ok())
                .map(|client| Arc::new(client) as Arc<dyn ChatProvider>)
        } else {
            builtin_model(settings, store)
        };
        providers.extend(fallback);
    }
    Some(Arc::new(ProviderChain(providers)))
}

/// The chosen built-in model, when it and its server are downloaded.
fn builtin_model(settings: &AssistSettings, store: &ModelStore) -> Option<Arc<dyn ChatProvider>> {
    let model = local::chat_model(&settings.local_model)?;
    let server = local::server_path(store).filter(|p| p.is_file())?;
    let local = LocalModel::new(server, store.path(model));
    store
        .installed(model)
        .then(|| Arc::new(local) as Arc<dyn ChatProvider>)
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

/// The subject being listened to, for the panel's title; `None` when not listening.
#[tauri::command]
pub fn assist_subject(app: AppHandle) -> Option<String> {
    app.state::<Assist>()
        .with(|r| r.as_ref().map(|r| r.subject.clone()))
}

#[tauri::command]
pub fn assist_keep_listening(app: AppHandle) {
    app.state::<Assist>().with(|r| {
        if let Some(r) = r {
            r.keep.store(true, Ordering::Relaxed);
        }
    });
}

/// A class AutoPara has just opened: listen to it as well, when the user turned that on. Without
/// consent nothing starts; a missing model is only logged, the class itself is already open.
pub fn listen_on_open(app: &AppHandle, lesson_id: i64) {
    let settings = load_settings(&app.state::<AppState>());
    if !settings.auto_listen || !settings.consent {
        return;
    }
    if let Err(code) = start(app, Some(lesson_id)) {
        tracing::warn!(%code, "could not start listening to the opened class");
    }
}

/// The tray item and the shortcut: stop if listening, otherwise start on the class happening now.
pub fn toggle(app: &AppHandle) {
    if app.state::<Assist>().is_running() {
        return stop(app);
    }
    if let Err(code) = start(app, None) {
        // Consent or a model is missing: the window explains.
        crate::show_main_window(app);
        let _ = app.emit(crate::tray::TRAY_ACTION_EVENT, format!("assist:{code}"));
    }
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

/// The language model off for the rest of this class (listening goes on), or back on. Off also
/// stops the built-in model's server, so its memory and graphics card are free at once.
#[tauri::command]
pub fn assist_set_chat(app: AppHandle, on: bool) {
    app.state::<Assist>()
        .with(|r| r.as_ref().map(|r| r.handle.set_chat(on)));
    if !on {
        local::shutdown();
    }
}

#[tauri::command]
pub fn assist_ask(app: AppHandle, kind: AskKind, text: String) {
    app.state::<Assist>()
        .with(|r| r.as_ref().map(|r| r.handle.ask(kind, text)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_goes_only_over_https_or_to_this_computer() {
        for good in [
            "",
            "https://api.deepseek.com/v1",
            "http://127.0.0.1:11434/v1",
            "http://localhost:1234/v1",
        ] {
            assert!(is_safe_endpoint(good), "{good}");
        }
        for bad in [
            "http://example.com/v1",
            "http://localhost.example.com/v1",
            "ftp://127.0.0.1",
            "api.deepseek.com",
        ] {
            assert!(!is_safe_endpoint(bad), "{bad}");
        }
        assert!(!t("assist_error_url").is_empty());
    }

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
            "assist_error_gpu",
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
            provider: "none".into(),
            base_url: String::new(),
            chat_model: String::new(),
            local_model: local::DEFAULT_CHAT_MODEL.into(),
            fallback: true,
            retention_days: 30,
            auto_listen: false,
            answer_questions: false,
            has_key: false,
            hotkey: String::new(),
        };
        let store = ModelStore::new("no-such-folder");
        assert!(chat(&settings, &store).is_none());
        let builtin = AssistSettings {
            provider: BUILTIN.into(),
            ..settings.clone()
        };
        assert!(chat(&builtin, &store).is_none(), "nothing downloaded");
        let ollama = AssistSettings {
            provider: "ollama".into(),
            ..settings
        };
        assert!(chat(&ollama, &store).is_some());
    }
}
