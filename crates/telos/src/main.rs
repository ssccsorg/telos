//! Minimal headless agent binary.
//!
//! Wires only the agent core (gpui headless + project + native agent +
//! external websocket sync) without the editor, workspace, or collab UI
//! stack. This is the strip surgery: the agent core closure does not
//! include those crates, so a binary rooted here excludes them from the
//! build entirely.

mod fake_backend;

use std::sync::Arc;

use anyhow::Result;
use client::{Client, UserStore};
use fs::{Fs, RealFs};
use gpui::{App, AppContext as _, Application, TaskExt as _};
use language::LanguageRegistry;
use node_runtime::{NodeBinaryOptions, NodeRuntime};
use project::{project_settings::ProjectSettings, Project};
use settings::Settings as _;
use watch;

/// Actus drives telos through a launch contract of `TELOS_*` environment
/// variables. The zed layer below reads its own `ZED_*` names, so the
/// contract names are translated here, before any module initializes.
fn map_launch_env() {
    const PAIRS: &[(&str, &str)] = &[
        ("TELOS_EXTERNAL_SYNC_ENABLED", "ZED_EXTERNAL_SYNC_ENABLED"),
        ("TELOS_WEBSOCKET_SYNC_ENABLED", "ZED_WEBSOCKET_SYNC_ENABLED"),
        ("TELOS_WS_URL", "ZED_HELIX_URL"),
        ("TELOS_WS_TOKEN", "ZED_HELIX_TOKEN"),
        ("TELOS_WS_TLS", "ZED_HELIX_TLS"),
        ("TELOS_WS_SKIP_TLS_VERIFY", "ZED_HELIX_SKIP_TLS_VERIFY"),
        ("TELOS_STATELESS", "ZED_STATELESS"),
        ("TELOS_SESSION_ID", "HELIX_SESSION_ID"),
        ("TELOS_TOOL_APPROVAL", "ZED_TOOL_APPROVAL"),
    ];
    for (contract_name, zed_name) in PAIRS {
        if let Ok(value) = std::env::var(contract_name) {
            std::env::set_var(zed_name, value);
        }
    }
}

fn main() {
    map_launch_env();
    // The shell-env capture invokes this binary with `--printenv` to dump
    // the login-shell environment. Handle it before any app initialization:
    // otherwise the child runs as a second headless agent, connects to the
    // actus WebSocket, and never exits, leaving an orphan that fights the
    // real agent for the single-server connection.
    if std::env::args().any(|a| a == "--printenv") {
        util::shell_env::print_env();
        return;
    }

    // Headless runtime: gpui::HeadlessPlatform over ThreadedDispatcher. The
    // windowed platform backends (gpui_macos/gpui_linux via gpui_platform) are
    // deliberately out of the agent graph (issue #3).
    let app = Application::with_platform(gpui::HeadlessPlatform::new());
    app.run(move |cx| {
        if let Err(e) = run_headless(cx) {
            log::error!("telos: {e:#}");
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

    // Deterministic test backend: respond to every prompt with a fixed
    // message so the actus contract can be verified without an LLM API key
    // and without model variance. When active, skip the real providers so
    // the fake is the only authenticated provider.
    let fake_backend = std::env::var("TELOS_FAKE_BACKEND").is_ok();
    if fake_backend {
        use language_model::LanguageModelRegistry;
        let fake = Arc::new(crate::fake_backend::FakeBackendProvider::default());
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(fake, cx);
            registry.set_should_use_fallback(true);
        });
    } else {
        language_models::init(user_store.clone(), client.clone(), cx);
    }

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

    // Worktree from the CLI path. Actus launches with
    // `--headless --allow-multiple-instances --user-data-dir <dir> <workdir>`,
    // so flag values must be skipped before picking the positional workdir.
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let mut workdir_path: Option<std::path::PathBuf> = None;
    let mut i = 0;
    while i < raw_args.len() {
        match raw_args[i].as_str() {
            "--user-data-dir" => {
                i += 2; // skip the flag and its value
                continue;
            }
            flag if flag.starts_with('-') => {}
            path => workdir_path = Some(std::path::PathBuf::from(path)),
        }
        i += 1;
    }
    if let Some(path) = workdir_path {
        let project_clone = project.clone();
        let fs_clone = fs.clone();
        cx.spawn(async move |cx| {
            if path.exists() {
                let canonical = fs_clone
                    .canonicalize(&path)
                    .await
                    .unwrap_or_else(|_| path.clone());
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

    log::info!("telos running");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::map_launch_env;

    // Env mutation is process-global; serialize the two tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const TELOS_KEYS: [&str; 7] = [
        "TELOS_EXTERNAL_SYNC_ENABLED",
        "TELOS_WEBSOCKET_SYNC_ENABLED",
        "TELOS_WS_URL",
        "TELOS_WS_TOKEN",
        "TELOS_STATELESS",
        "TELOS_SESSION_ID",
        "TELOS_TOOL_APPROVAL",
    ];
    const MAPPED_KEYS: [&str; 7] = [
        "ZED_EXTERNAL_SYNC_ENABLED",
        "ZED_WEBSOCKET_SYNC_ENABLED",
        "ZED_HELIX_URL",
        "ZED_HELIX_TOKEN",
        "ZED_STATELESS",
        "HELIX_SESSION_ID",
        "ZED_TOOL_APPROVAL",
    ];

    fn clear_env() {
        for key in TELOS_KEYS.iter().chain(MAPPED_KEYS.iter()) {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn maps_telos_launch_env_onto_vendored_names() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        let values = ["true", "true", "127.0.0.1:8080", "test-token", "1", "ses_t", "always"];
        for (key, value) in TELOS_KEYS.iter().zip(values.iter()) {
            std::env::set_var(key, value);
        }

        map_launch_env();

        for (mapped, value) in MAPPED_KEYS.iter().zip(values.iter()) {
            assert_eq!(
                std::env::var(mapped).ok().as_deref(),
                Some(*value),
                "mapped env {mapped}"
            );
        }
        clear_env();
    }

    #[test]
    fn does_not_map_absent_telos_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        map_launch_env();
        for key in MAPPED_KEYS {
            assert!(
                std::env::var_os(key).is_none(),
                "{key} must stay unset without a TELOS_* source"
            );
        }
        clear_env();
    }
}
