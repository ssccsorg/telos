//! Minimal headless agent binary.
//!
//! Wires only the agent core (gpui headless + project + native agent +
//! external websocket sync) without the editor, workspace, or collab UI
//! stack. This is the strip surgery: the agent core closure does not
//! include those crates, so a binary rooted here excludes them from the
//! build entirely.

use std::sync::Arc;

use anyhow::{Context, Result};
use client::Client;
use fs::{Fs, RealFs};
use gpui::{App, AppContext as _, Application, TaskExt as _};
use settings::Settings as _;
use language::LanguageRegistry;
use client::UserStore;
use node_runtime::{NodeBinaryOptions, NodeRuntime};
use project::{Project, project_settings::ProjectSettings};
use release_channel::AppVersion;
use util::ResultExt as _;
use watch;

fn main() {
    let app = Application::with_platform(gpui_platform::current_platform(true));
    app.run(move |cx| {
        if let Err(e) = run_headless(cx) {
            log::error!("telos-headless: {e:#}");
        }
    });
}

fn run_headless(cx: &mut App) -> Result<()> {
    release_channel::init(semver::Version::new(0, 1, 0), cx);
    settings::init(cx);
    zlog_settings::init(cx);
    feature_flags::FeatureFlagStore::init(cx);
    project::trusted_worktrees::init(Default::default(), cx);

    // HTTP client and collab client.
    let http = reqwest_client::ReqwestClient::new();
    cx.set_http_client(Arc::new(http));
    let client = Client::production(cx);

    // Filesystem.
    let fs: Arc<dyn Fs> = Arc::new(RealFs::new(None, cx.background_executor().clone()));
    <dyn Fs>::set_global(fs.clone(), cx);

    // Language registry.
    let languages = Arc::new(LanguageRegistry::new(cx.background_executor().clone()));
    let node_runtime = NodeRuntime::new(client.http_client(), None, {
        let (mut tx, rx): (
            watch::Sender<Option<NodeBinaryOptions>>,
            watch::Receiver<Option<NodeBinaryOptions>>,
        ) = watch::channel(None);
        tx.send(None).ok();
        rx
    });

    let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));

    // Agent globals and model providers.
    language_model::init(cx);
    agent::ThreadStore::init_global(cx);
    prompt_store::init(cx);
    language_models::init(user_store.clone(), client.clone(), cx);

    // Headless project with trusted worktrees.
    let mut project_settings = ProjectSettings::get_global(cx).clone();
    project_settings.session.trust_all_worktrees = true;
    ProjectSettings::override_global(project_settings, cx);

    let project = Project::local(
        client.clone(),
        node_runtime,
        user_store,
        languages,
        fs.clone(),
        None,
        project::LocalProjectFlags {
            init_worktree_trust: true,
            watch_global_configs: false,
        },
        cx,
    );

    // Worktree from the CLI path (first positional arg).
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if let Some(path) = paths.into_iter().find(|p| !p.starts_with('-')) {
        let project_clone = project.clone();
        let fs_clone = fs.clone();
        cx.spawn(async move |cx| {
            let path = std::path::Path::new(&path);
            if path.exists() {
                let canonical = fs_clone
                    .canonicalize(path)
                    .await
                    .unwrap_or_else(|_| path.to_path_buf());
                cx.update(|cx| {
                    project_clone
                        .update(cx, |project, cx| {
                            project.find_or_create_worktree(&canonical, true, cx)
                        })
                        .detach_and_log_err(cx);
                });
            }
        })
        .detach();
    }

    // External websocket sync: thread handler + websocket service.
    let thread_store = agent::ThreadStore::global(cx);
    external_websocket_sync::setup_thread_handler(project.clone(), thread_store, fs.clone(), cx);

    let sync_settings = external_websocket_sync::ExternalSyncSettings::get_global(cx);
    if sync_settings.enabled && sync_settings.websocket_sync.enabled {
        let config = external_websocket_sync::WebSocketSyncConfig {
            enabled: true,
            url: sync_settings.websocket_sync.external_url.clone(),
            auth_token: sync_settings
                .websocket_sync
                .auth_token
                .clone()
                .unwrap_or_default(),
            use_tls: sync_settings.websocket_sync.use_tls,
            skip_tls_verify: sync_settings.websocket_sync.skip_tls_verify,
        };
        external_websocket_sync::init_websocket_service(config);
        log::info!("websocket sync service initialized");
    }

    log::info!("telos-headless running");
    Ok(())
}
