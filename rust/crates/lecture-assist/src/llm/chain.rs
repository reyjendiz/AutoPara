//! Try providers in order, falling back when one is busy or unreachable.

use std::sync::Arc;

use futures_util::future::BoxFuture;

use super::{ChatProvider, Message, ReplyStream};
use crate::AssistError;

/// The user's provider first, then a local one. A provider that has started replying is not
/// abandoned: switching mid-reply would show two half answers.
pub struct ProviderChain(pub Vec<Arc<dyn ChatProvider>>);

impl ChatProvider for ProviderChain {
    fn stream(&self, messages: Vec<Message>) -> BoxFuture<'_, Result<ReplyStream, AssistError>> {
        Box::pin(async move {
            let mut last = AssistError::ChatUnavailable("no provider configured".into());
            for provider in &self.0 {
                match provider.stream(messages.clone()).await {
                    Err(AssistError::ChatUnavailable(reason)) => {
                        tracing::warn!(%reason, "chat provider unavailable, trying the next");
                        last = AssistError::ChatUnavailable(reason);
                    }
                    other => return other,
                }
            }
            Err(last)
        })
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;

    use super::*;
    use crate::llm::complete;

    enum Fake {
        Busy,
        Broken,
        Says(&'static str),
    }

    impl ChatProvider for Fake {
        fn stream(&self, _: Vec<Message>) -> BoxFuture<'_, Result<ReplyStream, AssistError>> {
            let result = match self {
                Self::Busy => Err(AssistError::ChatUnavailable("429".into())),
                Self::Broken => Err(AssistError::Chat("401".into())),
                Self::Says(text) => Ok(futures_util::stream::iter([Ok(text.to_string())]).boxed()),
            };
            Box::pin(async move { result })
        }
    }

    fn chain(providers: Vec<Fake>) -> ProviderChain {
        ProviderChain(
            providers
                .into_iter()
                .map(|p| Arc::new(p) as Arc<dyn ChatProvider>)
                .collect(),
        )
    }

    async fn ask(chain: &ProviderChain) -> Result<String, AssistError> {
        complete(chain, vec![Message::user("hi")]).await
    }

    #[tokio::test]
    async fn falls_back_when_busy() {
        let reply = ask(&chain(vec![Fake::Busy, Fake::Says("local")])).await;
        assert_eq!(reply.ok().as_deref(), Some("local"));
    }

    #[tokio::test]
    async fn the_first_that_answers_wins() {
        let reply = ask(&chain(vec![Fake::Says("cloud"), Fake::Says("local")])).await;
        assert_eq!(reply.ok().as_deref(), Some("cloud"));
    }

    #[tokio::test]
    async fn other_errors_do_not_fall_back() {
        let reply = ask(&chain(vec![Fake::Broken, Fake::Says("local")])).await;
        assert!(matches!(reply, Err(AssistError::Chat(_))));
    }

    #[tokio::test]
    async fn all_busy_or_empty_is_unavailable() {
        assert!(matches!(
            ask(&chain(vec![Fake::Busy, Fake::Busy])).await,
            Err(AssistError::ChatUnavailable(_))
        ));
        assert!(matches!(
            ask(&chain(vec![])).await,
            Err(AssistError::ChatUnavailable(_))
        ));
    }
}
