//! Contract conformance for the mock agent: a fake actus WebSocket server
//! drives the real `serve` loop and verifies the event sequences actus
//! depends on.

use futures_util::{SinkExt, StreamExt};
use telos_protocol::AgentEvent;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

async fn next_event<S>(ws: &mut S) -> AgentEvent
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    loop {
        let msg = ws.next().await.expect("stream ends").expect("ws error");
        if let Message::Text(text) = msg {
            return serde_json::from_str(&text).expect("event parses");
        }
    }
}

/// Spawn the mock against a fake actus server and return the actus-side
/// WebSocket plus the agent task.
async fn spawn_pair() -> (tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let agent_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = accept_async(stream).await.unwrap();
        telos::serve(ws).await.unwrap();
    });

    let (ws, _) = connect_async(format!("ws://{addr}")).await.unwrap();
    (ws, agent_task)
}

#[tokio::test]
async fn mock_announces_readiness_on_connect() {
    let (mut ws, task) = spawn_pair().await;

    let ev = next_event(&mut ws).await;
    match ev {
        AgentEvent::AgentReady { agent_name, .. } => assert_eq!(agent_name, "telos"),
        other => panic!("expected agent_ready, got {other:?}"),
    }

    task.abort();
}

#[tokio::test]
async fn mock_serves_new_thread_chat_flow() {
    let (mut ws, task) = spawn_pair().await;

    // Read readiness.
    let _ready = next_event(&mut ws).await;

    // New thread: acp_thread_id null.
    ws.send(Message::Text(
        r#"{"type":"chat_message","data":{"message":"hello","request_id":"req-1","acp_thread_id":null}}"#
            .into(),
    ))
    .await
    .unwrap();

    // Expect thread_created echoing the request id.
    let ev = next_event(&mut ws).await;
    let tid = match ev {
        AgentEvent::ThreadCreated {
            acp_thread_id,
            request_id,
        } => {
            assert_eq!(request_id, "req-1");
            acp_thread_id
        }
        other => panic!("expected thread_created, got {other:?}"),
    };

    // Then a stream of message_added and exactly one message_completed.
    // The canned backend (no API key in tests) produces text entries only;
    // tool_call entries appear once a real LLM is configured.
    let mut saw_text = false;
    let mut completed = false;
    for _ in 0..20 {
        let ev = next_event(&mut ws).await;
        match ev {
            AgentEvent::MessageAdded {
                acp_thread_id,
                entry_type,
                ..
            } => {
                assert_eq!(acp_thread_id, tid);
                match entry_type.as_str() {
                    "text" => saw_text = true,
                    "tool_call" => {}
                    other => panic!("unexpected entry_type {other}"),
                }
            }
            AgentEvent::MessageCompleted {
                acp_thread_id,
                request_id,
                ..
            } => {
                assert_eq!(acp_thread_id, tid);
                assert_eq!(request_id, "req-1");
                completed = true;
                break;
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert!(saw_text, "expected text entries");
    assert!(completed, "expected message_completed");

    task.abort();
}

#[tokio::test]
async fn mock_reuses_existing_thread() {
    let (mut ws, task) = spawn_pair().await;
    let _ready = next_event(&mut ws).await;

    ws.send(Message::Text(
        r#"{"type":"chat_message","data":{"message":"follow up","request_id":"req-2","acp_thread_id":"acp-9"}}"#
            .into(),
    ))
    .await
    .unwrap();

    // No thread_created for an existing thread; the first event must be a
    // message_added on the given thread.
    let ev = next_event(&mut ws).await;
    match ev {
        AgentEvent::MessageAdded { acp_thread_id, .. } => assert_eq!(acp_thread_id, "acp-9"),
        other => panic!("expected message_added, got {other:?}"),
    }

    task.abort();
}

#[tokio::test]
async fn mock_answers_cancel_with_turn_cancelled() {
    let (mut ws, task) = spawn_pair().await;
    let _ready = next_event(&mut ws).await;

    ws.send(Message::Text(
        r#"{"type":"chat_message","data":{"message":"hello","request_id":"req-3","acp_thread_id":"acp-9"}}"#
            .into(),
    ))
    .await
    .unwrap();

    // Consume the stream (sequential mock finishes before reading the cancel).
    let mut completed = false;
    for _ in 0..20 {
        match next_event(&mut ws).await {
            AgentEvent::MessageCompleted { .. } => {
                completed = true;
                break;
            }
            AgentEvent::MessageAdded { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(completed);

    ws.send(Message::Text(
        r#"{"type":"cancel_current_turn","data":{}}"#.into(),
    ))
    .await
    .unwrap();
    let ev = next_event(&mut ws).await;
    match ev {
        AgentEvent::TurnCancelled { status, .. } => assert_eq!(status, "cancelled"),
        other => panic!("expected turn_cancelled, got {other:?}"),
    }

    task.abort();
}
