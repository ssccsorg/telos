//! Deterministic language model backend for contract tests.
//!
//! Activated with `TELOS_STUB_BACKEND=1`. Every prompt is answered with a
//! fixed message, so the actus contract (thread lifecycle, mention format,
//! reconnect, concurrency) can be verified without an LLM API key and
//! without model variance. The response text is configurable through
//! `TELOS_STUB_RESPONSE` (defaults to "OK").

use futures::{channel::mpsc, future::BoxFuture, stream::StreamExt as _, FutureExt as _};
use gpui::{App, AsyncApp, Entity, Task};
use http_client::Result;
use language_model::{
    AuthenticateError, LanguageModel, LanguageModelClient, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelCompletionStream, LanguageModelId,
    LanguageModelName, LanguageModelProvider, LanguageModelProviderId, LanguageModelProviderName,
    LanguageModelProviderState, LanguageModelRequest, StopReason,
};

pub struct StubBackendProvider {
    id: LanguageModelProviderId,
    name: LanguageModelProviderName,
    model: LanguageModel,
}

impl Default for StubBackendProvider {
    fn default() -> Self {
        let id = LanguageModelProviderId::from("stub-backend".to_string());
        let name = LanguageModelProviderName::from("Stub Backend".to_string());
        Self {
            model: LanguageModel::new(
                LanguageModelId::from("stub-backend-model".to_string()),
                LanguageModelName::from("Stub Backend Model".to_string()),
                id.clone(),
                name.clone(),
                "stub-backend",
                8192,
            ),
            id,
            name,
        }
    }
}

impl LanguageModelProviderState for StubBackendProvider {
    type ObservableEntity = ();

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        None
    }
}

impl LanguageModelProvider for StubBackendProvider {
    fn id(&self) -> LanguageModelProviderId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelProviderName {
        self.name.clone()
    }

    fn default_model(&self, _cx: &App) -> Option<LanguageModel> {
        Some(self.model.clone())
    }

    fn default_fast_model(&self, _cx: &App) -> Option<LanguageModel> {
        Some(self.model.clone())
    }

    fn provided_models(&self, _cx: &App) -> Vec<LanguageModel> {
        vec![self.model.clone()]
    }

    fn is_authenticated(&self, _cx: &App) -> bool {
        true
    }

    fn authenticate(&self, _cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        Task::ready(Ok(()))
    }

    fn settings_view(&self, _cx: &mut App) -> Option<language_model::ProviderSettingsView> {
        None
    }
}

impl LanguageModelClient for StubBackendProvider {
    fn stream_completion(
        &self,
        _model: &LanguageModel,
        _request: LanguageModelRequest,
        _cx: &AsyncApp,
    ) -> BoxFuture<'static, Result<LanguageModelCompletionStream, LanguageModelCompletionError>>
    {
        let response = std::env::var("TELOS_STUB_RESPONSE").unwrap_or_else(|_| "OK".to_string());
        async move {
            let (tx, rx) = mpsc::unbounded();
            tx.unbounded_send(Ok(LanguageModelCompletionEvent::StartMessage {
                message_id: "stub-message-1".into(),
            }))
            .ok();
            tx.unbounded_send(Ok(LanguageModelCompletionEvent::Text(response)))
                .ok();
            tx.unbounded_send(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)))
                .ok();
            Ok(rx.boxed())
        }
        .boxed()
    }
}
