//! Agent-side wire types for the actus contract.
//!
//! Actus sends commands as `{"type": "...", "data": {...}}` and the agent
//! emits events as `{"event_type": "...", "data": {...}}`. These types pin
//! the subset actus exercises, as recorded in the actus integration contract
//! document. The field names are the wire contract and are shared with actus
//! by design; everything else in this crate is independent expression.

use serde::{Deserialize, Serialize};

/// Event emitted by the agent to actus. Adjacently tagged so the JSON shape
/// is exactly `{"event_type": "...", "data": {...}}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "data")]
pub enum AgentEvent {
    /// Sent right after the WebSocket connects so actus marks the agent
    /// ready. Submits are rejected until this arrives.
    #[serde(rename = "agent_ready")]
    AgentReady {
        agent_name: String,
        #[serde(default)]
        thread_id: Option<String>,
    },
    /// Sent in response to a `chat_message` whose `acp_thread_id` was null.
    /// Actus maps the request id to a local thread from this event.
    #[serde(rename = "thread_created")]
    ThreadCreated {
        acp_thread_id: String,
        request_id: String,
    },
    #[serde(rename = "thread_title_changed")]
    ThreadTitleChanged {
        acp_thread_id: String,
        title: String,
    },
    /// Streaming content entry. Content is cumulative for the same
    /// `message_id`; a repeated id replaces the entry in place.
    #[serde(rename = "message_added")]
    MessageAdded {
        acp_thread_id: String,
        message_id: String,
        role: String,
        content: String,
        #[serde(default)]
        request_id: String,
        #[serde(default)]
        entry_type: String,
        #[serde(default)]
        tool_name: String,
        #[serde(default)]
        tool_status: String,
        timestamp: i64,
    },
    /// Terminal event for a turn. Exactly one terminal event per request id.
    #[serde(rename = "message_completed")]
    MessageCompleted {
        acp_thread_id: String,
        message_id: String,
        request_id: String,
    },
    #[serde(rename = "chat_response_error")]
    ChatResponseError {
        request_id: String,
        error: String,
    },
    #[serde(rename = "turn_cancelled")]
    TurnCancelled {
        request_id: String,
        status: String,
    },
    /// The agent asks a human to approve or reject a tool call.
    #[serde(rename = "tool_call_authorization_requested")]
    ToolCallAuthorizationRequested {
        acp_thread_id: String,
        tool_call_id: String,
        tool_name: String,
    },
}

/// Command received from actus. Adjacently tagged as
/// `{"type": "...", "data": {...}}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AgentCommand {
    /// Submit a user message. A null `acp_thread_id` creates a new thread.
    #[serde(rename = "chat_message")]
    ChatMessage {
        message: String,
        request_id: String,
        #[serde(default)]
        acp_thread_id: Option<String>,
    },
    #[serde(rename = "cancel_current_turn")]
    CancelCurrentTurn {},
    #[serde(rename = "resolve_tool_call_authorization")]
    ResolveToolCallAuthorization {
        acp_thread_id: String,
        tool_call_id: String,
        allow: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_wire_format_roundtrip() {
        let ev = AgentEvent::MessageAdded {
            acp_thread_id: "acp-1".into(),
            message_id: "m-1".into(),
            role: "assistant".into(),
            content: "hello".into(),
            request_id: "req-1".into(),
            entry_type: "text".into(),
            tool_name: String::new(),
            tool_status: String::new(),
            timestamp: 1_700_000_000,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event_type"], "message_added");
        assert_eq!(value["data"]["request_id"], "req-1");
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ev);
    }

    #[test]
    fn command_wire_format_roundtrip() {
        let raw = r#"{"type":"chat_message","data":{"message":"hi","request_id":"req-2","acp_thread_id":null}}"#;
        let cmd: AgentCommand = serde_json::from_str(raw).unwrap();
        match cmd {
            AgentCommand::ChatMessage { request_id, acp_thread_id, .. } => {
                assert_eq!(request_id, "req-2");
                assert_eq!(acp_thread_id, None);
            }
            other => panic!("expected chat_message, got {other:?}"),
        }
        let json = serde_json::to_string(&AgentCommand::CancelCurrentTurn {}).unwrap();
        assert_eq!(json, r#"{"type":"cancel_current_turn","data":{}}"#);
    }
}
