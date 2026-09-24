//! Minimal headless agent binary.
//!
//! Wires only the agent core (gpui headless + project + native agent +
//! external websocket sync) without the editor, workspace, or collab UI
//! stack. This is the strip surgery: the agent core closure does not
//! include those crates, so a binary rooted here excludes them from the
//! build entirely.

mod stub_backend;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use client::{Client, UserStore};
use fs::{Fs, RealFs};
use futures::StreamExt as _;
use gpui::{App, AppContext as _, Application, TaskExt as _, UpdateGlobal as _};
use language::LanguageRegistry;
use node_runtime::{NodeBinaryOptions, NodeRuntime};
use project::{trusted_worktrees::DbTrustedPaths, Project};
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

/// Extracts the `--user-data-dir` value from the process arguments.
///
/// Both `--user-data-dir <path>` and `--user-data-dir=<path>` forms are
/// accepted. The actus launch contract passes the flag so the agent reads the
/// settings and credentials actus wrote under `<dir>/config` and
/// `<dir>/credentials`; the flag value must be consumed before any settings or
/// paths code resolves the default data directory.
fn user_data_dir_from_args(args: &[String]) -> Option<String> {
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix("--user-data-dir=") {
            return Some(value.to_string());
        }
        if arg == "--user-data-dir" {
            return args.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}

/// Extracts the positional workdir from the process arguments.
///
/// Actus launches with `--headless --allow-multiple-instances --user-data-dir <dir>
/// <workdir>`, so a flag value must be skipped rather than read as the path. The result is
/// canonicalized when the directory exists, because the worktree is created from its
/// canonical form and a trust entry has to name the same directory to cover it.
fn workdir_from_args(args: &[String]) -> Option<PathBuf> {
    let mut workdir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--user-data-dir" => index += 2,
            flag if flag.starts_with('-') => index += 1,
            path => {
                workdir = Some(PathBuf::from(path));
                index += 1;
            }
        }
    }
    workdir.map(|path| std::fs::canonicalize(&path).unwrap_or(path))
}

/// The trust a headless start declares, in the shape [`project::trusted_worktrees::init`]
/// expects.
///
/// A headless agent has nobody to answer the trust question a fresh worktree raises, and an
/// untrusted worktree restricts the workspace: the terminal and `fetch` are withheld from
/// the model, and worktree-local skills and settings are held back. The launch contract
/// names the one directory the agent was pointed at, so trusting that path is the whole of
/// the decision.
///
/// This cannot ride on a setting. `SettingsStore::override_global` is documented as
/// overwritten when the settings change, and a recomputation happens inside a session,
/// which is enough to take the terminal away mid-turn.
fn trusted_paths_for(workdir: Option<&Path>) -> DbTrustedPaths {
    match workdir {
        Some(path) => HashMap::from_iter([(None, HashSet::from_iter([path.to_path_buf()]))]),
        None => DbTrustedPaths::default(),
    }
}

/// Load the settings file the operator wrote, and keep following it.
///
/// Actus writes the agent's half-configured half under `--user-data-dir`: the model
/// and its reasoning effort, the terminal's tool permission, and the MCP servers in
/// `context_servers`. The editor loads that file and watches it; this binary was cut
/// down from the editor, and the load went with it, so every one of those settings
/// stayed on disk while the agent ran on the compiled-in defaults. The file is the
/// operator's input, so it is read here.
///
/// The read is not blocking and the file does not have to exist when the agent starts:
/// the watcher reports the file's current content as soon as it can, and every later
/// change after that. A directory that does not exist is created, because the watcher
/// needs somewhere to watch; a file that never appears leaves the defaults in place,
/// which is what a bare `tel` run with no operator configuration should get.
fn watch_settings_file(fs: Arc<dyn Fs>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let config_dir = paths::config_dir().clone();
        if let Err(error) = fs.create_dir(&config_dir).await {
            log::error!("telos: cannot create {}: {error}", config_dir.display());
            return;
        }

        let settings_path = paths::settings_file().clone();
        let (mut content_rx, _watcher) =
            settings::watch_config_file(cx.background_executor(), fs, settings_path.clone());

        while let Some(content) = content_rx.next().await {
            let applied = cx.update(|cx| {
                settings::SettingsStore::update_global(cx, |store, cx| {
                    store.set_user_settings(&content, cx).result()
                })
            });
            match applied {
                Ok(_) => log::info!("telos: read {}", settings_path.display()),
                Err(error) => log::error!("telos: ignoring {}: {error}", settings_path.display()),
            }
        }
    })
    .detach();
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

    // Actus launches the agent with `--user-data-dir <dir>` and writes the
    // settings bootstrap (LLM provider, `context_servers` MCP entries) and
    // credentials under that directory. Pin the custom data directory before
    // `settings::init` resolves `paths::config_dir`/`data_dir`, so the
    // actus-injected configuration is actually loaded.
    let args: Vec<String> = std::env::args().collect();
    if let Some(data_dir) = user_data_dir_from_args(&args) {
        paths::set_custom_data_dir(&data_dir);
    }

    // `zlog` is the logger this crate family uses, and nothing installed it in
    // this binary, so every `log::` record in the agent was discarded: the error
    // paths in this file, and everything the agent core reports when a turn
    // cannot start, had no destination. `zlog_settings::init` below only adjusts
    // a filter that had nothing to filter. stderr rather than stdout, because
    // `--printenv` owns stdout, and the caller parses it.
    zlog::init();
    zlog::init_output_stderr();

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

    // Trust is settled before the project exists. The entries are consumed when the
    // worktree store is registered, and a `can_trust` call that lands first would mark the
    // worktree restricted instead.
    let workdir = workdir_from_args(&std::env::args().skip(1).collect::<Vec<_>>());
    project::trusted_worktrees::init(trusted_paths_for(workdir.as_deref()), cx);

    // HTTP client and collab client.
    let http = reqwest_client::ReqwestClient::new();
    cx.set_http_client(Arc::new(http));
    let client = Client::production(cx);

    // Filesystem.
    let fs: Arc<dyn Fs> = RealFs::new(None, cx.background_executor().clone());
    <dyn Fs>::set_global(fs.clone(), cx);

    // The configuration actus wrote under `--user-data-dir` is what the operator
    // chose for this agent, and `settings::init` above reads none of it.
    watch_settings_file(fs.clone(), cx);

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
    // the stub is the only authenticated provider.
    let stub_backend = std::env::var("TELOS_STUB_BACKEND").is_ok();
    if stub_backend {
        use language_model::LanguageModelRegistry;
        let stub = Arc::new(crate::stub_backend::StubBackendProvider::default());
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(stub, cx);
            registry.set_should_use_fallback(true);
        });
    } else {
        language_models::init(user_store.clone(), client.clone(), cx);
    }

    // Headless project. The worktrees it opens are trusted through the store seeded above,
    // not through a setting: `session.trust_all_worktrees` is dropped on the next settings
    // recomputation, and the restriction that follows withholds the terminal from the model.
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

    // Worktree from the CLI path, the same path the trust seed above names.
    if let Some(path) = workdir {
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
    use super::{
        map_launch_env, trusted_paths_for, user_data_dir_from_args, workdir_from_args,
        DbTrustedPaths, HashSet, Path, PathBuf,
    };

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
    fn the_launch_workdir_is_the_only_trusted_path() {
        // The subdirectory does not exist, so the canonicalization falls back to the
        // argument itself and the assertion does not depend on the host's symlinks.
        let workdir_path = "/tmp/kletos-test-does-not-exist/workdir";
        let args: Vec<String> = [
            "--headless",
            "--allow-multiple-instances",
            "--user-data-dir",
            "/tmp/kletos-test-user-data",
            workdir_path,
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();

        let workdir = workdir_from_args(&args);
        assert_eq!(workdir.as_deref(), Some(Path::new(workdir_path)));

        let mut expected = DbTrustedPaths::default();
        expected.insert(None, HashSet::from_iter([PathBuf::from(workdir_path)]));
        assert_eq!(trusted_paths_for(workdir.as_deref()), expected);

        // A start with no workdir declares no trust. It also opens no visible worktree, so
        // there is nothing a restriction could apply to.
        assert!(trusted_paths_for(None).is_empty());
        assert!(workdir_from_args(&["--headless".to_string()]).is_none());
    }

    #[test]
    fn maps_telos_launch_env_onto_vendored_names() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        let values = [
            "true",
            "true",
            "127.0.0.1:8080",
            "test-token",
            "1",
            "ses_t",
            "always",
        ];
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

    #[test]
    fn parses_user_data_dir_from_separate_argument() {
        let args = [
            "tel".to_string(),
            "--headless".to_string(),
            "--user-data-dir".to_string(),
            "/tmp/actus-agent".to_string(),
            "/workspace".to_string(),
        ];
        assert_eq!(
            user_data_dir_from_args(&args).as_deref(),
            Some("/tmp/actus-agent")
        );
    }

    #[test]
    fn parses_user_data_dir_from_equals_argument() {
        let args = [
            "tel".to_string(),
            "--user-data-dir=/tmp/actus-agent".to_string(),
        ];
        assert_eq!(
            user_data_dir_from_args(&args).as_deref(),
            Some("/tmp/actus-agent")
        );
    }

    #[test]
    fn returns_none_without_user_data_dir() {
        let args = ["tel".to_string(), "--printenv".to_string()];
        assert_eq!(user_data_dir_from_args(&args), None);
    }

    #[test]
    fn returns_none_when_user_data_dir_flag_is_last() {
        let args = ["tel".to_string(), "--user-data-dir".to_string()];
        assert_eq!(user_data_dir_from_args(&args), None);
    }

    /// The shipped default settings, read from the crate's own path.
    ///
    /// `SettingsStore::test` reads them through the asset loader, whose dev arm reads from
    /// a checkout and panics where there is no `.git` above the binary. That is how the
    /// image build runs its gate, so the file is read here instead. It is the same file the
    /// loader would have read, and the path is resolved from the manifest directory, so it
    /// is the same wherever the source tree is.
    fn shipped_defaults() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/settings/default.json"
        ))
        .expect("the shipped default settings are in the checkout")
    }

    /// The operator's settings file is the half of an agent's configuration that is not
    /// compiled in: actus writes it before the agent starts. The headless binary used to
    /// run without ever reading it, so an MCP server, a tool permission or a model the
    /// operator set had no effect. This pins the read.
    ///
    /// Building a store reads two assets, and neither read is what this test is about, so
    /// both are supplied here: the default settings from the crate's own path, and the
    /// semantic token rules from a global set before the store is built. That keeps the
    /// test runnable in an image whose source was copied without VCS metadata.
    #[gpui::test]
    async fn the_operators_settings_file_is_loaded(cx: &mut gpui::TestAppContext) {
        use fs::Fs as _;
        use settings::Settings as _;

        let settings_path = paths::settings_file().clone();
        let fs = fs::FakeFs::new(cx.background_executor.clone());
        fs.create_dir(settings_path.parent().unwrap())
            .await
            .unwrap();
        fs.insert_file(
            settings_path,
            br#"{"context_servers":{"memory":{"command":"/bin/true","enabled":true}}}"#.to_vec(),
        )
        .await;

        cx.update(|cx| {
            cx.set_global(settings::DefaultSemanticTokenRules(
                settings::SemanticTokenRules::default(),
            ));
            let store = settings::SettingsStore::new(cx, &shipped_defaults());
            cx.set_global(store);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
            super::watch_settings_file(fs.clone(), cx);
        });
        cx.run_until_parked();

        cx.update(|cx| {
            let servers =
                &project::project_settings::ProjectSettings::get_global(cx).context_servers;
            assert!(
                servers.get("memory").is_some_and(|server| server.enabled()),
                "the settings file names an MCP server that must be enabled: {servers:?}"
            );
        });
    }
}
