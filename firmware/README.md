# Vibird firmware — AtomS3R reference client

Pure-Rust (`esp-hal`, `no_std`) firmware for the M5 **AtomS3R** (ESP32-S3).

**v0.1 is an animation spike** that de-risks the headline feature — smooth, pure-Rust animation on the
GC9107 display — before the full device logic (WiFi · WebSocket client · push-to-talk voice · status
animation · physical approval) is built out. See [`../docs/DESIGN.md`](../docs/DESIGN.md) and
[`ADR-0002`](../docs/adr/ADR-0002-firmware-rust-esp-rs.md).

## Build & flash

Prereqs: the esp-rs toolchain (via `espup`) and `espflash` (both installed by `espup install`).

```bash
. ~/export-esp.sh          # set up the Xtensa toolchain env (once per terminal)
cd firmware
cargo run --release        # build + flash + open the serial monitor (espflash is the cargo runner)
```

Plug the AtomS3R into USB-C first. To flash without monitoring:
`cargo build --release && espflash flash target/xtensa-esp32s3-none-elf/release/vibird-firmware`.

## What the spike does

Turns on the **LP5562** backlight → initializes the **GC9107** display → runs an off-screen
framebuffer animation (a bouncing, blinking blob) flushed every frame, logging **FPS** over USB serial.
It proves the backlight, display init, SPI throughput, and frame rate are all good in pure Rust. The
cute **Vibird** character art and the real device protocol land once this is verified on hardware.

## Pinout (source-verified)

Full table: [`../docs/research/atoms3r-hardware.md`](../docs/research/atoms3r-hardware.md). Summary:

| Function | Pins |
|---|---|
| Display GC9107 (SPI2) | SCLK **15**, MOSI **21**, CS **14**, DC **42**, RST **48** — 40 MHz, `offset_y = 32` |
| Backlight (LP5562) | internal I2C SDA **45** / SCL **0**, addr `0x30`, brightness = reg `0x0E` |
| Button (whole face) | GPIO **41** (active-low) |
| Echo Base audio (I2S) | BCLK **8**, WS **6**, DOUT **5**, DIN **7**; ES8311 @ `0x18` on I2C 38/39 |

## Gotchas

- **Display model:** some AtomS3R units ship an **ST7735S** instead of the GC9107. If the screen stays
  blank, switch the model in `main.rs` (`mipidsi::models::ST7735s`).
- **Color inversion:** the spike uses `ColorInversion::Inverted`; flip it if the colors look wrong.
- **Network for the toolchain:** this dev machine uses a transparent (TUN) proxy — keep `all_proxy` /
  `http_proxy` *unset* so `git`/`cargo`/`espup` reach github & crates.io directly.
