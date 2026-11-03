//! Speech and VAD models: catalogue, download with checksum, delete (plan step 7.5).
//!
//! Models are downloaded on demand from the official ggml repositories, never bundled.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::AssistError;
use crate::llm::install_tls;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "SpeechModel")]
pub struct Model {
    pub id: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    /// Bytes, shown before download.
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: &'static str,
    /// The voice detector rather than a recogniser.
    pub vad: bool,
}

pub const MODELS: &[Model] = &[
    Model {
        id: "large-v3-turbo-q5_0",
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        size: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        vad: false,
    },
    Model {
        id: "medium-q5_0",
        file: "ggml-medium-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
        size: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        vad: false,
    },
    Model {
        id: "small",
        file: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        vad: false,
    },
    Model {
        id: "silero-v5.1.2",
        file: "ggml-silero-v5.1.2.bin",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        size: 885_098,
        sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
        vad: true,
    },
];

pub fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}

/// What this build runs well: turbo on a GPU (CUDA, Vulkan), quantised medium on Metal, small on
/// the CPU.
pub fn default_speech_model() -> &'static str {
    if cfg!(any(feature = "cuda", feature = "vulkan")) {
        "large-v3-turbo-q5_0"
    } else if cfg!(feature = "metal") {
        "medium-q5_0"
    } else {
        "small"
    }
}

/// The `models` folder in the app data directory.
pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn path(&self, model: &Model) -> PathBuf {
        self.dir.join(model.file)
    }

    /// Present and whole; the checksum was checked when it arrived.
    pub fn installed(&self, model: &Model) -> bool {
        std::fs::metadata(self.path(model)).is_ok_and(|m| m.len() == model.size)
    }

    pub fn delete(&self, model: &Model) -> Result<(), AssistError> {
        match std::fs::remove_file(self.path(model)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// Download into a `.part` file, check SHA-256, then move it into place. `progress` gets
    /// (bytes so far, total). A failed or corrupt download leaves nothing behind.
    pub async fn download(
        &self,
        model: &Model,
        progress: impl Fn(u64, u64),
    ) -> Result<PathBuf, AssistError> {
        std::fs::create_dir_all(&self.dir)?;
        let target = self.path(model);
        let part = target.with_extension("part");
        let result = fetch(model, &part, &progress).await;
        match result {
            Ok(()) => {
                std::fs::rename(&part, &target)?;
                Ok(target)
            }
            Err(error) => {
                let _ = std::fs::remove_file(&part);
                Err(error)
            }
        }
    }
}

async fn fetch(
    model: &Model,
    part: &Path,
    progress: &impl Fn(u64, u64),
) -> Result<(), AssistError> {
    let failed = |e: reqwest::Error| AssistError::Model(e.without_url().to_string());
    install_tls();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(failed)?;
    let response = client.get(model.url).send().await.map_err(failed)?;
    if !response.status().is_success() {
        return Err(AssistError::Model(format!("HTTP {}", response.status())));
    }
    let mut file = std::fs::File::create(part)?;
    let mut hasher = Sha256::new();
    let mut done = 0;
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(failed)?;
        hasher.update(&chunk);
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        progress(done, model.size);
    }
    file.sync_all()?;
    let digest = format!("{:x}", hasher.finalize());
    if digest != model.sha256 {
        return Err(AssistError::Model(format!(
            "{}: checksum mismatch",
            model.file
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const BYTES: &[u8] = b"not really a model";

    fn leak(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    async fn served(sha256: &'static str) -> (MockServer, Model) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/m.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(BYTES))
            .mount(&server)
            .await;
        let model = Model {
            id: "test",
            file: "m.bin",
            url: leak(format!("{}/m.bin", server.uri())),
            size: BYTES.len() as u64,
            sha256,
            vad: false,
        };
        (server, model)
    }

    fn store() -> (PathBuf, ModelStore) {
        let dir = std::env::temp_dir().join(format!(
            "assist-models-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace(':', "_")
        ));
        (dir.clone(), ModelStore::new(dir))
    }

    #[tokio::test]
    async fn a_good_download_is_installed() {
        let sha = leak(format!("{:x}", Sha256::digest(BYTES)));
        let (_server, model) = served(sha).await;
        let (dir, store) = store();
        let seen = AtomicU64::new(0);
        let path = store
            .download(&model, |done, _| seen.store(done, Ordering::Relaxed))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), BYTES);
        assert_eq!(seen.load(Ordering::Relaxed), BYTES.len() as u64);
        assert!(store.installed(&model));
        store.delete(&model).unwrap();
        assert!(!store.installed(&model));
        store.delete(&model).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn a_corrupt_download_leaves_nothing() {
        let (_server, model) = served("00").await;
        let (dir, store) = store();
        let result = store.download(&model, |_, _| {}).await;
        assert!(matches!(result, Err(AssistError::Model(m)) if m.contains("checksum")));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn catalogue_is_consistent() {
        for m in MODELS {
            assert!(m.url.ends_with(m.file), "{}", m.id);
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            let whisper = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
            assert!(m.vad || m.url.starts_with(whisper), "{}", m.id);
        }
        assert!(model(default_speech_model()).is_some());
        assert_eq!(MODELS.iter().filter(|m| m.vad).count(), 1);
    }
}
