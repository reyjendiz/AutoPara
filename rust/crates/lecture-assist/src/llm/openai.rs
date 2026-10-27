//! One client for every OpenAI-compatible server: DeepSeek, Qwen, Ollama, llama.cpp, LM Studio.

use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use super::sse::SseParser;
use super::{ChatProvider, Message, ReplyStream, http_error, install_tls, transport_error};
use crate::AssistError;

/// No `Debug`: it holds the API key.
pub struct OpenAiCompatible {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    temperature: f32,
}

impl OpenAiCompatible {
    /// `timeout` bounds each wait for data, so a long reply may stream as long as it keeps
    /// coming, while a stalled server falls back.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
        temperature: f32,
        timeout: Duration,
    ) -> Result<Self, AssistError> {
        install_tls();
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(timeout)
            .build()
            .map_err(transport_error)?;
        Ok(Self {
            client,
            base_url: base_url.into(),
            model: model.into(),
            api_key: api_key.filter(|k| !k.is_empty()),
            temperature,
        })
    }
}

impl ChatProvider for OpenAiCompatible {
    fn stream(&self, messages: Vec<Message>) -> BoxFuture<'_, Result<ReplyStream, AssistError>> {
        Box::pin(async move {
            let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
            let body = json!({
                "model": self.model,
                "messages": messages,
                "temperature": self.temperature,
                "stream": true,
            });
            let mut request = self.client.post(url).json(&body);
            if let Some(key) = &self.api_key {
                request = request.bearer_auth(key);
            }
            let response = request.send().await.map_err(transport_error)?;
            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                return Err(http_error(status.as_u16(), &text));
            }
            let mut parser = SseParser::default();
            let pieces = response
                .bytes_stream()
                .map(move |chunk| match chunk {
                    Ok(bytes) => parser
                        .push(&bytes)
                        .iter()
                        .filter_map(|d| delta(d))
                        .collect(),
                    Err(error) => vec![Err(transport_error(error))],
                })
                .flat_map(futures_util::stream::iter);
            Ok(pieces.boxed())
        })
    }
}

/// The text in one stream event, if any.
fn delta(data: &str) -> Option<Result<String, AssistError>> {
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Some(Err(AssistError::Chat("unreadable stream event".into())));
    };
    if let Some(message) = value["error"]["message"].as_str() {
        return Some(Err(AssistError::Chat(message.to_string())));
    }
    value["choices"][0]["delta"]["content"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| Ok(s.to_string()))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::llm::complete;

    fn client(server: &MockServer) -> OpenAiCompatible {
        let base = format!("{}/v1", server.uri());
        OpenAiCompatible::new(base, "m", Some("k".into()), 0.3, Duration::from_secs(5)).unwrap()
    }

    async fn reply_with(template: ResponseTemplate) -> (MockServer, OpenAiCompatible) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer k"))
            .and(body_partial_json(json!({"model": "m", "stream": true})))
            .respond_with(template)
            .mount(&server)
            .await;
        let client = client(&server);
        (server, client)
    }

    #[tokio::test]
    async fn streams_the_reply() {
        let body = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"При\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"віт\"}}]}\n\n\
                    data: [DONE]\n\n";
        let template = ResponseTemplate::new(200).set_body_raw(body, "text/event-stream");
        let (_server, client) = reply_with(template).await;
        let reply = complete(&client, vec![Message::user("hi")]).await;
        assert_eq!(reply.ok().as_deref(), Some("Привіт"));
    }

    #[tokio::test]
    async fn rate_limit_means_try_another() {
        let (_server, client) = reply_with(ResponseTemplate::new(429)).await;
        let result = client.stream(vec![Message::user("hi")]).await;
        assert!(matches!(result, Err(AssistError::ChatUnavailable(_))));
    }

    #[tokio::test]
    async fn a_bad_key_is_reported() {
        let template = ResponseTemplate::new(401).set_body_string("invalid api key");
        let (_server, client) = reply_with(template).await;
        let result = client.stream(vec![Message::user("hi")]).await;
        assert!(matches!(result, Err(AssistError::Chat(m)) if m.contains("401")));
    }

    #[tokio::test]
    async fn an_unreachable_server_means_try_another() {
        let client = OpenAiCompatible::new(
            "http://127.0.0.1:9/v1",
            "m",
            None,
            0.3,
            Duration::from_secs(2),
        )
        .unwrap();
        let result = client.stream(vec![Message::user("hi")]).await;
        assert!(matches!(result, Err(AssistError::ChatUnavailable(_))));
    }
}
