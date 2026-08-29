//! Headless agent: connects to actus over WebSocket and serves the contract.
//!
//! The mock agent implements the full event and command surface of the actus
//! contract so the actus `run.sh` chat flow works end to end with this binary
//! attached. Streaming is simulated with cumulative `message_added` chunks and
//! a canned tool-call entry, which exercises the same consumer paths a real
//! agent would.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
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
                                let tid = format!("acp-{}", uuid::Uuid::new_v4());
                                write
                                    .send(wire(AgentEvent::ThreadCreated {
                                        acp_thread_id: tid.clone(),
                                        request_id: request_id.clone(),
                                    }))
                                    .await?;
                                tid
                            }
                        };
                        stream_answer(&mut write, &tid, &request_id, &message).await?;
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
                        tracing::debug!("tool authorization resolved; mock ignores");
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

/// Simulate a streaming answer: cumulative text chunks, a canned tool call,
/// a final answer, then the completion event.
async fn stream_answer<S>(
    write: &mut futures_util::stream::SplitSink<WebSocketStream<S>, Message>,
    tid: &str,
    request_id: &str,
    message: &str,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let base = format!("Telos mock: received \"{message}\".\n");
    let text_id = format!("m-{}", uuid::Uuid::new_v4());

    let mut acc = String::new();
    for chunk in [
        format!("{base}Checking the workspace...\n"),
        format!("{base}Checking the workspace...\nNothing to fix.\n"),
    ] {
        acc.push_str(&chunk);
        write
            .send(wire(AgentEvent::MessageAdded {
                acp_thread_id: tid.into(),
                message_id: text_id.clone(),
                role: "assistant".into(),
                content: acc.clone(),
                request_id: request_id.into(),
                entry_type: "text".into(),
                tool_name: String::new(),
                tool_status: String::new(),
                timestamp: now_ts(),
            }))
            .await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let tool_id = format!("t-{}", uuid::Uuid::new_v4());
    write
        .send(wire(AgentEvent::MessageAdded {
            acp_thread_id: tid.into(),
            message_id: tool_id,
            role: "assistant".into(),
            content: String::new(),
            request_id: request_id.into(),
            entry_type: "tool_call".into(),
            tool_name: "mock_tool".into(),
            tool_status: "completed".into(),
            timestamp: now_ts(),
        }))
        .await?;

    let final_id = format!("m-{}", uuid::Uuid::new_v4());
    write
        .send(wire(AgentEvent::MessageAdded {
            acp_thread_id: tid.into(),
            message_id: final_id.clone(),
            role: "assistant".into(),
            content: "Done. Telos mock is operational.\n".into(),
            request_id: request_id.into(),
            entry_type: "text".into(),
            tool_name: String::new(),
            tool_status: String::new(),
            timestamp: now_ts(),
        }))
        .await?;

    write
        .send(wire(AgentEvent::MessageCompleted {
            acp_thread_id: tid.into(),
            message_id: final_id,
            request_id: request_id.into(),
        }))
        .await?;
    Ok(())
}
