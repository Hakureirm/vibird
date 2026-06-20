//! vibird-emote-pack —— 把 GIF(Liz 的美术)打包成 `.veap` 表情资源。
//!
//! 流程:GIF 帧 → 缩放到画布 → 按 alpha 合成到背景 → RGB565 → 第 0 帧全画布、其余按
//! 包围盒 delta(区域刷新)→ `vibird_emote::build::Builder` 写出 `.veap`。

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use vibird_emote::build::{diff_bbox, full_frame, Builder, RectData};

/// RGB 三通道转 RGB565。
pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}

/// RGB565 还原成 8bit (r,g,b)(用于把背景色拆出来做 alpha 合成)。
fn rgb565_to_rgb(c: u16) -> (u8, u8, u8) {
    let r = ((c >> 11) & 0x1f) as u8;
    let g = ((c >> 5) & 0x3f) as u8;
    let b = (c & 0x1f) as u8;
    // 5/6/5 → 8bit 用高位复制补低位,避免偏暗
    (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    )
}

/// 一张 RGBA 图缩放到画布尺寸,按 alpha 合成到 `bg`,转成 RGB565 画布(行优先)。
pub fn rgba_to_canvas(img: &image::RgbaImage, w: u16, h: u16, bg: u16) -> Vec<u16> {
    let resized;
    let src = if img.width() == w as u32 && img.height() == h as u32 {
        img
    } else {
        resized = image::imageops::resize(
            img,
            w as u32,
            h as u32,
            image::imageops::FilterType::Lanczos3,
        );
        &resized
    };
    let (br, bgc, bb) = rgb565_to_rgb(bg);
    let mut out = Vec::with_capacity(w as usize * h as usize);
    for p in src.pixels() {
        let [r, g, b, a] = p.0;
        let (r, g, b) = if a == 255 {
            (r, g, b)
        } else {
            let a = a as u16;
            let blend = |s: u8, d: u8| (((s as u16 * a) + (d as u16 * (255 - a))) / 255) as u8;
            (blend(r, br), blend(g, bgc), blend(b, bb))
        };
        out.push(rgb565(r, g, b));
    }
    out
}

/// 一组画布帧(每帧 RGB565,行优先)→ 片段帧:第 0 帧全画布,其余为相对上一帧的包围盒 delta。
pub fn frames_to_clip(canvas_frames: &[Vec<u16>], w: u16, h: u16) -> Vec<Vec<RectData>> {
    let mut frames = Vec::with_capacity(canvas_frames.len());
    for (i, f) in canvas_frames.iter().enumerate() {
        if i == 0 {
            frames.push(full_frame(f, w, h));
        } else {
            frames.push(diff_bbox(&canvas_frames[i - 1], f, w, h));
        }
    }
    frames
}

/// 解码 GIF → 画布帧序列 + 估算 fps(取首帧延迟)。
pub fn decode_gif(path: &Path, w: u16, h: u16, bg: u16) -> Result<(Vec<Vec<u16>>, u16)> {
    use image::AnimationDecoder;
    let file = std::fs::File::open(path).with_context(|| format!("打不开 {}", path.display()))?;
    let dec = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
        .with_context(|| format!("{} 不是合法 GIF", path.display()))?;
    let frames = dec
        .into_frames()
        .collect_frames()
        .context("解码 GIF 帧失败")?;
    anyhow::ensure!(!frames.is_empty(), "GIF 没有帧:{}", path.display());
    // fps:用第一帧的延迟换算
    let (numer, denom) = frames[0].delay().numer_denom_ms();
    let ms = numer.checked_div(denom).unwrap_or(100); // denom==0 → 默认 100ms
    let fps = 1000u32
        .checked_div(ms)
        .map(|f| f.clamp(1, 120) as u16)
        .unwrap_or(10);
    let canvas = frames
        .into_iter()
        .map(|f| rgba_to_canvas(&f.into_buffer(), w, h, bg))
        .collect();
    Ok((canvas, fps))
}

/// 一个待打包的片段(逻辑名 + GIF 路径)。
pub struct ClipInput {
    pub name: String,
    pub path: PathBuf,
    pub looping: bool,
    pub fps_override: Option<u16>,
}

/// 把多个 GIF 片段打包成 `.veap` 字节。
pub fn pack(canvas_w: u16, canvas_h: u16, bg: u16, clips: &[ClipInput]) -> Result<Vec<u8>> {
    let mut b = Builder::new(canvas_w, canvas_h);
    for c in clips {
        let (frames, fps) = decode_gif(&c.path, canvas_w, canvas_h, bg)?;
        let fps = c.fps_override.unwrap_or(fps);
        let clip_frames = frames_to_clip(&frames, canvas_w, canvas_h);
        b.add_clip(
            &c.name,
            fps,
            c.looping,
            0,
            vibird_emote::LOOP_END_LAST,
            clip_frames,
        );
    }
    Ok(b.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_primaries() {
        assert_eq!(rgb565(255, 0, 0), 0xF800);
        assert_eq!(rgb565(0, 255, 0), 0x07E0);
        assert_eq!(rgb565(0, 0, 255), 0x001F);
        assert_eq!(rgb565(0, 0, 0), 0x0000);
    }

    #[test]
    fn fully_transparent_pixel_becomes_bg() {
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 0])); // 全透明 → 应得背景
        let bg = rgb565(0, 0, 255);
        let canvas = rgba_to_canvas(&img, 1, 1, bg);
        assert_eq!(canvas[0], bg);
    }

    #[test]
    fn opaque_pixel_kept() {
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([0, 255, 0, 255]));
        let canvas = rgba_to_canvas(&img, 1, 1, rgb565(0, 0, 0));
        assert_eq!(canvas[0], rgb565(0, 255, 0));
    }

    #[test]
    fn pack_roundtrips_through_parser() {
        // 不依赖真实 GIF:直接构造画布帧 → frames_to_clip → Builder → 解析校验
        let f0 = vec![rgb565(10, 10, 10); 4];
        let mut f1 = f0.clone();
        f1[0] = rgb565(255, 255, 255);
        let clip_frames = frames_to_clip(&[f0, f1], 2, 2);
        let mut b = Builder::new(2, 2);
        b.add_clip(
            "idle",
            12,
            true,
            0,
            vibird_emote::LOOP_END_LAST,
            clip_frames,
        );
        let bytes = b.to_bytes();
        let pack = vibird_emote::Pack::parse(&bytes).unwrap();
        let clip = pack.clip_by_name("idle").unwrap();
        assert_eq!(clip.frame_count(), 2);
        assert_eq!(clip.fps(), 12);
        // 第二帧应只含 1 个像素的包围盒(1x1)
        let f1_rects: Vec<_> = clip.frame(1).unwrap().rects().collect();
        assert_eq!(f1_rects.len(), 1);
        assert_eq!((f1_rects[0].w, f1_rects[0].h), (1, 1));
    }
}
