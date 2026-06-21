//! Vibird host bridge:设备拨入的 WebSocket 服务端 + ASR + Claude Code 注入。可复用的产品核心。
//!
//! v0.1 语音闭环:设备长按说话推 PCM → [`Asr`] 转写 → [`Inject`] 把文本注入正在运行的 Claude Code
//! 会话(tmux),同时把 [`AgentState`] 推回设备驱动表情。ASR 可插拔(stub / cloud)。mDNS 广播待做。

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use vibird_protocol::{AgentState, Downlink, Uplink, PROTOCOL_VERSION};

mod asr;
mod inject;
pub use asr::{Asr, CloudConfig};
pub use inject::Inject;

/// 桥接默认 WebSocket 端口。
pub const DEFAULT_PORT: u16 = 8137;

/// 桥接运行时配置。
#[derive(Clone, Default)]
pub struct Config {
    /// ASR 后端。
    pub asr: Asr,
    /// 转写注入目标。
    pub inject: Inject,
}

/// 运行桥接:在 `port` 上接受设备 WebSocket 连接,直到被取消。
pub async fn serve(port: u16, config: Config) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    info!("vibird bridge listening on ws://{addr}");
    let config = Arc::new(config);
    // TODO(v0.1): 通过 mDNS 广播 `_vibird._tcp` 让设备自动发现。
    loop {
        let (stream, peer) = listener.accept().await?;
        info!("device connecting from {peer}");
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, config).await {
                warn!("connection {peer} ended: {e}");
            }
        });
    }
}

/// 把 [`Downlink`] 序列化成文本帧。
fn frame(d: &Downlink) -> Result<Message> {
    Ok(Message::text(serde_json::to_string(d)?))
}

async fn handle_conn(stream: TcpStream, config: Arc<Config>) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut tx, mut rx) = ws.split();

    // 长按期间(AudioStart…AudioEnd)累积的原始 PCM。
    let mut pcm: Vec<u8> = Vec::new();
    let mut capturing = false;
    let mut sample_rate = 16_000u32;

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
                    Uplink::Hello {
                        device_id,
                        fw_version,
                        protocol,
                        caps,
                    } => {
                        info!("hello from {device_id} (fw {fw_version}, proto {protocol}, caps {caps:?})");
                        tx.send(frame(&Downlink::Welcome {
                            protocol: PROTOCOL_VERSION,
                            bridge_version: env!("CARGO_PKG_VERSION").to_string(),
                        })?)
                        .await?;
                        tx.send(frame(&Downlink::SetState(AgentState::Idle))?)
                            .await?;
                    }
                    Uplink::AudioStart {
                        sample_rate: sr,
                        format,
                    } => {
                        debug!("audio start: {sr} Hz {format:?}");
                        sample_rate = sr;
                        pcm.clear();
                        capturing = true;
                        tx.send(frame(&Downlink::SetState(AgentState::Listening))?)
                            .await?;
                    }
                    Uplink::AudioEnd => {
                        capturing = false;
                        info!("audio end: {} bytes @ {sample_rate} Hz", pcm.len());
                        tx.send(frame(&Downlink::SetState(AgentState::Thinking))?)
                            .await?;
                        // PCM 字节 → i16 样本(小端)。
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
                                tx.send(frame(&Downlink::SetState(AgentState::Working {
                                    tool: None,
                                }))?)
                                .await?;
                            }
                            Ok(_) => {
                                warn!("ASR returned empty");
                                tx.send(frame(&Downlink::SetState(AgentState::Idle))?)
                                    .await?;
                            }
                            Err(e) => {
                                warn!("ASR failed: {e}");
                                tx.send(frame(&Downlink::SetState(AgentState::Error {
                                    message: "ASR failed".into(),
                                }))?)
                                .await?;
                            }
                        }
                    }
                    Uplink::Button { event } => info!("button: {event:?}"),
                    Uplink::Approval {
                        request_id,
                        decision,
                    } => {
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
                    debug!(
                        "unexpected binary frame ({} bytes) outside capture",
                        buf.len()
                    );
                }
            }
            Message::Ping(p) => tx.send(Message::Pong(p)).await?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}
