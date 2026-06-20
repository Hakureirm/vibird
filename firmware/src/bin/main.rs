//! Vibird firmware — animation spike (v0.1 de-risk).
//!
//! Goal: prove we can drive the AtomS3R's GC9107 display from pure Rust (esp-hal) at a smooth frame
//! rate, with the LP5562 backlight, before committing the full firmware. Draws a bouncing + blinking
//! blob into an off-screen framebuffer and flushes the whole frame each loop, logging FPS over USB.
//! (The cute "Vibird" character art comes once the pipeline is verified on hardware.)
//!
//! Pins (source-verified — see docs/research/atoms3r-hardware.md):
//!   Display GC9107 (SPI2): SCLK=15, MOSI=21, CS=14, DC=42, RST=48; 40 MHz; panel 128x128, offset_y=32.
//!   Backlight: LP5562 LED driver on internal I2C (SDA=45, SCL=0), addr 0x30, brightness = reg 0x0E.

#![no_std]
#![no_main]

extern crate alloc;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    main,
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::{Duration, Instant, Rate},
};
use log::info;
use mipidsi::{interface::SpiInterface, models::GC9107, options::ColorInversion, Builder};

esp_bootloader_esp_idf::esp_app_desc!();

const W: u16 = 128;
const H: u16 = 128;
const LP5562_ADDR: u8 = 0x30;

/// Off-screen 128x128 Rgb565 framebuffer for tear-free, fast full-frame updates.
struct FrameBuf {
    px: alloc::vec::Vec<Rgb565>,
}
impl FrameBuf {
    fn new() -> Self {
        Self { px: alloc::vec![Rgb565::BLACK; W as usize * H as usize] }
    }
    fn clear(&mut self, c: Rgb565) {
        self.px.iter_mut().for_each(|p| *p = c);
    }
}
impl DrawTarget for FrameBuf {
    type Color = Rgb565;
    type Error = core::convert::Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, pixels: I) -> Result<(), Self::Error> {
        for Pixel(pt, c) in pixels {
            if (0..W as i32).contains(&pt.x) && (0..H as i32).contains(&pt.y) {
                self.px[pt.y as usize * W as usize + pt.x as usize] = c;
            }
        }
        Ok(())
    }
}
impl OriginDimensions for FrameBuf {
    fn size(&self) -> Size {
        Size::new(W as u32, H as u32)
    }
}

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 110 * 1024);
    let mut delay = Delay::new();

    // ---- Backlight: LP5562 on internal I2C (SDA=45, SCL=0) ----
    {
        let mut i2c = I2c::new(p.I2C0, I2cConfig::default())
            .unwrap()
            .with_sda(p.GPIO45)
            .with_scl(p.GPIO0);
        let _ = i2c.write(LP5562_ADDR, &[0x00, 0x40]); // ENABLE
        let _ = i2c.write(LP5562_ADDR, &[0x08, 0x01]); // CONFIG: internal clock
        let _ = i2c.write(LP5562_ADDR, &[0x70, 0x00]); // LED map
        let _ = i2c.write(LP5562_ADDR, &[0x0E, 0xC0]); // B-channel PWM ~= 75% brightness
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
        .display_size(W, H)
        .display_offset(0, 32) // GC9107 is natively 128x160; AtomS3R panel offset by 32
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(rst)
        .init(&mut delay)
        .unwrap();
    info!("display (GC9107) init ok");

    // ---- Animation pipeline spike ----
    let mut fb = FrameBuf::new();
    let bg = Rgb565::new(2, 4, 8);
    let body = Rgb565::new(10, 38, 31);
    let belly = Rgb565::new(22, 52, 46);
    // smooth-ish bob (no float sin in no_std)
    const BOB: [i32; 16] = [0, 1, 2, 3, 3, 3, 2, 1, 0, -1, -2, -3, -3, -3, -2, -1];

    let mut frame: u32 = 0;
    let mut fps_t0 = Instant::now();
    let mut fps_n: u32 = 0;

    loop {
        let cx = 64i32;
        let cy = 66i32 + BOB[(frame / 3 % 16) as usize];
        let blink = frame % 140 < 8;

        fb.clear(bg);
        let r = 30i32;
        Circle::new(Point::new(cx - r, cy - r), 2 * r as u32)
            .into_styled(PrimitiveStyle::with_fill(body))
            .draw(&mut fb)
            .ok();
        let rb = 17i32;
        Circle::new(Point::new(cx - rb, cy - rb + 10), 2 * rb as u32)
            .into_styled(PrimitiveStyle::with_fill(belly))
            .draw(&mut fb)
            .ok();
        for ex in [cx - 11, cx + 11] {
            Circle::new(Point::new(ex - 6, cy - 12), 12)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
                .draw(&mut fb)
                .ok();
            if !blink {
                Circle::new(Point::new(ex - 3, cy - 9), 6)
                    .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                    .draw(&mut fb)
                    .ok();
            }
        }

        display.set_pixels(0, 0, W - 1, H - 1, fb.px.iter().copied()).ok();

        frame += 1;
        fps_n += 1;
        if fps_t0.elapsed() >= Duration::from_millis(1000) {
            info!("animation: {} fps", fps_n);
            fps_n = 0;
            fps_t0 = Instant::now();
        }
    }
}
