---
doc_kind: adr
adr_id: 0002
title: "Firmware in Rust on esp-rs (esp-hal), not Zephyr or C++"
status: accepted
date: 2026-06-21
last_verified_commit: eb40d35
supersedes: []
superseded_by: []
---

# ADR-0002 — Firmware in Rust on esp-rs (esp-hal), not Zephyr or C++

- **Status:** accepted
- **Date:** 2026-06-21
- **Context source:** technical-selection research track (2026-06)
- **Update (`eb40d35`):** the two spikes this ADR gated are partly resolved — the **60 fps animation**
  risk is closed (99/53 fps, [finding-rust-animation-feasibility](../findings/finding-rust-animation-feasibility.md));
  the **esp-wifi WS-streaming** risk is still open. Panel colour quirk found + fixed
  ([finding-gc9107-color-order](../findings/finding-gc9107-color-order.md)).

## Context

The reference device is the M5 AtomS3R (ESP32-S3) + Atomic Echo Base. The firmware must drive a 128×128
GC9107 display with a **cute, high-refresh (target 60 fps) animation** (a headline product requirement), an
I2S audio path (ES8311 mic → host over WiFi), a WebSocket client, an IMU, button, and RGB. The project
prefers **Rust everywhere** and accepts building drivers from scratch. The user asked specifically whether
the firmware can be **fully Rust**, and whether **Zephyr (device tree)** is a good base.

Candidate firmware bases:
- **C++ (Arduino + M5Unified + LovyanGFX)** — proven 60 fps animation (we built exactly this for a prior
  project), but not Rust.
- **esp-rs / `esp-hal`** (no_std, bare-metal, pure Rust) — Espressif-backed Rust-on-ESP32.
- **esp-rs / `esp-idf-svc`** (std, Rust app on the C ESP-IDF) — mature WiFi/WS/I2S, still 100% Rust app code.
- **Zephyr RTOS + `zephyr-lang-rust`** (device tree) — elegant, declarative hardware model.

## Decision

**Firmware is written in Rust on `esp-hal` (no_std).** Not Zephyr. Not C++.

Validate two hard bets with early spikes before building the full firmware:
1. **60 fps pure-Rust animation** on the real GC9107 (embedded-graphics + a DMA double-buffered framebuffer).
2. **`esp-wifi` reliability** for sustained WebSocket audio streaming.

If `esp-wifi` cannot hold the audio stream, fall back **for the networking layer only** to `esp-idf-svc`
(ESP-IDF's proven WiFi) — still all-Rust application code.

## Rationale

- **Fully Rust is achievable.** esp-rs is the Espressif-supported Rust path; the Xtensa toolchain installs
  cleanly via `espup`.
- **The animation is feasible in Rust.** 128×128 × 16-bit = 32 KB/frame; a full DMA flush at ~40 MHz SPI is
  ~6 ms → ample headroom for 60 fps with double buffering. The earlier worry (no LovyanGFX in Rust) matters
  for large displays; on a small 128×128 panel a framebuffer + DMA closes the gap.
- **Why not Zephyr (for *this* combo):**
  1. `zephyr-lang-rust` is young and smaller than esp-rs.
  2. ESP32-S3 WiFi on Zephyr is less proven than `esp-wifi` / ESP-IDF — and WiFi is our **core transport**.
  3. High-fps graphics on Zephyr means FFI to **LVGL (C)** — so it would not be pure Rust anyway.
  4. Zephyr-Rust + Zephyr-ESP32-WiFi + LVGL-FFI stacks three bleeding-edge layers — too much integration
     risk for a product that must feel polished.
  Device tree is genuinely elegant, but `esp-hal`'s typed peripherals give the same "declarative, safe
  hardware" benefit in pure Rust without Zephyr's integration risk. (Zephyr-Rust is compelling on STM32/nRF
  with mature ports and no exotic WiFi — not here.)
- **Why not C++:** it would deliver the animation fastest, but the project's Rust preference plus the user's
  willingness to build drivers makes pure Rust worth it. The firmware is only the *reference client*; the
  protocol and host SDK (the reusable, commercial core) are Rust regardless, so the firmware language does
  not dilute the Rust story.

## Consequences

- We build/port drivers in Rust: GC9107 display (mipidsi model or custom init), ES8311 codec, BMI270 IMU,
  the animation engine. Accepted (wheel-building is in scope).
- esp-hal/esp-wifi is less batteried than ESP-IDF; the `esp-idf-svc` fallback is the safety net for WiFi.
- Two spikes gate the firmware plan; if both pass, full Rust firmware proceeds.
