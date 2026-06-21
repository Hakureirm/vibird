//! 把语音转写注入正在运行的 Claude Code 会话。
//!
//! 没有官方 IPC,所以有两条路:
//! - **`Tmux`**:`tmux send-keys` 到目标 pane —— 最可靠,无需系统权限,但 Claude Code 得跑在 tmux 里。
//! - **`Keystroke`**(仅 macOS):`osascript` 把文本经剪贴板**粘贴进前台窗口** + 回车 —— 能注入到
//!   普通终端里的 Claude Code(不必 tmux),但需一次性授予「辅助功能」权限,且说话时该窗口要在前台。
//!
//! 之后(v0.2+)走 Agent SDK / stream-json 更干净。

use anyhow::Result;
use tracing::{info, warn};

/// 注入器后端。
#[derive(Clone, Default)]
pub enum Inject {
    /// 只记录转写、不注入(默认)。
    #[default]
    Log,
    /// `tmux send-keys` 到目标(session / window / pane)。
    Tmux(String),
    /// macOS 模拟键盘:转写经剪贴板粘贴进前台窗口 + 回车。
    Keystroke,
}

impl Inject {
    /// 指定 tmux 目标。
    pub fn tmux(target: impl Into<String>) -> Self {
        Inject::Tmux(target.into())
    }

    /// macOS 模拟键盘注入(前台窗口)。
    pub fn keystroke() -> Self {
        Inject::Keystroke
    }

    /// 把 `text` 当一行输入注入(文本 + 回车)。注入失败只告警、不让连接挂掉。
    pub fn send(&self, text: &str) -> Result<()> {
        match self {
            Inject::Log => info!("(只记录,未注入)转写:{text}"),
            Inject::Tmux(target) => {
                // `-l` 字面量,避免 tmux 把内容当快捷键;随后单独发 Enter 提交。
                let typed = run_tmux(&["send-keys", "-t", target, "-l", text]);
                let entered = run_tmux(&["send-keys", "-t", target, "Enter"]);
                if !(typed && entered) {
                    warn!(
                        "tmux 注入失败(目标 {target} 在 socket {TMUX_SOCKET} 上吗?\
                         建会话用:tmux -S {TMUX_SOCKET} new -s {target});转写:{text}"
                    );
                }
            }
            Inject::Keystroke => {
                if !run_keystroke(text) {
                    warn!("keystroke 注入失败(是否已授予终端「辅助功能」权限?);转写:{text}");
                }
            }
        }
        Ok(())
    }
}

/// 固定的绝对路径 tmux socket。bridge 与用户建会话都用它,绕开两边 $TMPDIR 不同导致的
/// 默认 socket(`$TMPDIR/tmux-$UID/default`)不一致问题 —— 否则 send-keys 找不到对方的会话。
const TMUX_SOCKET: &str = "/tmp/vibird-tmux.sock";

fn run_tmux(args: &[&str]) -> bool {
    std::process::Command::new("tmux")
        .arg("-S")
        .arg(TMUX_SOCKET)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// macOS:把 `text` 放剪贴板 → Cmd+V 粘进前台窗口 → 回车。用剪贴板而非逐字 keystroke,
/// 是因为 `keystroke` 对中文等非 ASCII 不可靠,剪贴板粘贴对 Unicode 稳。
fn run_keystroke(text: &str) -> bool {
    // 转义 AppleScript 字符串字面量里的 \ 和 "。
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "set the clipboard to \"{escaped}\"\n\
         tell application \"System Events\"\n\
         keystroke \"v\" using command down\n\
         delay 0.15\n\
         key code 36\n\
         end tell"
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
