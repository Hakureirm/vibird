//! `vibird` — the command-line entry point.
//!
//! v0.1 wires up `serve` (the bridge) and stubs the rest of the surface so the shape is visible:
//! `hook` (Claude Code hook handler), `mcp` (MCP server), `config` (serial device setup),
//! `service` (daemon install), `claude` (install the Claude Code plugin).

use anyhow::Result;
use clap::{Parser, Subcommand};

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
        /// ASR backend: `stub` (placeholder) or `cloud` (reads VIBIRD_ASR_* env).
        #[arg(long, default_value = "stub")]
        asr: String,
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
        Cmd::Serve { port, tmux, asr } => {
            let asr = match asr.as_str() {
                "cloud" => vibird_bridge::Asr::cloud_from_env()?,
                _ => vibird_bridge::Asr::stub(),
            };
            let inject = match tmux {
                Some(t) => vibird_bridge::Inject::tmux(t),
                None => vibird_bridge::Inject::default(),
            };
            let config = vibird_bridge::Config { asr, inject };
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(vibird_bridge::serve(port, config))?;
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
        // TODO: 下面这些桩在 v0.1–v0.3 落地。
        Cmd::Mcp => println!("TODO MCP server (rmcp, stdio)"),
        Cmd::Config => println!("TODO serial device configuration"),
        Cmd::Service { .. } => println!("TODO service install (launchd/systemd)"),
        Cmd::Claude { .. } => println!("TODO Claude Code plugin install"),
    }
    Ok(())
}
