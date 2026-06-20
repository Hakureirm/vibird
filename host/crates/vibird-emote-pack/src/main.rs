//! `vibird-emote-pack` CLI —— 把 GIF 片段打包成 `.veap`。
//!
//! 例:`vibird-emote-pack --canvas 128x128 -o liz.veap idle=idle.gif listening=listening.gif`

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use std::path::PathBuf;
use vibird_emote_pack::{pack, ClipInput};

#[derive(Parser)]
#[command(name = "vibird-emote-pack", about = "GIF → .veap 表情打包器")]
struct Cli {
    /// 画布尺寸,如 128x128(应与目标屏一致)
    #[arg(long, default_value = "128x128")]
    canvas: String,
    /// 背景色(透明像素合成到此),十六进制 RRGGBB
    #[arg(long, default_value = "000000")]
    bg: String,
    /// 帧率覆盖(默认取 GIF 首帧延迟换算)
    #[arg(long)]
    fps: Option<u16>,
    /// 输出 .veap 路径
    #[arg(short, long)]
    out: PathBuf,
    /// 片段:`name=path.gif`(可多个,name 对应 AgentState,如 idle/listening/thinking…)
    #[arg(value_name = "NAME=GIF")]
    clips: Vec<String>,
}

fn parse_canvas(s: &str) -> Result<(u16, u16)> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| anyhow!("画布格式应为 WxH,如 128x128"))?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}

fn parse_bg(s: &str) -> Result<u16> {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16)?;
    Ok(vibird_emote_pack::rgb565(
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.clips.is_empty() {
        bail!("至少给一个片段:name=path.gif");
    }
    let (cw, ch) = parse_canvas(&cli.canvas)?;
    let bg = parse_bg(&cli.bg)?;
    let mut inputs = Vec::new();
    for spec in &cli.clips {
        let (name, path) = spec
            .split_once('=')
            .ok_or_else(|| anyhow!("片段格式应为 name=path.gif:{spec}"))?;
        inputs.push(ClipInput {
            name: name.to_string(),
            path: PathBuf::from(path),
            looping: true,
            fps_override: cli.fps,
        });
    }
    let bytes = pack(cw, ch, bg, &inputs)?;
    std::fs::write(&cli.out, &bytes)?;
    println!(
        "✓ 打包 {} 个片段 → {} ({} 字节, 画布 {}x{})",
        inputs.len(),
        cli.out.display(),
        bytes.len(),
        cw,
        ch
    );
    Ok(())
}
