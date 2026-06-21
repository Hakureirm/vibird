//! 生成一个程序化占位 `.veap`(呼吸的青色圆点),用于在 Liz 美术到位前测试固件播放器。
//! 用法:`cargo run --example gen_placeholder -- <输出路径> [画布宽 画布高]`

use vibird_emote::build::Builder;
use vibird_emote::LOOP_END_LAST;
use vibird_emote_pack::{frames_to_clip, rgb565};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "placeholder.veap".into());
    let w: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(128);
    let h: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(128);

    let bg = rgb565(18, 20, 28); // 深底
    let dot = rgb565(90, 210, 190); // 青
    let nframes = 24u16;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);

    let mut canvas_frames = Vec::with_capacity(nframes as usize);
    for f in 0..nframes {
        let phase = (f as f32 / nframes as f32 * std::f32::consts::TAU).sin(); // -1..1
        let r = 26.0 + 8.0 * phase; // 呼吸半径
        let r2 = r * r;
        let mut canvas = vec![bg; w as usize * h as usize];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                if dx * dx + dy * dy <= r2 {
                    canvas[y as usize * w as usize + x as usize] = dot;
                }
            }
        }
        canvas_frames.push(canvas);
    }

    let frames = frames_to_clip(&canvas_frames, w, h);
    let mut b = Builder::new(w, h);
    b.add_clip("idle", 24, true, 0, LOOP_END_LAST, frames);
    let bytes = b.to_bytes();
    std::fs::write(&out, &bytes).expect("写出失败");
    println!(
        "✓ 占位 .veap → {out} ({} 字节, {w}x{h}, 24 帧)",
        bytes.len()
    );
}
