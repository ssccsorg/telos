//! Telos headless binary.
//!
//! Actus launches this binary with the same launch contract it used for the
//! previous headless binary:
//!
//!   telos --headless --allow-multiple-instances --user-data-dir <dir> <workdir>
//!
//! with the environment contract:
//!
//!   ZED_HELIX_URL   actus WebSocket host:port (required)
//!   HELIX_SESSION_ID  session id assigned by actus
//!   ZED_WORK_DIR      workdir (also passed positionally)
//!   ZED_TOOL_APPROVAL tool approval mode
//!
//! Only the WebSocket address is functionally required; the remaining args
//! and variables are tolerated so actus needs no change.

use anyhow::Result;

fn main() -> Result<()> {
    // The launch args are contract compatibility, not configuration.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "telos: headless agent for actus\n\
             usage: telos [--headless] [--allow-multiple-instances] [--user-data-dir <dir>] [<workdir>]\n\
             env: ZED_HELIX_URL (required), HELIX_SESSION_ID, ZED_WORK_DIR, ZED_TOOL_APPROVAL"
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "telos=info".into()),
        )
        .init();

    let host = std::env::var("ZED_HELIX_URL").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let ws_url = if host.starts_with("ws://") {
        host
    } else {
        format!("ws://{host}")
    };

    tracing::info!("telos mock connecting to {ws_url}");
    tokio::runtime::Runtime::new()?.block_on(telos::run_mock_agent(&ws_url))
}
