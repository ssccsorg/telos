//! Tool-loop tests with a scripted backend, so no network or API key is
//! involved.

use std::sync::Arc;

use telos_core::agent::{Agent, Step};
use telos_core::llm::{ChatBackend, ChatResponse, Message, ToolCall};
use telos_core::tools::ToolRegistry;
use tokio::sync::Mutex;

/// Backend that replays a fixed script of responses.
struct ScriptedBackend {
    responses: Arc<Mutex<Vec<ChatResponse>>>,
}

#[async_trait::async_trait]
impl ChatBackend for ScriptedBackend {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> anyhow::Result<ChatResponse> {
        let mut guard = self.responses.lock().await;
        if guard.is_empty() {
            anyhow::bail!("script exhausted");
        }
        Ok(guard.remove(0))
    }
}

fn tool_call(id: &str, name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: args,
    }
}

#[tokio::test]
async fn tool_loop_executes_and_answers() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out.txt");

    let responses = Arc::new(Mutex::new(vec![
        // Round one: request a write_file tool call.
        ChatResponse {
            content: Some("I will write the file.".into()),
            tool_calls: vec![tool_call(
                "call-1",
                "write_file",
                serde_json::json!({
                    "path": target.to_string_lossy(),
                    "content": "hello telos\n",
                }),
            )],
        },
        // Round two: no tools, final answer.
        ChatResponse {
            content: Some("File written.".into()),
            tool_calls: vec![],
        },
    ]));

    let agent = Agent::new(
        Box::new(ScriptedBackend { responses }),
        ToolRegistry::default_tools(),
    );
    let steps = agent.run_turn("write a file").await.unwrap();

    // The tool actually ran.
    let content = std::fs::read_to_string(&target).unwrap();
    assert_eq!(content, "hello telos\n");

    // The step stream shows the expected shape.
    assert!(steps.iter().any(|s| matches!(s, Step::Thinking(t) if t.contains("write the file"))));
    assert!(steps.iter().any(|s| matches!(
        s,
        Step::ToolCall { name, status } if name == "write_file" && status == "completed"
    )));
    assert!(steps.iter().any(|s| matches!(s, Step::Answer(a) if a == "File written.")));
}

#[tokio::test]
async fn direct_answer_skips_tools() {
    let responses = Arc::new(Mutex::new(vec![ChatResponse {
        content: Some("no tools needed".into()),
        tool_calls: vec![],
    }]));
    let agent = Agent::new(
        Box::new(ScriptedBackend { responses }),
        ToolRegistry::default_tools(),
    );
    let steps = agent.run_turn("hello").await.unwrap();
    assert_eq!(
        steps,
        vec![
            Step::Thinking("no tools needed".into()),
            Step::Answer("no tools needed".into())
        ]
    );
}
