//! 最小 MCP 服务器(stdio,JSON-RPC 2.0):把设备控制工具暴露给 Claude Code。
//!
//! 故意**不引入 rmcp** —— 手写最小协议(initialize / tools/list / tools/call / ping),工具调用经
//! 控制面 [`crate::push_downlink`] 推给设备。`vibird mcp` 运行它;插件的 `.mcp.json` 指向这里。

use crate::{control_port, push_downlink};
use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use vibird_protocol::{AgentState, Downlink, NotifyLevel};

/// 跑 MCP 服务器(stdio),把设备工具的调用转成控制面状态推送。
pub async fn run(ws_port: u16) -> Result<()> {
    let cport = control_port(ws_port);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = req.get("id").cloned();
        let result: Option<Value> = match method {
            "initialize" => Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "vibird", "version": env!("CARGO_PKG_VERSION") }
            })),
            "tools/list" => Some(json!({ "tools": tools_spec() })),
            "tools/call" => {
                Some(handle_call(cport, &req.get("params").cloned().unwrap_or(json!({}))).await)
            }
            "ping" => Some(json!({})),
            _ => None, // 通知(无 id)或未知方法:不回
        };
        // 只有带 id 的请求才回响应(通知不回)。
        if let (Some(id), Some(result)) = (id, result) {
            let out = json!({ "jsonrpc": "2.0", "id": id, "result": result });
            stdout
                .write_all(serde_json::to_string(&out)?.as_bytes())
                .await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// 工具清单(JSON Schema)。
fn tools_spec() -> Value {
    json!([
        {
            "name": "vibird_set_state",
            "description": "在 Vibird 设备上显示一个 agent 状态表情(Liz 的脸跟随)。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "state": { "type": "string", "enum": ["idle","listening","thinking","working","done","error"] },
                    "tool": { "type": "string", "description": "working 时的工具名(可选)" }
                },
                "required": ["state"]
            }
        },
        {
            "name": "vibird_notify",
            "description": "在 Vibird 设备上弹一条短提示(只在重要时用)。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "level": { "type": "string", "enum": ["info","success","warn","error"] },
                    "text": { "type": "string" }
                },
                "required": ["text"]
            }
        }
    ])
}

/// 处理 `tools/call`。
async fn handle_call(cport: u16, params: &Value) -> Value {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let outcome = match name {
        "vibird_set_state" => push_downlink(cport, Downlink::SetState(parse_state(&args))).await,
        "vibird_notify" => {
            let level = parse_level(args.get("level").and_then(|l| l.as_str()));
            let text = args
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string();
            push_downlink(cport, Downlink::Notify { level, text }).await
        }
        other => Err(anyhow::anyhow!("unknown tool {other}")),
    };
    match outcome {
        Ok(()) => json!({ "content": [{ "type": "text", "text": "ok" }] }),
        Err(e) => {
            json!({ "content": [{ "type": "text", "text": format!("error: {e}") }], "isError": true })
        }
    }
}

fn parse_state(args: &Value) -> AgentState {
    let tool = args.get("tool").and_then(|t| t.as_str()).map(String::from);
    match args.get("state").and_then(|s| s.as_str()).unwrap_or("idle") {
        "listening" => AgentState::Listening,
        "thinking" => AgentState::Thinking,
        "working" => AgentState::Working { tool },
        "done" => AgentState::Done,
        "error" => AgentState::Error {
            message: "error".into(),
        },
        _ => AgentState::Idle,
    }
}

fn parse_level(s: Option<&str>) -> NotifyLevel {
    match s {
        Some("success") => NotifyLevel::Success,
        Some("warn") => NotifyLevel::Warn,
        Some("error") => NotifyLevel::Error,
        _ => NotifyLevel::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_two_tools() {
        let t = tools_spec();
        assert_eq!(t.as_array().unwrap().len(), 2);
        assert_eq!(t[0]["name"], "vibird_set_state");
    }

    #[test]
    fn parses_state_and_level() {
        assert!(matches!(
            parse_state(&json!({"state":"working","tool":"bash"})),
            AgentState::Working { tool: Some(_) }
        ));
        assert!(matches!(
            parse_state(&json!({"state":"idle"})),
            AgentState::Idle
        ));
        assert!(matches!(parse_level(Some("warn")), NotifyLevel::Warn));
        assert!(matches!(parse_level(None), NotifyLevel::Info));
    }
}
