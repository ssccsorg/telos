//! Enables or disables one of the MCP servers the operator provisioned for this
//! agent, or lists what the agent can enable.
//!
//! The catalog is the operator's: the servers in the agent's settings, whether
//! they start on or off. This tool flips the `enabled` flag of an entry that is
//! already there. It never adds an entry, so an agent can turn on what the
//! operator provisioned and nothing else, and no server definition reaches the
//! settings file from a model.
//!
//! Flipping the flag writes the agent's settings file. The `ContextServerStore`
//! observes the settings store, so the server starts (or stops) in the same
//! process, and the tool list the model receives is rebuilt at each iteration of
//! the agent loop from the live registry. A server that has just been turned on
//! therefore reaches the model after it has finished starting, which in practice
//! means the turn after the one that turned it on.

use agent_client_protocol::schema::v1 as acp;
use gpui::{App, Entity, SharedString, Task};
use project::{Project, project_settings::ProjectSettings};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::Settings as _;
use std::sync::Arc;
use util::markdown::MarkdownInlineCode;

use crate::{AgentTool, ToolCallEventStream, ToolInput};

/// Turns an MCP server on or off for this agent, or lists the servers it can
/// turn on.
///
/// MCP (Model Context Protocol) servers add tools to this agent. Which servers
/// exist is fixed by this agent's configuration; you choose which of them are
/// running. Call this tool with no `name` to see what is available and what is
/// already running, then call it again with a `name` to change one.
///
/// A server that is turned on starts immediately, and its tools reach you after
/// it has finished starting, which is not before the turn after this one. Its
/// tools arrive with the server's own names, not prefixed.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EnableContextServerToolInput {
    /// The name of the server to change, exactly as the listing gives it. Omit
    /// to list the servers and change nothing.
    #[serde(default)]
    pub name: Option<String>,

    /// Whether to turn the server on. Omit to turn it on.
    #[serde(default)]
    pub enabled: Option<bool>,
}

pub struct EnableContextServerTool {
    project: Entity<Project>,
}

impl EnableContextServerTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { project }
    }
}

impl AgentTool for EnableContextServerTool {
    type Input = EnableContextServerToolInput;
    type Output = String;

    const NAME: &'static str = "enable_context_server";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        match input {
            Ok(EnableContextServerToolInput {
                name: Some(name),
                enabled,
            }) => {
                let verb = if enabled.unwrap_or(true) {
                    "Turn on"
                } else {
                    "Turn off"
                };
                format!("{verb} MCP server {}", MarkdownInlineCode(&name)).into()
            }
            _ => "MCP servers".into(),
        }
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        let fs = self.project.read(cx).fs().clone();
        cx.spawn(async move |cx| {
            let input = input.recv().await.map_err(|e| e.to_string())?;
            let catalog = cx.update(|cx| catalog(cx));

            let Some(name) = input.name else {
                return Ok(render_catalog(&catalog));
            };

            let Some(entry) = catalog.iter().find(|entry| entry.name == name) else {
                return Err(unknown_server(&name, &catalog));
            };
            let enabled = input.enabled.unwrap_or(true);
            if entry.enabled == enabled {
                return Ok(format!(
                    "{name} is already {}.",
                    if enabled { "on" } else { "off" }
                ));
            }

            let written_name = name.clone();
            let rx = cx.update(|cx| {
                settings::update_settings_file_with_completion(fs, cx, move |settings, _cx| {
                    if let Some(server) = settings
                        .project
                        .context_servers
                        .get_mut(written_name.as_str())
                    {
                        server.set_enabled(enabled);
                    }
                })
            });
            rx.await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("Could not write the settings file: {e}"))?;

            Ok(if enabled {
                format!(
                    "Turned on {name}. It is starting now; its tools reach you once it is up, \
                     which is not before the next turn."
                )
            } else {
                format!("Turned off {name}.")
            })
        })
    }
}

/// One MCP server this agent can enable, in the order the caller can present it.
struct CatalogEntry {
    name: String,
    enabled: bool,
}

/// The agent's MCP catalog: every server in its resolved settings, with the
/// state it is in.
fn catalog(cx: &App) -> Vec<CatalogEntry> {
    let mut entries: Vec<CatalogEntry> = ProjectSettings::get_global(cx)
        .context_servers
        .iter()
        .map(|(name, settings)| CatalogEntry {
            name: name.to_string(),
            enabled: settings.enabled(),
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn render_catalog(catalog: &[CatalogEntry]) -> String {
    if catalog.is_empty() {
        return "This agent has no MCP servers configured. Its tools are the built-in ones only; \
                if you need a capability that is missing, ask the operator to add a server."
            .to_string();
    }

    let mut out = String::from("MCP servers for this agent:\n");
    for entry in catalog {
        out.push_str(&format!(
            "- {}: {}\n",
            entry.name,
            if entry.enabled { "on" } else { "off" }
        ));
    }
    out.push_str("\nCall this tool with a name to turn one on or off.");
    out
}

/// The error for a name the catalog does not hold. It names what is available,
/// because a model that guessed a name needs the real ones to retry with.
fn unknown_server(name: &str, catalog: &[CatalogEntry]) -> String {
    if catalog.is_empty() {
        return format!(
            "There is no MCP server named {name}, and this agent has no servers configured."
        );
    }

    let names: Vec<&str> = catalog.iter().map(|entry| entry.name.as_str()).collect();
    format!(
        "There is no MCP server named {name}. Configured servers: {}.",
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::project_settings::ContextServerSettings;
    use settings::ContextServerCommand;

    fn stdio(enabled: bool) -> ContextServerSettings {
        ContextServerSettings::Stdio {
            enabled,
            remote: false,
            command: ContextServerCommand {
                path: "/usr/bin/true".into(),
                args: vec![],
                env: None,
                timeout: None,
            },
        }
    }

    /// Install a settings store and make the named servers this agent's catalog.
    fn install_catalog(cx: &mut gpui::TestAppContext, servers: &[(&str, bool)]) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            let mut settings = ProjectSettings::get_global(cx).clone();
            for (name, enabled) in servers {
                settings
                    .context_servers
                    .insert((*name).into(), stdio(*enabled));
            }
            ProjectSettings::override_global(settings, cx);
        });
    }

    #[gpui::test]
    fn catalog_lists_every_server_with_its_state(cx: &mut gpui::TestAppContext) {
        install_catalog(cx, &[("memory", true), ("github", false)]);

        cx.update(|cx| {
            let catalog = catalog(cx);
            assert_eq!(catalog.len(), 2, "the catalog is the servers settings hold");
            assert_eq!(catalog[0].name, "github");
            assert!(!catalog[0].enabled);
            assert_eq!(catalog[1].name, "memory");
            assert!(catalog[1].enabled);

            let listing = render_catalog(&catalog);
            assert!(listing.contains("- github: off"), "{listing}");
            assert!(listing.contains("- memory: on"), "{listing}");
        });
    }

    #[gpui::test]
    fn an_unknown_name_is_refused_with_the_names_that_exist(cx: &mut gpui::TestAppContext) {
        install_catalog(cx, &[("memory", true)]);

        cx.update(|cx| {
            let error = unknown_server("github", &catalog(cx));
            assert!(error.contains("no MCP server named github"), "{error}");
            assert!(error.contains("memory"), "{error}");
        });
    }

    #[test]
    fn an_empty_catalog_says_so() {
        assert!(render_catalog(&[]).contains("no MCP servers configured"));
        assert!(unknown_server("github", &[]).contains("no MCP server named github"));
    }
}
