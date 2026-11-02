//! The chat provider layer (plan step 7.6).

pub mod chain;
pub mod local;
pub mod openai;
pub mod presets;
pub mod rate;
pub mod sse;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::AssistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
}

/// The pieces of a reply as they arrive.
pub type ReplyStream = BoxStream<'static, Result<String, AssistError>>;

pub trait ChatProvider: Send + Sync {
    /// Start a streamed reply. Fails with [`AssistError::ChatUnavailable`] when the next
    /// provider in a chain should be tried instead.
    fn stream(&self, messages: Vec<Message>) -> BoxFuture<'_, Result<ReplyStream, AssistError>>;
}

/// The whole reply as one string.
pub async fn complete(
    provider: &dyn ChatProvider,
    messages: Vec<Message>,
) -> Result<String, AssistError> {
    let mut stream = provider.stream(messages).await?;
    let mut reply = String::new();
    while let Some(piece) = stream.next().await {
        reply.push_str(&piece?);
    }
    Ok(reply)
}

/// Rate limits and server faults are worth another provider; anything else (a wrong key, a
/// wrong model name) is the user's to fix.
pub fn http_error(status: u16, body: &str) -> AssistError {
    if status == 429 || status >= 500 {
        AssistError::ChatUnavailable(format!("HTTP {status}"))
    } else {
        let body: String = body.chars().take(200).collect();
        AssistError::Chat(format!("HTTP {status}: {body}"))
    }
}

/// Timeouts and failed connections are worth another provider. The URL is left out of the
/// message: some servers take the key in the query string.
pub fn transport_error(error: reqwest::Error) -> AssistError {
    let unavailable = error.is_timeout() || error.is_connect();
    let text = error.without_url().to_string();
    if unavailable {
        AssistError::ChatUnavailable(text)
    } else {
        AssistError::Chat(text)
    }
}

/// rustls needs one process-wide crypto provider; the updater installs the same one.
pub fn install_tls() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Canned(Vec<&'static str>);

    impl ChatProvider for Canned {
        fn stream(
            &self,
            _messages: Vec<Message>,
        ) -> BoxFuture<'_, Result<ReplyStream, AssistError>> {
            let pieces: Vec<Result<String, AssistError>> =
                self.0.iter().map(|p| Ok(p.to_string())).collect();
            Box::pin(async move { Ok(futures_util::stream::iter(pieces).boxed()) })
        }
    }

    #[tokio::test]
    async fn complete_joins_the_pieces() {
        let reply = complete(&Canned(vec!["При", "віт"]), vec![Message::user("hi")]).await;
        assert_eq!(reply.ok().as_deref(), Some("Привіт"));
    }

    #[test]
    fn busy_servers_fall_back_and_bad_requests_do_not() {
        assert!(matches!(
            http_error(429, ""),
            AssistError::ChatUnavailable(_)
        ));
        assert!(matches!(
            http_error(503, ""),
            AssistError::ChatUnavailable(_)
        ));
        assert!(matches!(http_error(401, "bad key"), AssistError::Chat(_)));
    }

    #[test]
    fn messages_serialise_as_openai_expects() {
        let json = serde_json::to_string(&Message::system("x")).unwrap_or_default();
        assert_eq!(json, r#"{"role":"system","content":"x"}"#);
    }
}
