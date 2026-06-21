//! 把语音转写注入正在运行的 Claude Code 会话。
//!
//! v0.1 走 `tmux send-keys`:这是当下唯一可靠地往活动交互式会话里塞输入的办法(没有官方 IPC)。
//! 之后(v0.2+)走 Agent SDK / stream-json 更干净。

use anyhow::Result;
use tracing::{info, warn};

/// 注入器。
#[derive(Clone, Default)]
pub struct Inject {
    /// tmux 目标(session / window / pane,如 `"claude"` 或 `"0:1.0"`);`None` 则只记录不注入。
    pub tmux_target: Option<String>,
}

impl Inject {
    /// 指定 tmux 目标。
    pub fn tmux(target: impl Into<String>) -> Self {
        Self {
            tmux_target: Some(target.into()),
        }
    }

    /// 把 `text` 当一行输入注入(字面量文本 + 回车)。注入失败只告警、不让连接挂掉。
    pub fn send(&self, text: &str) -> Result<()> {
        let Some(target) = &self.tmux_target else {
            info!("(无 tmux 目标)转写:{text}");
            return Ok(());
        };
        // `-l` 表示字面量,避免 tmux 把内容当快捷键解释;随后单独发 Enter 提交。
        let typed = run_tmux(["send-keys", "-t", target, "-l", text]);
        let entered = run_tmux(["send-keys", "-t", target, "Enter"]);
        if !(typed && entered) {
            warn!("tmux 注入失败(目标 {target} 是否存在?);转写:{text}");
        }
        Ok(())
    }
}

fn run_tmux<const N: usize>(args: [&str; N]) -> bool {
    std::process::Command::new("tmux")
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
