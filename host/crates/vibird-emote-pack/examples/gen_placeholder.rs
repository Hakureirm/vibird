//! 生成多状态占位 `.veap`:7 个 agent 状态各一个程序化表情(异色 + 动作),用于在 Liz 正式美术
//! (Live2D)到位前演示固件的多片段切换。片段名对齐 `AgentState` 的 snake_case。
//! 用法:`cargo run --example gen_placeholder -- <输出路径> [宽 高]`

use vibird_emote::build::Builder;
use vibird_emote::LOOP_END_LAST;
use vibird_emote_pack::{frames_to_clip, rgb565};

#[derive(Clone, Copy)]
enum Motion {
    Breathe,
    Pulse,
    Orbit,
    Spin,
    Flash,
    Settle,
    ErrorFlash,
}

fn u8c(x: f32) -> u8 {
    x.clamp(0.0, 255.0) as u8
}

/// 把基色按亮度 `k`(0..1)缩放后转 RGB565。
fn dim(base: (u8, u8, u8), k: f32) -> u16 {
    rgb565(
        u8c(base.0 as f32 * k),
        u8c(base.1 as f32 * k),
        u8c(base.2 as f32 * k),
    )
}

fn fill_disc(canvas: &mut [u16], w: u16, h: u16, cx: f32, cy: f32, r: f32, color: u16) {
    let r2 = r * r;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            if dx * dx + dy * dy <= r2 {
                canvas[y as usize * w as usize + x as usize] = color;
            }
        }
    }
}

fn gen_clip(
    w: u16,
    h: u16,
    bg: u16,
    base: (u8, u8, u8),
    motion: Motion,
    nframes: u16,
) -> Vec<Vec<u16>> {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let color = rgb565(base.0, base.1, base.2);
    let tau = std::f32::consts::TAU;
    let pi = std::f32::consts::PI;
    let mut out = Vec::with_capacity(nframes as usize);
    for f in 0..nframes {
        let t = f as f32 / nframes as f32;
        let mut canvas = vec![bg; w as usize * h as usize];
        match motion {
            Motion::Breathe => fill_disc(
                &mut canvas,
                w,
                h,
                cx,
                cy,
                24.0 + 8.0 * (t * tau).sin(),
                color,
            ),
            Motion::Pulse => fill_disc(
                &mut canvas,
                w,
                h,
                cx,
                cy,
                6.0 + 26.0 * t,
                dim(base, 1.0 - t),
            ),
            Motion::Orbit => {
                let a = t * tau;
                fill_disc(
                    &mut canvas,
                    w,
                    h,
                    cx + 18.0 * a.cos(),
                    cy + 18.0 * a.sin(),
                    13.0,
                    color,
                );
            }
            Motion::Spin => {
                let a = t * tau;
                for s in 0..2 {
                    let aa = a + s as f32 * pi;
                    fill_disc(
                        &mut canvas,
                        w,
                        h,
                        cx + 16.0 * aa.cos(),
                        cy + 16.0 * aa.sin(),
                        9.0,
                        color,
                    );
                }
            }
            Motion::Flash => fill_disc(
                &mut canvas,
                w,
                h,
                cx,
                cy,
                24.0,
                dim(base, 0.4 + 0.6 * (t * tau * 2.0).sin().abs()),
            ),
            Motion::Settle => fill_disc(
                &mut canvas,
                w,
                h,
                cx,
                cy,
                ((t * 2.0).min(1.0) * 26.0).max(3.0),
                color,
            ),
            Motion::ErrorFlash => {
                let on = (f / (nframes / 4).max(1)) % 2 == 0;
                fill_disc(
                    &mut canvas,
                    w,
                    h,
                    cx,
                    cy,
                    24.0,
                    if on { color } else { dim(base, 0.25) },
                );
            }
        }
        out.push(canvas);
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "placeholder.veap".into());
    let w: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(128);
    let h: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(128);
    let bg = rgb565(18, 20, 28);

    // (片段名, 基色, 动作)—— 名字对齐 AgentState 的 snake_case,将来 SetState 直接 set_clip 即可。
    let clips: &[(&str, (u8, u8, u8), Motion)] = &[
        ("idle", (90, 210, 190), Motion::Breathe),
        ("listening", (120, 230, 120), Motion::Pulse),
        ("thinking", (120, 170, 250), Motion::Orbit),
        ("working", (245, 180, 90), Motion::Spin),
        ("awaiting_approval", (240, 120, 120), Motion::Flash),
        ("done", (130, 230, 140), Motion::Settle),
        ("error", (240, 90, 90), Motion::ErrorFlash),
    ];

    let mut b = Builder::new(w, h);
    for (name, color, motion) in clips {
        let frames = gen_clip(w, h, bg, *color, *motion, 18);
        b.add_clip(
            name,
            18,
            true,
            0,
            LOOP_END_LAST,
            frames_to_clip(&frames, w, h),
        );
    }
    let bytes = b.to_bytes();
    std::fs::write(&out, &bytes).expect("写出失败");
    println!(
        "✓ 多状态占位 .veap → {out} ({} 字节, {w}x{h}, {} 个状态片段)",
        bytes.len(),
        clips.len()
    );
}
