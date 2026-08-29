//! Headless agent: connects to actus over WebSocket and serves the contract.
//!
//! Chat messages run through the telos-core agent loop. When an API key is
//! configured the loop calls the LLM and executes tools; otherwise a canned
//! backend keeps the flow alive. Every observable step is streamed to actus
//! as contract events.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use telos_core::agent::{Agent, Step};
use telos_core::llm::{CannedBackend, ChatBackend, OpenAiBackend};
use telos_core::tools::ToolRegistry;
use telos_protocol::{AgentCommand, AgentEvent};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn wire(ev: AgentEvent) -> Message {
    Message::Text(serde_json::to_string(&ev).expect("event serializes").into())
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}

/// Connect to actus and serve the contract, reconnecting on drops with
/// capped exponential backoff.
pub async fn run_mock_agent(url: &str) -> Result<()> {
    let mut delay = 1u64;
    loop {
        match connect_and_serve(url).await {
            Ok(()) => tracing::info!("connection closed by actus; reconnecting"),
            Err(e) => tracing::warn!("connection error: {e}; reconnecting"),
        }
        tokio::time::sleep(Duration::from_secs(delay)).await;
        delay = (delay * 2).min(30);
    }
}

async fn connect_and_serve(url: &str) -> Result<()> {
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .context("websocket connect")?;
    serve(ws).await
}

/// Serve one WebSocket connection: announce readiness, then handle commands
/// until the peer closes the connection.
pub async fn serve<S>(ws: WebSocketStream<S>) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();

    write
        .send(wire(AgentEvent::AgentReady {
            agent_name: "telos".into(),
            thread_id: None,
        }))
        .await?;

    let mut current_request: Option<String> = None;

    while let Some(msg) = read.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                let cmd: AgentCommand = match serde_json::from_str(&text) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        tracing::warn!("ignoring malformed command: {e}");
                        continue;
                    }
                };
                match cmd {
                    AgentCommand::ChatMessage {
                        message,
                        request_id,
                        acp_thread_id,
                    } => {
                        current_request = Some(request_id.clone());
                        let tid = match acp_thread_id {
                            Some(tid) => tid,
                            None => {
                                let tid = new_id("acp");
                                write
                                    .send(wire(AgentEvent::ThreadCreated {
                                        acp_thread_id: tid.clone(),
                                        request_id: request_id.clone(),
                                    }))
                                    .await?;
                                tid
                            }
                        };
                        run_turn_and_stream(&mut write, &tid, &request_id, &message).await?;
                        // Keep the last request id so a late cancel still
                        // gets answered; actus drops duplicates via its
                        // consumed-request sentinel.
                    }
                    AgentCommand::CancelCurrentTurn {} => {
                        if let Some(rid) = current_request.take() {
                            write
                                .send(wire(AgentEvent::TurnCancelled {
                                    request_id: rid,
                                    status: "cancelled".into(),
                                }))
                                .await?;
                        }
                    }
                    AgentCommand::ResolveToolCallAuthorization { .. } => {
                        tracing::debug!("tool authorization resolved; no pending ask");
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

/// Run one agent turn and stream its steps as contract events, ending with
/// `message_completed`.
async fn run_turn_and_stream<S>(
    write: &mut futures_util::stream::SplitSink<WebSocketStream<S>, Message>,
    tid: &str,
    request_id: &str,
    message: &str,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let backend: Box<dyn ChatBackend> = match OpenAiBackend::from_env() {
        Some(b) => Box::new(b),
        None => Box::new(CannedBackend),
    };
    let agent = Agent::new(backend, ToolRegistry::default_tools());
    let steps = agent.run_turn(message).await?;

    let mut last_id: Option<String> = None;
    for step in steps {
        match step {
            Step::Thinking(content) => {
                let id = new_id("m");
                last_id = Some(id.clone());
                write
                    .send(wire(AgentEvent::MessageAdded {
                        acp_thread_id: tid.into(),
                        message_id: id,
                        role: "assistant".into(),
                        content,
                        request_id: request_id.into(),
                        entry_type: "text".into(),
                        tool_name: String::new(),
                        tool_status: String::new(),
                        timestamp: now_ts(),
                    }))
                    .await?;
            }
            Step::ToolCall { name, status } => {
                let id = new_id("t");
                last_id = Some(id.clone());
                write
                    .send(wire(AgentEvent::MessageAdded {
                        acp_thread_id: tid.into(),
                        message_id: id,
                        role: "assistant".into(),
                        content: String::new(),
                        request_id: request_id.into(),
                        entry_type: "tool_call".into(),
                        tool_name: name,
                        tool_status: status,
                        timestamp: now_ts(),
                    }))
                    .await?;
            }
            Step::Answer(content) => {
                let id = new_id("m");
                last_id = Some(id.clone());
                write
                    .send(wire(AgentEvent::MessageAdded {
                        acp_thread_id: tid.into(),
                        message_id: id,
                        role: "assistant".into(),
                        content,
                        request_id: request_id.into(),
                        entry_type: "text".into(),
                        tool_name: String::new(),
                        tool_status: String::new(),
                        timestamp: now_ts(),
                    }))
                    .await?;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    write
        .send(wire(AgentEvent::MessageCompleted {
            acp_thread_id: tid.into(),
            message_id: last_id.unwrap_or_else(|| new_id("m")),
            request_id: request_id.into(),
        }))
        .await?;
    Ok(())
}
