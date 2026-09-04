//! Deterministic language model backend for contract tests.
//!
//! Activated with `TELOS_STUB_BACKEND=1`. Every prompt is answered with a
//! fixed message, so the actus contract (thread lifecycle, mention format,
//! reconnect, concurrency) can be verified without an LLM API key and
//! without model variance. The response text is configurable through
//! `TELOS_STUB_RESPONSE` (defaults to "OK").

use std::sync::Arc;

use futures::{
    channel::mpsc,
    future::BoxFuture,
    stream::{BoxStream, StreamExt as _},
    FutureExt as _,
};
use gpui::{App, AsyncApp, Entity, Task};
use http_client::Result;
use language_model::{
    AuthenticateError, LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelId, LanguageModelName, LanguageModelProvider, LanguageModelProviderId,
    LanguageModelProviderName, LanguageModelProviderState, LanguageModelRequest,
    LanguageModelToolChoice, StopReason,
};

pub struct StubBackendProvider {
    id: LanguageModelProviderId,
    name: LanguageModelProviderName,
    model: Arc<StubBackendModel>,
}

impl Default for StubBackendProvider {
    fn default() -> Self {
        Self {
            id: LanguageModelProviderId::from("stub-backend".to_string()),
            name: LanguageModelProviderName::from("Stub Backend".to_string()),
            model: Arc::new(StubBackendModel::default()),
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

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.model.clone())
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.model.clone())
    }

    fn provided_models(&self, _cx: &App) -> Vec<Arc<dyn LanguageModel>> {
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

pub struct StubBackendModel {
    id: LanguageModelId,
    name: LanguageModelName,
}

impl Default for StubBackendModel {
    fn default() -> Self {
        Self {
            id: LanguageModelId::from("stub-backend-model".to_string()),
            name: LanguageModelName::from("Stub Backend Model".to_string()),
        }
    }
}

impl LanguageModel for StubBackendModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        self.name.clone()
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        LanguageModelProviderId::from("stub-backend".to_string())
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        LanguageModelProviderName::from("Stub Backend".to_string())
    }

    fn telemetry_id(&self) -> String {
        "stub-backend".to_string()
    }

    fn supports_images(&self) -> bool {
        false
    }

    fn supports_tools(&self) -> bool {
        false
    }

    fn supports_tool_choice(&self, _choice: LanguageModelToolChoice) -> bool {
        false
    }

    fn max_token_count(&self) -> u64 {
        8192
    }

    fn stream_completion(
        &self,
        _request: LanguageModelRequest,
        _cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>,
            LanguageModelCompletionError,
        >,
    > {
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
