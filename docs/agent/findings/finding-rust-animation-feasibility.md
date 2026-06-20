---
doc_kind: finding
finding_id: rust-animation-feasibility
last_verified_commit: eb40d35
discovered_by: firmware animation spike on the real AtomS3R
severity: P2
status: closed_by_eb40d35
related: [gc9107-color-order]
---

# Finding: pure-Rust high-FPS animation is feasible on the AtomS3R (headline risk de-risked)

## Hypothesis
A full-frame, double-buffered animation written in **pure Rust** (`esp-hal` + `embedded-graphics`, with no
LovyanGFX/M5Unified) can reach **≥ 60 fps** on the GC9107 — i.e. the project's "cute high-refresh" headline
does not require C++. ([ADR-0002](../adr/ADR-0002-firmware-rust-esp-rs.md) flagged this as the top risk.)

## Method
Firmware spike: an off-screen 128×128 Rgb565 framebuffer, redrawn each loop and flushed whole via mipidsi
`set_pixels`, with FPS logged over USB-serial-JTAG. Two render paths were measured on real hardware:
1. a pixel-blit sprite path;
2. a per-pixel coverage-AA vector path (f32 SDF math on the ESP32-S3 FPU).

## Result (raw)
- Pixel-blit path: **99 fps** sustained (`INFO - vibird: 99 fps`, repeated).
- AA-vector path: **~52–53 fps** sustained (`INFO - vibird: 53 fps`, repeated).

## Conclusion
The headline feature is feasible with margin: 99 fps (pixel) / 53 fps (smooth AA vector), both well above a
usable threshold. The earlier C++/LovyanGFX caution (ADR-0002) was over-conservative for a small 128×128
panel — a framebuffer + DMA SPI flush closes the gap in Rust. If a future expression needs more headroom,
the lever is **flushing only the character's bounding box** instead of the full screen.

## Cross-references
[ADR-0002](../adr/ADR-0002-firmware-rust-esp-rs.md), [ADR-0004](../adr/ADR-0004-on-device-rendering.md).
