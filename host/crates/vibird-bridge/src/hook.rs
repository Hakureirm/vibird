//! Claude Code hook 事件 → 设备状态映射,以及把状态推给桥接控制面。
//!
//! `vibird hook <event>` 由安装的插件在每个 hook 点调用;它把事件映射成 [`AgentState`] 并经控制面
//! 推给设备,从而让设备表情实时反映 agent 状态(空闲 / 在想 / 干活 / 等你 / 完成)。

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use vibird_protocol::{AgentState, Downlink};

/// 把 hook 事件名(+ 可选工具名)映射成设备要显示的状态。`None` = 忽略该事件。
pub fn hook_event_to_state(event: &str, tool: Option<String>) -> Option<AgentState> {
    match event {
        "UserPromptSubmit" => Some(AgentState::Thinking),
        "PreToolUse" | "PostToolUse" => Some(AgentState::Working { tool }),
        "Notification" => Some(AgentState::AwaitingApproval {
            request_id: 0,
            summary: tool.unwrap_or_else(|| "需要你确认".into()),
        }),
        "Stop" | "SubagentStop" => Some(AgentState::Done),
        _ => None,
    }
}

/// 连接控制面(`127.0.0.1:control_port`)并推一条 [`Downlink`](单行 JSON)。
pub async fn push_downlink(control_port: u16, d: Downlink) -> Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", control_port))
        .await
        .with_context(|| format!("连不上控制面 127.0.0.1:{control_port}(桥接在运行吗?)"))?;
    let line = serde_json::to_string(&d)? + "\n";
    stream.write_all(line.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// 推一条状态([`push_downlink`] 的便捷封装)。
pub async fn push_state(control_port: u16, state: AgentState) -> Result<()> {
    push_downlink(control_port, Downlink::SetState(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_events() {
        assert!(matches!(
            hook_event_to_state("UserPromptSubmit", None),
            Some(AgentState::Thinking)
        ));
        assert!(matches!(
            hook_event_to_state("PreToolUse", Some("bash".into())),
            Some(AgentState::Working { tool: Some(_) })
        ));
        assert!(matches!(
            hook_event_to_state("Stop", None),
            Some(AgentState::Done)
        ));
        assert!(hook_event_to_state("Whatever", None).is_none());
    }
}
