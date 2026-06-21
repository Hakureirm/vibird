//! Vibird host bridge:设备拨入的 WebSocket 服务端 + ASR + Claude Code 注入 / 状态。可复用产品核心。
//!
//! - **语音闭环**(v0.1):设备长按推 PCM → [`Asr`] 转写 → [`Inject`] 注入 Claude Code(tmux)。
//! - **状态显示**(v0.2):一个本地**控制面**(TCP,`control_port`),Claude Code hook(`vibird hook`)
//!   或 MCP 把 [`Downlink`] 推进来,桥接广播给所有连着的设备,驱动表情。
//!
//! ASR 可插拔(stub / cloud)。mDNS 广播待做。

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use vibird_protocol::{AgentState, Downlink, Uplink, PROTOCOL_VERSION};

mod asr;
mod hook;
mod inject;
mod mcp;
pub use asr::{Asr, CloudConfig};
pub use hook::{hook_event_to_state, push_downlink, push_state};
pub use inject::Inject;
pub use mcp::run as run_mcp;

/// 桥接默认 WebSocket 端口(设备连这里)。
pub const DEFAULT_PORT: u16 = 8137;

/// 控制面端口 = WS 端口 + 1(hook / MCP 在本地推状态)。
pub fn control_port(ws_port: u16) -> u16 {
    ws_port.wrapping_add(1)
}

/// 桥接运行时配置。
#[derive(Clone, Default)]
pub struct Config {
    /// ASR 后端。
    pub asr: Asr,
    /// 转写注入目标。
    pub inject: Inject,
}

/// 运行桥接:WS 端口接受设备,控制面端口接受本地状态推送。
pub async fn serve(port: u16, config: Config) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    info!("vibird bridge listening on ws://{addr}");
    let config = Arc::new(config);

    // 状态广播:控制面推入 → 扇出给所有设备。
    let (state_tx, _) = broadcast::channel::<Downlink>(32);
    {
        let state_tx = state_tx.clone();
        let cport = control_port(port);
        tokio::spawn(async move {
            if let Err(e) = control_listener(cport, state_tx).await {
                warn!("control plane ended: {e}");
            }
        });
    }
    // TODO(v0.1): 通过 mDNS 广播 `_vibird._tcp` 让设备自动发现。
    loop {
        let (stream, peer) = listener.accept().await?;
        info!("device connecting from {peer}");
        let config = config.clone();
        let state_rx = state_tx.subscribe();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, config, state_rx).await {
                warn!("connection {peer} ended: {e}");
            }
        });
    }
}

/// 控制面:本地 TCP,每行一个 [`Downlink`] JSON,广播给所有设备。`vibird hook` / MCP 用它推状态。
async fn control_listener(port: u16, state_tx: broadcast::Sender<Downlink>) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    info!("vibird control plane on tcp://127.0.0.1:{port}");
    loop {
        let (stream, _) = listener.accept().await?;
        let state_tx = state_tx.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<Downlink>(&line) {
                    Ok(d) => {
                        let _ = state_tx.send(d); // 没有设备时无订阅者,忽略
                    }
                    Err(e) => warn!("bad control line: {e}: {line}"),
                }
            }
        });
    }
}

/// 把 [`Downlink`] 序列化成文本帧。
fn frame(d: &Downlink) -> Result<Message> {
    Ok(Message::text(serde_json::to_string(d)?))
}

async fn handle_conn(
    stream: TcpStream,
    config: Arc<Config>,
    mut state_rx: broadcast::Receiver<Downlink>,
) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut tx, mut rx) = ws.split();

    // 长按期间(AudioStart…AudioEnd)累积的原始 PCM。
    let mut pcm: Vec<u8> = Vec::new();
    let mut capturing = false;
    let mut sample_rate = 16_000u32;

    loop {
        tokio::select! {
            // 控制面广播的状态 → 转发给本设备
            res = state_rx.recv() => match res {
                Ok(d) => tx.send(frame(&d)?).await?,
                Err(broadcast::error::RecvError::Lagged(n)) => debug!("state lagged by {n}"),
                Err(broadcast::error::RecvError::Closed) => {}
            },
            // 设备上行
            msg = rx.next() => {
                let Some(msg) = msg else { break };
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
                                tx.send(frame(&Downlink::Welcome {
                                    protocol: PROTOCOL_VERSION,
                                    bridge_version: env!("CARGO_PKG_VERSION").to_string(),
                                })?).await?;
                                tx.send(frame(&Downlink::SetState(AgentState::Idle))?).await?;
                            }
                            Uplink::AudioStart { sample_rate: sr, format } => {
                                debug!("audio start: {sr} Hz {format:?}");
                                sample_rate = sr;
                                pcm.clear();
                                capturing = true;
                                tx.send(frame(&Downlink::SetState(AgentState::Listening))?).await?;
                            }
                            Uplink::AudioEnd => {
                                capturing = false;
                                info!("audio end: {} bytes @ {sample_rate} Hz", pcm.len());
                                tx.send(frame(&Downlink::SetState(AgentState::Thinking))?).await?;
                                let samples: Vec<i16> = pcm
                                    .chunks_exact(2)
                                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                                    .collect();
                                match config.asr.transcribe(&samples, sample_rate).await {
                                    Ok(text) if !text.is_empty() => {
                                        info!("ASR → {text:?}");
                                        if let Err(e) = config.inject.send(&text) {
                                            warn!("inject failed: {e}");
                                        }
                                        tx.send(frame(&Downlink::SetState(AgentState::Working { tool: None }))?).await?;
                                    }
                                    Ok(_) => {
                                        warn!("ASR returned empty");
                                        tx.send(frame(&Downlink::SetState(AgentState::Idle))?).await?;
                                    }
                                    Err(e) => {
                                        warn!("ASR failed: {e}");
                                        tx.send(frame(&Downlink::SetState(AgentState::Error { message: "ASR failed".into() }))?).await?;
                                    }
                                }
                            }
                            Uplink::Button { event } => info!("button: {event:?}"),
                            Uplink::Approval { request_id, decision } => {
                                info!("approval {request_id} -> {decision:?}");
                                // TODO(v0.2): 用此决定解决挂起的 PreToolUse hook。
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
        }
    }
    Ok(())
}
