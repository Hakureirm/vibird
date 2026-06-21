//! Vibird firmware — animation spike (v0.3: smooth anti-aliased vector style).
//!
//! Verified at high FPS on real hardware. This iteration drops pixel-art for a clean, anti-aliased
//! vector look that suits the colour LCD: soft-edged rounded shapes composited with coverage-based AA
//! (no sqrt — squared-distance edges), a precomputed gradient backdrop, gentle breathing, and smooth
//! blinks. No fake drop-shadows. ESP32-S3 has an FPU, so the per-pixel f32 math is cheap.
//!
//! Pins (source-verified — see docs/research/atoms3r-hardware.md):
//!   Display GC9107 (SPI2): SCLK=15, MOSI=21, CS=14, DC=42, RST=48; 40 MHz; panel 128x128, offset_y=32.
//!   Backlight: LP5562 LED driver on internal I2C (SDA=45, SCL=0), addr 0x30, brightness = reg 0x0E.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    main,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::{Duration, Instant, Rate},
};
use libm::sinf;
use log::{info, warn};
use mipidsi::{
    Builder,
    interface::SpiInterface,
    models::GC9107,
    options::{ColorInversion, ColorOrder},
};
use vibird_emote::{Pack, Player};

esp_bootloader_esp_idf::esp_app_desc!();

const W: i32 = 128;
const H: i32 = 128;
const LP5562_ADDR: u8 = 0x30;

/// 内嵌占位表情包(呼吸青点);Liz 美术到位后换成正式 .veap(见 assets/placeholder.veap)。
static PLACEHOLDER: &[u8] = include_bytes!("../../../assets/placeholder.veap");

/// 把 .veap 的原始 RGB565(u16)转成 embedded-graphics 的 Rgb565。
#[inline]
fn rgb565_from_raw(c: u16) -> Rgb565 {
    Rgb565::new((c >> 11) as u8, ((c >> 5) & 0x3f) as u8, (c & 0x1f) as u8)
}

#[inline]
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Blend two Rgb565 colours in 5/6/5 space.
#[inline]
fn lerp_col(a: Rgb565, b: Rgb565, t: f32) -> Rgb565 {
    let t = clamp01(t);
    let r = a.r() as f32 + (b.r() as f32 - a.r() as f32) * t;
    let g = a.g() as f32 + (b.g() as f32 - a.g() as f32) * t;
    let bl = a.b() as f32 + (b.b() as f32 - a.b() as f32) * t;
    Rgb565::new((r + 0.5) as u8, (g + 0.5) as u8, (bl + 0.5) as u8)
}

/// Off-screen 128x128 framebuffer.
struct Fb {
    px: Vec<Rgb565>,
}
impl Fb {
    fn new() -> Self {
        Self {
            px: alloc::vec![Rgb565::BLACK; (W * H) as usize],
        }
    }
    #[inline]
    fn idx(x: i32, y: i32) -> usize {
        (y * W + x) as usize
    }
    /// Alpha-blend `col` at (x,y) with coverage `a`.
    #[inline]
    fn blend(&mut self, x: i32, y: i32, col: Rgb565, a: f32) {
        if a <= 0.0 || !(0..W).contains(&x) || !(0..H).contains(&y) {
            return;
        }
        let i = Self::idx(x, y);
        self.px[i] = if a >= 1.0 {
            col
        } else {
            lerp_col(self.px[i], col, a)
        };
    }
}

/// Anti-aliased filled ellipse (coverage from squared distance — no sqrt).
fn ellipse(fb: &mut Fb, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb565, alpha: f32) {
    let edgek = rx.min(ry) * 0.5; // ~1px AA band
    let x0 = (cx - rx - 1.0) as i32;
    let x1 = (cx + rx + 1.0) as i32;
    let y0 = (cy - ry - 1.0) as i32;
    let y1 = (cy + ry + 1.0) as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (x as f32 + 0.5 - cx) / rx;
            let dy = (y as f32 + 0.5 - cy) / ry;
            let d2 = dx * dx + dy * dy;
            let cov = clamp01((1.0 - d2) * edgek + 0.5);
            fb.blend(x, y, col, cov * alpha);
        }
    }
}

#[inline]
fn disc(fb: &mut Fb, cx: f32, cy: f32, r: f32, col: Rgb565, alpha: f32) {
    ellipse(fb, cx, cy, r, r, col, alpha);
}

/// AA filled triangle (2x2 supersample) — used for the little beak.
fn tri(fb: &mut Fb, p: [(f32, f32); 3], col: Rgb565) {
    let minx = (p[0].0.min(p[1].0).min(p[2].0) - 1.0) as i32;
    let maxx = (p[0].0.max(p[1].0).max(p[2].0) + 1.0) as i32;
    let miny = (p[0].1.min(p[1].1).min(p[2].1) - 1.0) as i32;
    let maxy = (p[0].1.max(p[1].1).max(p[2].1) + 1.0) as i32;
    let edge = |ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32| {
        (px - ax) * (by - ay) - (py - ay) * (bx - ax)
    };
    for y in miny..=maxy {
        for x in minx..=maxx {
            let mut cov = 0.0f32;
            for &sy in &[0.25f32, 0.75] {
                for &sx in &[0.25f32, 0.75] {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    let d0 = edge(p[0].0, p[0].1, p[1].0, p[1].1, px, py);
                    let d1 = edge(p[1].0, p[1].1, p[2].0, p[2].1, px, py);
                    let d2 = edge(p[2].0, p[2].1, p[0].0, p[0].1, px, py);
                    if (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0)
                        || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
                    {
                        cov += 0.25;
                    }
                }
            }
            fb.blend(x, y, col, cov);
        }
    }
}

/// Draw the Vibird character at animation time `t` (seconds).
fn draw_vibird(fb: &mut Fb, t: f32) {
    // palette
    let body = Rgb565::new(14, 47, 31);
    let cream = Rgb565::new(31, 61, 30);
    let eye = Rgb565::new(2, 6, 8);
    let beak = Rgb565::new(31, 42, 6);
    let cheek = Rgb565::new(31, 30, 21);

    let bob = sinf(t * 2.3) * 3.0;
    let cx = 64.0;
    let cy = 62.0 + bob;
    let breath = 1.0 + sinf(t * 2.3) * 0.02;

    // smooth blink: a quick dip every ~3 s
    let cyc = {
        let v = t * 0.33;
        v - libm::floorf(v)
    };
    let closed = if cyc < 0.07 {
        sinf(cyc / 0.07 * core::f32::consts::PI)
    } else {
        0.0
    };
    let ery = 9.0 * (1.0 - 0.9 * closed);

    // body (flat, clean — no outline, no shadow)
    ellipse(fb, cx, cy, 35.0, 39.0 * breath, body, 1.0);
    // belly / chest
    ellipse(fb, cx, cy + 17.0, 24.0, 22.0, cream, 1.0);
    // cheeks (soft blush)
    disc(fb, cx - 22.0, cy + 2.0, 6.5, cheek, 0.5);
    disc(fb, cx + 22.0, cy + 2.0, 6.5, cheek, 0.5);
    // eyes (glossy, with catchlight) — blink squashes ery
    ellipse(fb, cx - 14.0, cy - 8.0, 9.0, ery, eye, 1.0);
    ellipse(fb, cx + 14.0, cy - 8.0, 9.0, ery, eye, 1.0);
    if ery > 4.0 {
        disc(fb, cx - 17.0, cy - 11.0, 2.6, Rgb565::WHITE, 0.95);
        disc(fb, cx + 11.0, cy - 11.0, 2.6, Rgb565::WHITE, 0.95);
    }
    // little beak (downward triangle), just under the eyes
    tri(
        fb,
        [(cx, cy + 2.0), (cx - 5.0, cy - 4.0), (cx + 5.0, cy - 4.0)],
        beak,
    );
}

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 140 * 1024);
    let mut delay = Delay::new();

    // ---- Backlight: LP5562 on internal I2C (SDA=45, SCL=0) ----
    {
        let mut i2c = I2c::new(p.I2C0, I2cConfig::default())
            .unwrap()
            .with_sda(p.GPIO45)
            .with_scl(p.GPIO0);
        let _ = i2c.write(LP5562_ADDR, &[0x00, 0x40]);
        let _ = i2c.write(LP5562_ADDR, &[0x08, 0x01]);
        let _ = i2c.write(LP5562_ADDR, &[0x70, 0x00]);
        let _ = i2c.write(LP5562_ADDR, &[0x0E, 0xC0]);
        info!("backlight (LP5562) on");
    }

    // ---- Display GC9107 over SPI2 ----
    let spi = Spi::new(
        p.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(p.GPIO15)
    .with_mosi(p.GPIO21);
    let cs = Output::new(p.GPIO14, Level::High, OutputConfig::default());
    let dc = Output::new(p.GPIO42, Level::Low, OutputConfig::default());
    let rst = Output::new(p.GPIO48, Level::High, OutputConfig::default());
    let spi_dev = ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();
    let mut if_buf = alloc::vec![0u8; 4096];
    let di = SpiInterface::new(spi_dev, dc, &mut if_buf);
    let mut display = Builder::new(GC9107, di)
        .display_size(W as u16, H as u16)
        .display_offset(0, 32)
        .invert_colors(ColorInversion::Normal)
        .color_order(ColorOrder::Bgr) // this AtomS3R panel is BGR (red<->blue swap confirmed)
        .reset_pin(rst)
        .init(&mut delay)
        .unwrap();
    info!("display (GC9107) init ok");

    // ---- Button (GPIO41, active-low) = push-to-talk ----
    let button = Input::new(p.GPIO41, InputConfig::default().with_pull(Pull::Up));

    // ---- Base I2C (38/39): probe the Atomic Echo Base (ES8311 0x18 + PI4IOE 0x43) ----
    let mut base = I2c::new(p.I2C1, I2cConfig::default())
        .unwrap()
        .with_sda(p.GPIO38)
        .with_scl(p.GPIO39);
    let mut pb = [0u8; 1];
    let es8311_present = base.read(0x18u8, &mut pb).is_ok();
    let pi4ioe_present = base.read(0x43u8, &mut pb).is_ok();
    if es8311_present {
        info!("Echo Base ES8311 @0x18 present (pi4ioe@0x43={pi4ioe_present})");
    } else {
        warn!("Echo Base not detected (ES8311 0x18 no ACK) — mic capture needs it attached");
    }

    // ---- Precompute the gradient backdrop once (static); copy it each frame ----
    let bg_top = Rgb565::new(21, 42, 44);
    let bg_bot = Rgb565::new(15, 33, 40);
    let mut backdrop: Vec<Rgb565> = alloc::vec![Rgb565::BLACK; (W * H) as usize];
    for y in 0..H {
        let c = lerp_col(bg_top, bg_bot, y as f32 / H as f32);
        for x in 0..W {
            backdrop[(y * W + x) as usize] = c;
        }
    }

    // 主路径:播放内嵌表情包(区域刷新);解析失败则回退到抗锯齿矢量占位动画。
    match Pack::parse(PLACEHOLDER) {
        Ok(pack) => {
            let (cw, ch) = pack.canvas();
            info!(
                "emote pack: {} clips, canvas {}x{}",
                pack.clip_count(),
                cw,
                ch
            );
            let mut player = Player::new(pack);
            const STEP_MS: u32 = 5;
            let mut fps_t0 = Instant::now();
            let mut frames_n: u32 = 0;
            // 无网络时:每 2.5s 轮播展示全部表情;按住按键则切 listening。将来语音/SetState 替代。
            let mut clip_t0 = Instant::now();
            let mut was_held = false;
            loop {
                if let Some(frame) = player.tick(STEP_MS) {
                    // 只刷脏矩形 —— 区域刷新
                    for r in frame.rects() {
                        let x1 = r.x + r.w.saturating_sub(1);
                        let y1 = r.y + r.h.saturating_sub(1);
                        display
                            .set_pixels(r.x, r.y, x1, y1, r.pixels_rgb565().map(rgb565_from_raw))
                            .ok();
                    }
                    frames_n += 1;
                }
                // 按键 push-to-talk(GPIO41 低有效):按住 → listening 表情。
                // 将来此处采 Echo Base 麦克风(ES8311 + I2S)并经 WebSocket 上传给桥接。
                let held = button.is_low();
                if held && !was_held {
                    player.set_clip("listening");
                    info!("PTT down → listening (es8311_present={es8311_present})");
                } else if !held && was_held {
                    info!("PTT up");
                    clip_t0 = Instant::now();
                }
                was_held = held;
                if fps_t0.elapsed() >= Duration::from_millis(1000) {
                    info!(
                        "vibird emote: {} frames/s (btn={held} echo_base={es8311_present})",
                        frames_n
                    );
                    frames_n = 0;
                    fps_t0 = Instant::now();
                }
                if !held && clip_t0.elapsed() >= Duration::from_millis(2500) {
                    player.next_clip();
                    if let Some(c) = player.clip() {
                        info!("emote clip → {}", c.name());
                    }
                    clip_t0 = Instant::now();
                }
                delay.delay_millis(STEP_MS);
            }
        }
        Err(_) => {
            // 回退:抗锯齿矢量占位动画
            let mut fb = Fb::new();
            let start = Instant::now();
            let mut fps_t0 = Instant::now();
            let mut fps_n: u32 = 0;
            loop {
                let t = start.elapsed().as_millis() as f32 / 1000.0;
                fb.px.copy_from_slice(&backdrop);
                draw_vibird(&mut fb, t);
                display
                    .set_pixels(0, 0, W as u16 - 1, H as u16 - 1, fb.px.iter().copied())
                    .ok();
                fps_n += 1;
                if fps_t0.elapsed() >= Duration::from_millis(1000) {
                    info!("vibird: {} fps", fps_n);
                    fps_n = 0;
                    fps_t0 = Instant::now();
                }
            }
        }
    }
}
