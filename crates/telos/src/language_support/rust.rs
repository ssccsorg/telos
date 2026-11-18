use std::{
    borrow::Cow,
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use futures::StreamExt as _;
use gpui::{AppContext, AsyncApp};
use http_client::github::{latest_github_release, AssetKind, GitHubLspBinaryVersion};
use http_client::github_download::{download_server_binary, GithubBinaryMetadata};
use language::{
    LanguageServerId, LanguageServerName, LspAdapter, LspAdapterDelegate, LspInstaller,
};
use lsp::LanguageServerBinary;
use project::lsp_store::rust_analyzer_ext::CARGO_DIAGNOSTICS_SOURCE_NAME;
use regex::Regex;
use util::command::{new_command, Stdio};
use util::fs::{make_file_executable, remove_matching};

pub struct RustLspAdapter;

const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("rust-analyzer");

#[cfg(target_os = "macos")]
impl RustLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Gz;
    const ARCH_SERVER_NAME: &str = "apple-darwin";
}

#[cfg(target_os = "linux")]
impl RustLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Gz;
    const ARCH_SERVER_NAME: &str = "unknown-linux";
}

#[cfg(target_os = "freebsd")]
impl RustLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Gz;
    const ARCH_SERVER_NAME: &str = "unknown-freebsd";
}

#[cfg(target_os = "windows")]
impl RustLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Zip;
    const ARCH_SERVER_NAME: &str = "pc-windows-msvc";
}

#[cfg(target_os = "linux")]
enum LibcType {
    Gnu,
    Musl,
}

impl RustLspAdapter {
    #[cfg(target_os = "linux")]
    async fn determine_libc_type() -> LibcType {
        use futures::pin_mut;

        async fn from_ldd_version() -> Option<LibcType> {
            let ldd_output = new_command("ldd").arg("--version").output().await.ok()?;
            let ldd_version = String::from_utf8_lossy(&ldd_output.stdout);

            if ldd_version.contains("GNU libc") || ldd_version.contains("GLIBC") {
                Some(LibcType::Gnu)
            } else if ldd_version.contains("musl") {
                Some(LibcType::Musl)
            } else {
                None
            }
        }

        if let Some(libc_type) = from_ldd_version().await {
            return libc_type;
        }

        let Ok(dir_entries) = smol::fs::read_dir("/lib").await else {
            // Defaulting to gnu because nix does not follow FHS.
            return LibcType::Gnu;
        };
        let dir_entries = dir_entries.filter_map(async move |e| e.ok());
        pin_mut!(dir_entries);

        let mut has_musl = false;
        let mut has_gnu = false;

        while let Some(entry) = dir_entries.next().await {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with("ld-musl-") {
                has_musl = true;
            } else if file_name.starts_with("ld-linux-") {
                has_gnu = true;
            }
        }

        match (has_musl, has_gnu) {
            (true, _) => LibcType::Musl,
            (_, true) => LibcType::Gnu,
            _ => LibcType::Gnu,
        }
    }

    #[cfg(target_os = "linux")]
    async fn build_arch_server_name_linux() -> String {
        let libc = match Self::determine_libc_type().await {
            LibcType::Musl => "musl",
            LibcType::Gnu => "gnu",
        };

        format!("{}-{}", Self::ARCH_SERVER_NAME, libc)
    }

    async fn rustup_rust_analyzer_for_worktree(
        delegate: &dyn LspAdapterDelegate,
    ) -> Option<PathBuf> {
        if !Self::workspace_has_rust_toolchain_override(delegate).await {
            return None;
        }

        let rustup = delegate.which("rustup".as_ref()).await?;
        let env = delegate.shell_env().await;
        let worktree_root = delegate.worktree_root_path();
        let output = new_command(rustup)
            .args(["which", "rust-analyzer"])
            .envs(env.iter())
            .current_dir(worktree_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        let output = match output {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                log::debug!(
                    "failed to locate rust-analyzer through rustup in {worktree_root:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return None;
            }
            Err(err) => {
                log::debug!(
                    "failed to run `rustup which rust-analyzer` in {worktree_root:?}: {err:#}"
                );
                return None;
            }
        };

        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        Some(path).filter(|p| !p.as_os_str().is_empty())
    }

    async fn workspace_has_rust_toolchain_override(delegate: &dyn LspAdapterDelegate) -> bool {
        for file_name in ["rust-toolchain.toml", "rust-toolchain"] {
            if smol::fs::metadata(delegate.resolve_relative_path(PathBuf::from(file_name)))
                .await
                .is_ok()
            {
                return true;
            }
        }

        false
    }

    async fn build_asset_name() -> String {
        let extension = match Self::GITHUB_ASSET_KIND {
            AssetKind::TarGz => "tar.gz",
            AssetKind::TarBz2 => "tar.bz2",
            AssetKind::Gz => "gz",
            AssetKind::Zip => "zip",
        };

        #[cfg(target_os = "linux")]
        let arch_server_name = Self::build_arch_server_name_linux().await;
        #[cfg(not(target_os = "linux"))]
        let arch_server_name = Self::ARCH_SERVER_NAME.to_string();

        format!(
            "{}-{}-{}.{}",
            SERVER_NAME,
            std::env::consts::ARCH,
            arch_server_name,
            extension
        )
    }
}

#[async_trait(?Send)]
impl LspAdapter for RustLspAdapter {
    fn name(&self) -> LanguageServerName {
        SERVER_NAME
    }

    fn disk_based_diagnostic_sources(&self) -> Vec<String> {
        vec![CARGO_DIAGNOSTICS_SOURCE_NAME.to_owned()]
    }

    fn disk_based_diagnostics_progress_token(&self) -> Option<String> {
        Some("rust-analyzer/flycheck".into())
    }

    fn process_diagnostics(&self, params: &mut lsp::PublishDiagnosticsParams, _: LanguageServerId) {
        static REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)`([^`]+)\n`$").expect("Failed to create REGEX"));

        for diagnostic in &mut params.diagnostics {
            for message in diagnostic
                .related_information
                .iter_mut()
                .flatten()
                .map(|info| &mut info.message)
                .chain(match &mut diagnostic.message {
                    lsp::DiagnosticMessage::String(message) => Some(message),
                    lsp::DiagnosticMessage::MarkupContent(_) => None,
                })
            {
                if let Cow::Owned(sanitized) = REGEX.replace_all(message, "`$1`") {
                    *message = sanitized;
                }
            }
        }
    }

    fn diagnostic_message_to_markdown(&self, message: &str) -> Option<String> {
        static REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)\n *").expect("Failed to create REGEX"));
        Some(REGEX.replace_all(message, "\n\n").to_string())
    }
}

impl LspInstaller for RustLspAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;
    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<language::Toolchain>,
        cx: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let delegate = delegate.clone();
        cx.background_spawn(async move {
            let env = delegate.shell_env().await;
            if let Some(path) = Self::rustup_rust_analyzer_for_worktree(delegate.as_ref()).await {
                let result = delegate
                    .try_exec(LanguageServerBinary {
                        path: path.clone(),
                        arguments: vec!["--help".into()],
                        env: Some(env.clone()),
                    })
                    .await;
                if result.is_ok() {
                    log::debug!("found rust-analyzer in rustup toolchain override");
                    return Some(LanguageServerBinary {
                        path,
                        env: Some(env),
                        arguments: vec![],
                    });
                }
            }

            let path = delegate.which("rust-analyzer".as_ref()).await?;

            // A `~/.cargo/bin/rust-analyzer` symlink to /usr/bin often fails when run,
            // so it has to answer `--help` before it is adopted.
            log::debug!("found rust-analyzer in PATH. trying to run `rust-analyzer --help`");
            let result = delegate
                .try_exec(LanguageServerBinary {
                    path: path.clone(),
                    arguments: vec!["--help".into()],
                    env: Some(env.clone()),
                })
                .await;
            if let Err(err) = result {
                log::debug!(
                    "failed to run rust-analyzer after detecting it in PATH: binary: {:?}: {}",
                    path,
                    err
                );
                return None;
            }

            Some(LanguageServerBinary {
                path,
                env: Some(env),
                arguments: vec![],
            })
        })
        .await
    }

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        pre_release: bool,
        _: &mut AsyncApp,
    ) -> Result<GitHubLspBinaryVersion> {
        let release = latest_github_release(
            "rust-lang/rust-analyzer",
            true,
            pre_release,
            delegate.http_client(),
        )
        .await?;
        let asset_name = Self::build_asset_name().await;
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == asset_name)
            .with_context(|| format!("no asset found matching `{asset_name:?}`"))?;
        Ok(GitHubLspBinaryVersion {
            name: release.tag_name,
            url: asset.browser_download_url,
            digest: asset.digest,
        })
    }

    fn fetch_server_binary(
        &self,
        version: GitHubLspBinaryVersion,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let GitHubLspBinaryVersion {
                name,
                url,
                digest: expected_digest,
            } = version;
            let destination_path = container_dir.join(format!("rust-analyzer-{name}"));
            let server_path = match Self::GITHUB_ASSET_KIND {
                AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => destination_path.clone(),
                AssetKind::Zip => destination_path.clone().join("rust-analyzer.exe"),
            };

            let binary = LanguageServerBinary {
                path: server_path.clone(),
                env: None,
                arguments: Default::default(),
            };

            let metadata_path = destination_path.with_extension("metadata");
            let metadata = GithubBinaryMetadata::read_from_file(&metadata_path)
                .await
                .ok();
            if let Some(metadata) = metadata {
                let validity_check = async || {
                    delegate
                        .try_exec(LanguageServerBinary {
                            path: server_path.clone(),
                            arguments: vec!["--version".into()],
                            env: None,
                        })
                        .await
                        .inspect_err(|err| {
                            log::warn!(
                                "Unable to run {server_path:?} asset, redownloading: {err:#}",
                            )
                        })
                };
                if let (Some(actual_digest), Some(expected_digest)) =
                    (&metadata.digest, &expected_digest)
                {
                    if actual_digest == expected_digest {
                        if validity_check().await.is_ok() {
                            return Ok(binary);
                        }
                    } else {
                        log::info!(
                            "SHA-256 mismatch for {destination_path:?} asset, downloading new asset. Expected: {expected_digest}, Got: {actual_digest}"
                        );
                    }
                } else if validity_check().await.is_ok() {
                    return Ok(binary);
                }
            }

            download_server_binary(
                &*delegate.http_client(),
                &url,
                expected_digest.as_deref(),
                &destination_path,
                Self::GITHUB_ASSET_KIND,
            )
            .await?;
            make_file_executable(&server_path).await?;
            remove_matching(&container_dir, |path| path != destination_path).await;
            GithubBinaryMetadata::write_to_file(
                &GithubBinaryMetadata {
                    metadata_version: 1,
                    digest: expected_digest,
                },
                &metadata_path,
            )
            .await?;

            Ok(LanguageServerBinary {
                path: server_path,
                env: None,
                arguments: Default::default(),
            })
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        get_cached_server_binary(container_dir).await
    }
}

async fn get_cached_server_binary(container_dir: PathBuf) -> Option<LanguageServerBinary> {
    let mut entries = match smol::fs::read_dir(&container_dir).await {
        Ok(entries) => entries,
        Err(error) => {
            log::info!("No cached rust-analyzer binary found: {error:#}");
            return None;
        }
    };

    let mut last = None;
    while let Some(entry) = entries.next().await {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "metadata") {
            continue;
        }
        last = Some(path);
    }

    let Some(path) = last else {
        log::info!("No cached rust-analyzer binary found");
        return None;
    };
    let path = match RustLspAdapter::GITHUB_ASSET_KIND {
        AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => path,
        AssetKind::Zip => path.join("rust-analyzer.exe"),
    };

    Some(LanguageServerBinary {
        path,
        env: None,
        arguments: Vec::new(),
    })
}
