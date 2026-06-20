//! Vibird host bridge: the WebSocket server that devices dial into, plus (later) ASR and the Claude Code
//! integration. This is the reusable product core.
//!
//! v0.1 skeleton: the transport + protocol handshake + audio framing are real; ASR and agent injection
//! are marked `TODO` and land in v0.1/v0.2.

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use vibird_protocol::{AgentState, Downlink, Uplink, PROTOCOL_VERSION};

/// Default WebSocket port for the bridge.
pub const DEFAULT_PORT: u16 = 8137;

/// Run the bridge: accept device WebSocket connections on `port` until cancelled.
pub async fn serve(port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    info!("vibird bridge listening on ws://{addr}");
    // TODO(v0.1): advertise `_vibird._tcp` over mDNS so devices auto-discover us.
    loop {
        let (stream, peer) = listener.accept().await?;
        info!("device connecting from {peer}");
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream).await {
                warn!("connection {peer} ended: {e}");
            }
        });
    }
}

async fn handle_conn(stream: TcpStream) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut tx, mut rx) = ws.split();

    // Raw PCM accumulated between AudioStart and AudioEnd (push-to-talk).
    let mut pcm: Vec<u8> = Vec::new();
    let mut capturing = false;

    while let Some(msg) = rx.next().await {
        match msg? {
            Message::Text(txt) => {
                let up: Uplink = match serde_json::from_str(txt.as_str()) {
                    Ok(u) => u,
                    Err(e) => {
                        warn!("ignoring bad uplink json: {e}: {txt}");
                        continue;
                    }
                };
                match up {
                    Uplink::Hello { device_id, fw_version, protocol, caps } => {
                        info!("hello from {device_id} (fw {fw_version}, proto {protocol}, caps {caps:?})");
                        let welcome = Downlink::Welcome {
                            protocol: PROTOCOL_VERSION,
                            bridge_version: env!("CARGO_PKG_VERSION").to_string(),
                        };
                        tx.send(Message::text(serde_json::to_string(&welcome)?)).await?;
                        // Greet with the idle face.
                        let idle = Downlink::SetState(AgentState::Idle);
                        tx.send(Message::text(serde_json::to_string(&idle)?)).await?;
                    }
                    Uplink::AudioStart { sample_rate, format } => {
                        debug!("audio start: {sample_rate} Hz {format:?}");
                        pcm.clear();
                        capturing = true;
                    }
                    Uplink::AudioEnd => {
                        capturing = false;
                        info!("audio end: {} bytes captured", pcm.len());
                        // TODO(v0.1): run ASR over `pcm`, then inject the transcript into Claude Code.
                    }
                    Uplink::Button { event } => info!("button: {event:?}"),
                    Uplink::Approval { request_id, decision } => {
                        info!("approval {request_id} -> {decision:?}");
                        // TODO(v0.2): resolve the pending PreToolUse hook with this decision.
                    }
                    Uplink::Gesture { kind } => debug!("gesture: {kind:?}"),
                    Uplink::Pong { nonce } => debug!("pong {nonce}"),
                }
            }
            Message::Binary(buf) => {
                if capturing {
                    pcm.extend_from_slice(&buf);
                } else {
                    debug!("unexpected binary frame ({} bytes) outside capture", buf.len());
                }
            }
            Message::Ping(p) => tx.send(Message::Pong(p)).await?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}
