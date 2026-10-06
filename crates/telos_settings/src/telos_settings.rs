//! The settings a launched telos reads at startup, in the shape this program expects.
//!
//! The names, the nesting, and which entries a headless run needs are properties of this
//! program, because this is the file it opens. Whoever launches telos writes it, so the
//! format is published here for a deployment to call, instead of every deployment
//! retyping it and every deployment getting a detail of it wrong.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::anyhow;

/// Whether a tool call needs approval before it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolApproval {
    Always,
    Ask,
    Never,
}

impl ToolApproval {
    /// The value a settings file carries.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolApproval::Always => "always",
            ToolApproval::Ask => "ask",
            ToolApproval::Never => "never",
        }
    }
}

/// One context server (an MCP server) to write into the settings.
///
/// `env` and `headers` arrive resolved. A value that names an environment variable is the
/// caller's to resolve, because the caller is the process that holds that environment.
#[derive(Clone, Debug, Default)]
pub struct ContextServer {
    pub name: String,
    /// Whether the server starts with the agent. One that is off is still written: it
    /// stays in the agent's catalog, which is where the agent turns it on itself.
    pub enabled: bool,
    /// Stdio transport: the program to run.
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// HTTP transport: the endpoint to reach.
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    /// Tool call timeout in seconds (stdio only).
    pub timeout: Option<u64>,
}

/// The values one launch resolved, in the shape this program reads them.
#[derive(Clone, Debug)]
pub struct LaunchSettings {
    /// The provider key. A launch needs one, because an agent without it has no provider.
    pub api_key: Option<String>,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub model_display: String,
    /// One of `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`.
    pub reasoning_effort: String,
    pub tool_approval: ToolApproval,
    pub context_servers: Vec<ContextServer>,
}

/// Write the settings and the credentials this program reads from `data_dir`, the directory
/// it was launched with.
///
/// A settings file that is already there keeps everything a launch has no opinion on, so a
/// file an operator edited by hand stays theirs everywhere a launch does not speak.
pub fn write(data_dir: &Path, launch: &LaunchSettings) -> anyhow::Result<()> {
    let api_key = launch
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow!("no LLM API key: an agent has no provider without one"))?;
    let provider = launch.provider.as_str();
    let base_url = launch.base_url.as_str();
    let model_name = launch.model.as_str();
    let model_display = launch.model_display.as_str();
    let reasoning_effort = launch.reasoning_effort.as_str();
    let context_servers = &launch.context_servers;

    let settings_dir = data_dir.join("config");
    fs::create_dir_all(&settings_dir)?;
    let settings_file = settings_dir.join("settings.json");

    let mut settings: serde_json::Value = if settings_file.exists() {
        serde_json::from_str(&fs::read_to_string(&settings_file)?)?
    } else {
        serde_json::json!({})
    };

    // The OpenAI-compatible endpoint is written only when the caller supplied a base URL
    // and a model name. Without them there is no provider entry: an empty api_url or model
    // breaks the settings parse, and this crate does not invent endpoint values.
    let endpoint_configured = !base_url.trim().is_empty() && !model_name.trim().is_empty();
    if endpoint_configured {
        if settings
            .get("language_models")
            .and_then(|language_models| language_models.get("openai_compatible"))
            .and_then(|openai_compatible| openai_compatible.get(provider))
            .is_none()
        {
            let mut model = serde_json::json!({
                "name": model_name,
                "display_name": model_display,
                "max_tokens": 65536,
                "max_output_tokens": 8192,
                "tool_use": true,
            });
            // The OpenAI-compatible provider decides whether a model can think from this
            // field, and sends the level with every request. A level of `none` is written
            // as no field at all: that is the one shape that asks for no reasoning
            // parameter, which is what a model that rejects the parameter needs.
            if reasoning_effort != "none" {
                model["reasoning_effort"] = serde_json::json!(reasoning_effort);
            }
            settings["language_models"]["openai_compatible"][provider] = serde_json::json!({
                "api_url": base_url,
                "available_models": [model],
            });
        }
    } else {
        tracing::warn!(
            "LLM endpoint not configured (set LLM_BASE_URL and LLM_MODEL in the local environment or the agent's config.toml); skipping provider injection"
        );
    }

    // A thread reads its thinking state from `agent.default_model` and nowhere else, so
    // without this a launched agent thinks at the provider default while the operator
    // chose another level. Written only when nothing set it, so a file an operator edited
    // stays theirs.
    if endpoint_configured && settings["agent"]["default_model"].is_null() {
        let enable_thinking = reasoning_effort != "none";
        settings["agent"]["default_model"] = serde_json::json!({
            "provider": provider,
            "model": model_name,
            "enable_thinking": enable_thinking,
            "effort": if enable_thinking {
                serde_json::Value::String(reasoning_effort.to_string())
            } else {
                serde_json::Value::Null
            },
        });
    }

    // The terminal is the tool a headless agent runs commands with, and it is the one the
    // permission gate can refuse outright: a command carrying a shell substitution is
    // denied whenever the effective decision is not an unconditional allow, and that
    // happens before an approval could be asked for. The refusal protects a per-command
    // prompt, so a launch that starts its agents with `always` has nothing left for it to
    // protect and the tool is opened up here. Any other mode keeps the agent's own
    // setting, prompt and all.
    //
    // Only the terminal default is written, and only when nothing set it, so a rule an
    // operator wrote by hand stays theirs.
    if launch.tool_approval == ToolApproval::Always {
        let permissions = &mut settings["agent"]["tool_permissions"];
        if permissions["tools"]["terminal"]["default"].is_null() {
            permissions["tools"]["terminal"]["default"] = serde_json::json!("allow");
        }
    }

    // Each context server becomes its `context_servers` entry: stdio as
    // `{ command, args, env, timeout }`, HTTP as `{ url, headers }`. A declared entry is
    // merged over whatever the file holds, so a server an operator added by hand survives.
    for server in context_servers {
        let mut entry = serde_json::Map::new();
        if let Some(url) = &server.url {
            entry.insert("url".to_string(), serde_json::json!(url));
            if !server.headers.is_empty() {
                entry.insert("headers".to_string(), serde_json::json!(server.headers));
            }
        } else {
            if let Some(command) = &server.command {
                entry.insert("command".to_string(), serde_json::json!(command));
            }
            if !server.args.is_empty() {
                entry.insert("args".to_string(), serde_json::json!(server.args));
            }
            if !server.env.is_empty() {
                entry.insert("env".to_string(), serde_json::json!(server.env));
            }
            if let Some(timeout) = server.timeout {
                entry.insert("timeout".to_string(), serde_json::json!(timeout));
            }
        }
        entry.insert("enabled".to_string(), serde_json::json!(server.enabled));
        settings["context_servers"][server.name.as_str()] = serde_json::Value::Object(entry);
    }
    if !context_servers.is_empty() {
        let enabled = context_servers
            .iter()
            .filter(|server| server.enabled)
            .count();
        tracing::info!(
            "Wrote {} MCP server(s) to settings, {} of them enabled",
            context_servers.len(),
            enabled
        );
    }

    let mut file = fs::File::create(&settings_file)?;
    file.write_all(serde_json::to_string_pretty(&settings)?.as_bytes())?;

    let credentials_dir = data_dir.join("credentials");
    fs::create_dir_all(&credentials_dir)?;
    let credentials_file = credentials_dir.join("credentials.json");

    let mut credentials = serde_json::Map::new();
    let mut provider_credentials = serde_json::Map::new();
    provider_credentials.insert(
        "api_key".to_string(),
        serde_json::Value::String(api_key.to_string()),
    );
    credentials.insert(
        format!("provider/{provider}"),
        serde_json::Value::Object(provider_credentials),
    );

    let mut file = fs::File::create(&credentials_file)?;
    file.write_all(
        serde_json::to_string_pretty(&serde_json::Value::Object(credentials))?.as_bytes(),
    )?;

    tracing::info!("Executor settings written to {}", settings_file.display());
    Ok(())
}
