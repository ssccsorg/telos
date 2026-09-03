use std::sync::Arc;

use anyhow::Result;
use convert_case::{Case, Casing};
use credentials_provider::CredentialsProvider;
use gpui::{App, AppContext as _, Context, Entity, SharedString, Task};
use language_model::{ApiKeyState, AuthenticateError, EnvVar};
use settings::SettingsStore;

pub trait ApiCompatibleProviderSettings: Clone + Default + PartialEq + 'static {
    fn api_url(&self) -> &str;
}

pub struct ApiCompatibleProviderState<S: ApiCompatibleProviderSettings> {
    id: Arc<str>,
    pub api_key_state: ApiKeyState,
    pub settings: S,
    credentials_provider: Arc<dyn CredentialsProvider>,
}

impl<S: ApiCompatibleProviderSettings> ApiCompatibleProviderState<S> {
    pub fn new(
        id: Arc<str>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        resolve_settings: for<'a> fn(&'a str, &'a App) -> Option<&'a S>,
        cx: &mut App,
    ) -> Entity<Self> {
        let api_key_env_var_name: SharedString =
            format!("{}_API_KEY", id).to_case(Case::UpperSnake).into();
        cx.new(|cx| {
            cx.observe_global::<SettingsStore>(move |this: &mut Self, cx| {
                let Some(settings) = resolve_settings(&this.id, cx).cloned() else {
                    return;
                };
                this.update_settings(settings, cx);
            })
            .detach();

            let settings = resolve_settings(&id, cx).cloned().unwrap_or_default();
            Self {
                id,
                api_key_state: ApiKeyState::new(
                    SharedString::new(settings.api_url()),
                    EnvVar::new(api_key_env_var_name),
                ),
                settings,
                credentials_provider,
            }
        })
    }

    pub fn is_authenticated(&self) -> bool {
        self.api_key_state.has_key()
    }

    pub fn set_api_key(
        &mut self,
        api_key: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let api_url = SharedString::new(self.settings.api_url());
        self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            self.credentials_provider.clone(),
            cx,
        )
    }

    pub fn authenticate(&mut self, cx: &mut Context<Self>) -> Task<Result<(), AuthenticateError>> {
        let api_url = SharedString::new(self.settings.api_url());
        self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            self.credentials_provider.clone(),
            cx,
        )
    }

    pub fn update_settings(&mut self, settings: S, cx: &mut Context<Self>) {
        if self.settings != settings {
            let api_url = SharedString::new(settings.api_url());
            self.api_key_state.handle_url_change(
                api_url,
                |this| &mut this.api_key_state,
                self.credentials_provider.clone(),
                cx,
            );
            self.settings = settings;
            cx.notify();
        }
    }
}
