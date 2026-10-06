// The settings this program reads, pinned by the decisions a launch makes: which entries
// it writes, and which it leaves alone. These are the decisions actus used to make for
// every deployment; they belong to the program that reads the file.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::json;
use telos_settings::{ContextServer, LaunchSettings, ToolApproval, write};

fn launch(effort: &str, approval: ToolApproval) -> LaunchSettings {
    LaunchSettings {
        api_key: Some("sk-test".to_string()),
        provider: "openai-compatible".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        model: "example-model".to_string(),
        model_display: "Example".to_string(),
        reasoning_effort: effort.to_string(),
        tool_approval: approval,
        context_servers: Vec::new(),
    }
}

fn read(dir: &Path, name: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join(name)).expect("the file is written"))
        .expect("the file is JSON")
}

/// Writes into a fresh directory and returns both files, which is what a launch produces.
fn written(settings: &LaunchSettings) -> (serde_json::Value, serde_json::Value) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    write(dir.path(), settings).expect("the settings are written");
    let dir = dir.path();
    (
        read(dir, "config/settings.json"),
        read(dir, "credentials/credentials.json"),
    )
}

/// A level the caller declares reaches both places the agent reads it: the model entry,
/// which is where the OpenAI-compatible provider decides a model can think at all, and
/// `agent.default_model`, which is the only thing a new thread reads its thinking from.
#[test]
fn a_declared_effort_reaches_the_model_entry_and_the_default_model() {
    for effort in ["minimal", "low", "medium", "high", "xhigh", "max"] {
        let (settings, _) = written(&launch(effort, ToolApproval::Always));

        let provider = &settings["language_models"]["openai_compatible"]["openai-compatible"];
        let model = &provider["available_models"][0];
        assert_eq!(model["name"], "example-model");
        assert_eq!(model["reasoning_effort"], effort);
        assert_eq!(
            settings["agent"]["default_model"]["provider"],
            "openai-compatible"
        );
        assert_eq!(settings["agent"]["default_model"]["model"], "example-model");
        assert_eq!(settings["agent"]["default_model"]["effort"], effort);
        assert_eq!(
            settings["agent"]["default_model"]["enable_thinking"],
            json!(true)
        );
    }
}

/// `none` is the one shape that asks for no reasoning parameter, which is what a model
/// that rejects the parameter needs, so the field is absent rather than empty.
#[test]
fn no_effort_asks_for_no_reasoning_parameter() {
    let (settings, _) = written(&launch("none", ToolApproval::Always));

    let provider = &settings["language_models"]["openai_compatible"]["openai-compatible"];
    let model = &provider["available_models"][0];
    assert!(model.get("reasoning_effort").is_none(), "model: {model}");
    assert_eq!(
        settings["agent"]["default_model"]["enable_thinking"],
        json!(false)
    );
    assert!(settings["agent"]["default_model"]["effort"].is_null());
}

/// The terminal default is opened up only for a launch that starts its agents with
/// `always`, because that is the mode where the refusal it performs has nothing left to
/// protect. Any other mode keeps the agent's own setting.
#[test]
fn only_an_always_approval_opens_the_terminal() {
    let (always, _) = written(&launch("high", ToolApproval::Always));
    assert_eq!(
        always["agent"]["tool_permissions"]["tools"]["terminal"]["default"],
        "allow"
    );

    let (ask, _) = written(&launch("high", ToolApproval::Ask));
    assert!(
        ask["agent"]["tool_permissions"]["tools"]["terminal"].is_null(),
        "ask: {ask}"
    );
}

/// A context server is written with its transport, and one that is off stays in the file:
/// it is in the agent's catalog, which is where the agent turns it on itself.
#[test]
fn a_context_server_is_written_with_its_transport_and_stays_when_it_is_off() {
    let mut settings = launch("high", ToolApproval::Always);
    let mut env = BTreeMap::new();
    env.insert("TOKEN".to_string(), "resolved".to_string());
    settings.context_servers = vec![
        ContextServer {
            name: "memory".to_string(),
            enabled: true,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "server-memory".to_string()],
            env,
            timeout: Some(30),
            ..ContextServer::default()
        },
        ContextServer {
            name: "remote".to_string(),
            enabled: false,
            url: Some("https://mcp.example.com/mcp".to_string()),
            ..ContextServer::default()
        },
    ];

    let (settings, _) = written(&settings);

    let memory = &settings["context_servers"]["memory"];
    assert_eq!(memory["command"], "npx");
    assert_eq!(memory["args"], json!(["-y", "server-memory"]));
    assert_eq!(memory["env"]["TOKEN"], "resolved");
    assert_eq!(memory["timeout"], json!(30));
    assert_eq!(memory["enabled"], json!(true));

    let remote = &settings["context_servers"]["remote"];
    assert_eq!(remote["url"], "https://mcp.example.com/mcp");
    assert_eq!(remote["enabled"], json!(false));
}

/// A file an operator edited keeps everything a launch has no opinion on, and keeps every
/// entry a launch declines to overwrite.
#[test]
fn an_operator_edit_survives_everywhere_a_launch_has_no_opinion() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = dir.path().join("config");
    fs::create_dir_all(&config).expect("the config directory");
    fs::write(
        config.join("settings.json"),
        r#"{"theme":"One Dark","agent":{"default_model":{"provider":"hand","model":"hand"}}}"#,
    )
    .expect("an existing settings file");

    write(dir.path(), &launch("high", ToolApproval::Always)).expect("the settings are written");

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(config.join("settings.json")).expect("the file is there"),
    )
    .expect("the file is JSON");
    assert_eq!(settings["theme"], "One Dark");
    assert_eq!(settings["agent"]["default_model"]["provider"], "hand");
}

#[test]
fn the_credentials_carry_the_key_the_agent_reads() {
    let (_, credentials) = written(&launch("high", ToolApproval::Always));
    assert_eq!(
        credentials["provider/openai-compatible"]["api_key"],
        "sk-test"
    );
}

/// A launch with no key is refused by name, because an agent without one has no provider
/// and every turn would fail somewhere the reason is not visible.
#[test]
fn a_launch_without_a_key_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut settings = launch("high", ToolApproval::Always);
    settings.api_key = None;

    let error = write(dir.path(), &settings)
        .expect_err("a launch with no key is refused")
        .to_string();
    assert!(error.contains("API key"), "{error}");
}
