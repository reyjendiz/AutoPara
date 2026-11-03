//! The built-in language model: llama.cpp's `llama-server` on this computer, downloaded on demand
//! with a GGUF model and reached through the OpenAI-compatible client (DECISIONS.md, "Lecture
//! Assist: the built-in language model").

use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;

use super::openai::OpenAiCompatible;
use super::{ChatProvider, Message, ReplyStream};
use crate::AssistError;
use crate::models::{Model, ModelStore};

/// Chat models offered in the settings, strongest first. All answer Ukrainian well and fit an
/// 8 GB graphics card; the last one also runs on a plain processor.
pub const CHAT_MODELS: &[Model] = &[
    Model {
        id: "gemma4-12b",
        file: "gemma-4-12b-it-qat-q4_0.gguf",
        url: "https://huggingface.co/google/gemma-4-12B-it-qat-q4_0-gguf/resolve/main/gemma-4-12b-it-qat-q4_0.gguf",
        size: 6_975_879_296,
        sha256: "93567e57a8fe10b23569b9d9ec38cd005deedf71e29477c421a4b83f418a538b",
        vad: false,
    },
    Model {
        id: "mamaylm-12b",
        file: "MamayLM-Gemma-3-12B-IT-v2.0-Q4_K_M.gguf",
        url: "https://huggingface.co/INSAIT-Institute/MamayLM-Gemma-3-12B-IT-v2.0-GGUF/resolve/main/MamayLM-Gemma-3-12B-IT-v2.0-Q4_K_M.gguf",
        size: 7_300_778_784,
        sha256: "aeb8e3676b5af63e84bd9ecbd0e705ab252f44f749d34baab1ee511e786b2863",
        vad: false,
    },
    Model {
        id: "gemma4-e4b",
        file: "gemma-4-E4B_q4_0-it.gguf",
        url: "https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-gguf/resolve/main/gemma-4-E4B_q4_0-it.gguf",
        size: 5_154_941_280,
        sha256: "676c35070db6dbe52f93e9c864ee0fba4eddea94b9c875d9cb10daff453fbaee",
        vad: false,
    },
];

pub const DEFAULT_CHAT_MODEL: &str = "gemma4-12b";

pub fn chat_model(id: &str) -> Option<&'static Model> {
    CHAT_MODELS.iter().find(|m| m.id == id)
}

/// llama.cpp's own release build: Vulkan on Windows (any GPU vendor, falls back to the CPU).
/// `None` where AutoPara has no built-in model yet (macOS: left for later by the owner).
pub const RUNTIME: Option<Model> = runtime();

const fn runtime() -> Option<Model> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some(Model {
            id: "llama-b12052",
            file: "llama-b12052-bin-win-vulkan-x64.zip",
            url: "https://github.com/ggml-org/llama.cpp/releases/download/b12052/llama-b12052-bin-win-vulkan-x64.zip",
            size: 37_528_522,
            sha256: "9806d110501576d9aa4a58de996209a89ffca3c0634e4bda54b0019dfafc3be5",
            vad: false,
        })
    } else {
        None
    }
}

const SERVER: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};

/// Where `llama-server` lives once the runtime is unpacked.
pub fn server_path(store: &ModelStore) -> Option<PathBuf> {
    let runtime = RUNTIME?;
    Some(store.path(&runtime).with_file_name(runtime.id).join(SERVER))
}

pub fn runtime_installed(store: &ModelStore) -> bool {
    server_path(store).is_some_and(|p| p.is_file())
}

/// Download the runtime archive (checksum checked by the store), unpack it next to the models
/// and drop the archive.
pub async fn install_runtime(
    store: &ModelStore,
    progress: impl Fn(u64, u64),
) -> Result<(), AssistError> {
    let runtime = RUNTIME.ok_or_else(|| AssistError::Model("no built-in model here".into()))?;
    let archive = store.download(&runtime, progress).await?;
    let target = store.path(&runtime).with_file_name(runtime.id);
    let part = target.with_extension("part");
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part)?;
    let unpacked = unpack(&archive, &part).await;
    let _ = std::fs::remove_file(&archive);
    if let Err(error) = unpacked {
        let _ = std::fs::remove_dir_all(&part);
        return Err(error);
    }
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&part, &target)?;
    Ok(())
}

pub fn delete_runtime(store: &ModelStore) -> Result<(), AssistError> {
    let Some(runtime) = RUNTIME else {
        return Ok(());
    };
    match std::fs::remove_dir_all(store.path(&runtime).with_file_name(runtime.id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// The server and its libraries only: the archive also holds tools AutoPara never runs.
#[cfg(windows)]
async fn unpack(archive: &Path, into: &Path) -> Result<(), AssistError> {
    let bad = |e: zip::result::ZipError| AssistError::Model(e.to_string());
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(bad)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(bad)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        // Flat names only: nothing may land outside the runtime folder.
        let Some(name) = name.to_str().filter(|n| !n.contains(['/', '\\'])) else {
            continue;
        };
        if name.ends_with(".dll") || name == SERVER || name.starts_with("LICENSE") {
            let mut file = std::fs::File::create(into.join(name))?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
async fn unpack(_archive: &Path, _into: &Path) -> Result<(), AssistError> {
    Err(AssistError::Model("no built-in model here".into()))
}

/// One server for the whole app, started on the first request and stopped by [`shutdown`]. It
/// unloads the model by itself after ten idle minutes, so a quiet app holds no video memory.
static RUNNING: tokio::sync::Mutex<Option<Server>> = tokio::sync::Mutex::const_new(None);

struct Server {
    /// Killed when dropped.
    child: tokio::process::Child,
    model: PathBuf,
    client: Arc<OpenAiCompatible>,
    pid_file: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.pid_file);
    }
}

/// Stop the server, if one runs. Called when the app quits.
pub fn shutdown() {
    // A quit during the model's load leaves the server running; the next start reaps it.
    if let Ok(mut running) = RUNNING.try_lock() {
        running.take();
    }
}

/// Where the running server's process id is kept, beside `llama-server`: an AutoPara killed
/// without quitting (an upgrade's installer, a crash) cannot stop its server, so the next does.
fn pid_file(server: &Path) -> PathBuf {
    server.with_file_name("server.pid")
}

/// Stop a server an earlier AutoPara left running, if the recorded process is still a
/// `llama-server`. Called when the app starts and before each new server.
pub fn reap(server: &Path) {
    let file = pid_file(server);
    let Some(pid) = std::fs::read_to_string(&file)
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok())
    else {
        return;
    };
    let _ = std::fs::remove_file(&file);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let system = std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32"))
            .unwrap_or_default();
        let listed = std::process::Command::new(system.join("tasklist.exe"))
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output();
        let ours = listed
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{SERVER}\"")));
        if ours {
            tracing::info!(pid, "stopping a built-in model server left running");
            let _ = std::process::Command::new(system.join("taskkill.exe"))
                .args(["/PID", &pid.to_string(), "/F"])
                .creation_flags(0x0800_0000)
                .status();
        }
    }
}

/// The built-in model as a chat provider. Failing to start the server is
/// [`AssistError::ChatUnavailable`], so a chain moves on to its next provider.
pub struct LocalModel {
    server: PathBuf,
    model: PathBuf,
}

impl LocalModel {
    pub fn new(server: PathBuf, model: PathBuf) -> Self {
        Self { server, model }
    }

    async fn client(&self) -> Result<Arc<OpenAiCompatible>, AssistError> {
        let mut running = RUNNING.lock().await;
        if let Some(server) = running.as_mut()
            && server.model == self.model
            && matches!(server.child.try_wait(), Ok(None))
        {
            return Ok(server.client.clone());
        }
        running.take();
        let server = start(&self.server, &self.model).await?;
        let client = server.client.clone();
        *running = Some(server);
        Ok(client)
    }
}

impl ChatProvider for LocalModel {
    fn stream(&self, messages: Vec<Message>) -> BoxFuture<'_, Result<ReplyStream, AssistError>> {
        Box::pin(async move { self.client().await?.stream(messages).await })
    }
}

/// Loading a large model from a slow disk can take a while; the server says 503 until ready.
const LOAD_TIMEOUT: Duration = Duration::from_secs(180);

async fn start(server: &Path, model: &Path) -> Result<Server, AssistError> {
    let unavailable =
        |what: String| AssistError::ChatUnavailable(format!("built-in model: {what}"));
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    // Only AutoPara may talk to it, not a web page that guesses the port.
    let key = format!(
        "{:016x}{:016x}",
        RandomState::new().hash_one(std::process::id()),
        RandomState::new().hash_one(port)
    );
    let mut command = tokio::process::Command::new(server);
    command
        .arg("-m")
        .arg(model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["--api-key", &key, "--no-webui", "--parallel", "1"])
        .args(["--ctx-size", "8192", "--reasoning", "off"])
        .args(["--sleep-idle-seconds", "600"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    reap(server);
    let mut child = command.spawn().map_err(|e| unavailable(e.to_string()))?;
    let pid_file = pid_file(server);
    if let Some(pid) = child.id() {
        let _ = std::fs::write(&pid_file, pid.to_string());
    }
    let health = format!("http://127.0.0.1:{port}/health");
    super::install_tls();
    let probe = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| unavailable(e.to_string()))?;
    let deadline = tokio::time::Instant::now() + LOAD_TIMEOUT;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(unavailable(format!("exited with {status}")));
        }
        if tokio::time::Instant::now() > deadline {
            return Err(unavailable("did not load in time".into()));
        }
        if probe
            .get(&health)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    tracing::info!(port, "built-in model ready");
    let client = OpenAiCompatible::new(
        format!("http://127.0.0.1:{port}/v1"),
        "local",
        Some(key),
        0.3,
        Duration::from_secs(60),
    )?;
    Ok(Server {
        child,
        model: model.to_path_buf(),
        client: Arc::new(client),
        pid_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_consistent() {
        for m in CHAT_MODELS.iter().chain(RUNTIME.iter()) {
            assert!(m.url.ends_with(m.file), "{}", m.id);
            assert!(m.url.starts_with("https://"), "{}", m.id);
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(!m.vad, "{}", m.id);
        }
        for m in CHAT_MODELS {
            assert!(m.file.ends_with(".gguf"), "{}", m.id);
            assert!(crate::models::model(m.id).is_none(), "{}", m.id);
        }
        assert!(chat_model(DEFAULT_CHAT_MODEL).is_some());
    }

    #[tokio::test]
    async fn a_missing_server_lets_the_chain_move_on() {
        let local = LocalModel::new("no-such-server".into(), "no-such-model.gguf".into());
        let result = local.stream(vec![Message::user("hi")]).await;
        assert!(matches!(result, Err(AssistError::ChatUnavailable(_))));
    }

    /// Downloads the runtime into a temporary folder and asks a model you already have:
    /// `AUTOPARA_LLAMA_MODEL=<file.gguf> cargo test -p lecture-assist -- --ignored real_server`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "downloads llama.cpp and needs a GGUF model"]
    async fn a_real_server_answers() {
        let dir = std::env::temp_dir().join(format!("assist-runtime-{}", std::process::id()));
        let store = ModelStore::new(&dir);
        install_runtime(&store, |_, _| {}).await.unwrap();
        assert!(runtime_installed(&store));
        let model = PathBuf::from(std::env::var("AUTOPARA_LLAMA_MODEL").unwrap());
        let local = LocalModel::new(server_path(&store).unwrap(), model);
        for _ in 0..2 {
            let reply = crate::llm::complete(&local, vec![Message::user("Скажи одне слово: так")])
                .await
                .unwrap();
            assert!(!reply.trim().is_empty());
        }
        shutdown();
        // The killed server lets go of its files a moment later.
        tokio::time::sleep(Duration::from_secs(2)).await;
        delete_runtime(&store).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_server_sits_in_the_runtime_folder() {
        let store = ModelStore::new("models");
        if let Some(path) = server_path(&store) {
            assert!(path.starts_with("models"));
            assert!(path.ends_with(SERVER));
            assert!(!runtime_installed(&store));
        }
    }
}
