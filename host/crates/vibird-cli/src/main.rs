//! `vibird` — the command-line entry point.
//!
//! v0.1 wires up `serve` (the bridge) and stubs the rest of the surface so the shape is visible:
//! `hook` (Claude Code hook handler), `mcp` (MCP server), `config` (serial device setup),
//! `service` (daemon install), `claude` (install the Claude Code plugin).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "vibird",
    version,
    about = "Vibird — voice + status companion for vibe coding"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the bridge daemon: WebSocket server + ASR + agent integration.
    Serve {
        /// Port to listen on.
        #[arg(long, default_value_t = vibird_bridge::DEFAULT_PORT)]
        port: u16,
        /// tmux target (session/window/pane) to inject transcripts into; omit to only log.
        #[arg(long)]
        tmux: Option<String>,
        /// macOS only: inject transcripts into the FOREGROUND window via keystroke (no tmux needed).
        /// Needs Accessibility permission for the terminal running the bridge. Overrides --tmux.
        #[arg(long)]
        keystroke: bool,
        /// ASR backend: `stub` / `cloud` (VIBIRD_ASR_* env) / `local` (SenseVoice via VIBIRD_ASR_SCRIPT).
        #[arg(long, default_value = "stub")]
        asr: String,
    },
    /// Simulate a device: stream a WAV's PCM to the bridge (test the voice loop end-to-end).
    Simulate {
        /// WAV file (16 kHz mono PCM) to stream as push-to-talk audio.
        audio: PathBuf,
        /// Bridge port.
        #[arg(long, default_value_t = vibird_bridge::DEFAULT_PORT)]
        port: u16,
    },
    /// Internal: handle a Claude Code hook event (invoked by the installed plugin).
    Hook {
        /// Hook event name (PreToolUse, Notification, Stop, ...).
        event: String,
    },
    /// Run the MCP server (stdio) exposing device tools to the agent.
    Mcp,
    /// Configure a device over USB serial (first-time WiFi / bridge setup).
    Config,
    /// Manage the background service (launchd / systemd).
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Install/uninstall the Claude Code integration (plugin: hooks + MCP + skills).
    Claude {
        #[command(subcommand)]
        action: ClaudeAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    Install,
    Uninstall,
    Status,
}

#[derive(Subcommand)]
enum ClaudeAction {
    Install,
    Uninstall,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    match Cli::parse().cmd {
        Cmd::Serve {
            port,
            tmux,
            keystroke,
            asr,
        } => {
            let asr = match asr.as_str() {
                "cloud" => vibird_bridge::Asr::cloud_from_env()?,
                "local" => vibird_bridge::Asr::local_from_env()?,
                _ => vibird_bridge::Asr::stub(),
            };
            let inject = if keystroke {
                vibird_bridge::Inject::keystroke()
            } else if let Some(t) = tmux {
                vibird_bridge::Inject::tmux(t)
            } else {
                vibird_bridge::Inject::default()
            };
            let config = vibird_bridge::Config { asr, inject };
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(vibird_bridge::serve(port, config))?;
        }
        Cmd::Simulate { audio, port } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(vibird_bridge::simulate(port, &audio))?;
        }
        Cmd::Hook { event } => {
            // 读 Claude Code 经 stdin 传入的 hook JSON,取 tool_name;映射成状态后经控制面推给设备。
            use std::io::Read;
            let mut buf = String::new();
            let _ = std::io::stdin().read_to_string(&mut buf);
            let tool = serde_json::from_str::<serde_json::Value>(&buf)
                .ok()
                .and_then(|v| {
                    v.get("tool_name")
                        .and_then(|t| t.as_str())
                        .map(String::from)
                });
            if let Some(state) = vibird_bridge::hook_event_to_state(&event, tool) {
                let rt = tokio::runtime::Runtime::new()?;
                let cport = vibird_bridge::control_port(vibird_bridge::DEFAULT_PORT);
                if let Err(e) = rt.block_on(vibird_bridge::push_state(cport, state)) {
                    eprintln!("vibird hook: {e}"); // 只告警,绝不阻断 Claude Code
                }
            }
        }
        Cmd::Mcp => {
            // 最小 MCP 服务器(stdio):把设备工具暴露给 Claude Code。
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(vibird_bridge::run_mcp(vibird_bridge::DEFAULT_PORT))?;
        }
        // TODO: 下面这些桩在后续版本落地(需硬件 / OS 细节)。
        Cmd::Config => println!("TODO serial device configuration"),
        Cmd::Service { .. } => println!("TODO service install (launchd/systemd)"),
        Cmd::Claude { action } => match action {
            ClaudeAction::Install => {
                println!("Vibird 的 Claude Code 插件在仓库 claude-plugin/ 下。");
                println!("安装(二选一):");
                println!(
                    "  1) Claude Code 里把本仓库加为 plugin marketplace,再 /plugin 启用 vibird;"
                );
                println!("  2) 把 claude-plugin/ 链接到 ~/.claude/plugins/vibird。");
                println!("插件 hooks 会调用 `vibird hook`,先确保 `vibird serve` 在运行。");
            }
            ClaudeAction::Uninstall => {
                println!("在 Claude Code /plugin 停用 vibird,或删除 ~/.claude/plugins/vibird。");
            }
        },
    }
    Ok(())
}
