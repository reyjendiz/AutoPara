//! Ready-made provider settings shown in the settings screen.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "ChatPreset")]
pub struct Preset {
    pub id: &'static str,
    pub base_url: &'static str,
    pub model: &'static str,
    /// Runs on this computer: no key, nothing leaves the machine, and a fallback candidate.
    pub local: bool,
}

/// The user picks one and may edit the URL and model afterwards ("custom").
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "deepseek",
        base_url: "https://api.deepseek.com/v1",
        model: "deepseek-chat",
        local: false,
    },
    Preset {
        id: "qwen",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        model: "qwen-plus",
        local: false,
    },
    Preset {
        id: "ollama",
        base_url: "http://127.0.0.1:11434/v1",
        model: "qwen2.5:7b",
        local: true,
    },
    Preset {
        id: "lmstudio",
        base_url: "http://127.0.0.1:1234/v1",
        model: "local-model",
        local: true,
    },
    Preset {
        id: "llamacpp",
        base_url: "http://127.0.0.1:8080/v1",
        model: "local-model",
        local: true,
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_local_means_this_machine() {
        let mut ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len());
        for p in PRESETS {
            assert_eq!(
                p.local,
                p.base_url.starts_with("http://127.0.0.1:"),
                "{}",
                p.id
            );
            assert!(p.local || p.base_url.starts_with("https://"), "{}", p.id);
        }
        assert_eq!(preset("ollama").map(|p| p.local), Some(true));
    }
}
