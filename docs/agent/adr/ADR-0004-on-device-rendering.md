---
doc_kind: adr
adr_id: 0004
title: "On-device rendering — Vibird emote pack + region-flush Rust player (pure Rust)"
status: accepted
date: 2026-06-21
last_verified_commit: eb40d35
supersedes: []
superseded_by: []
---

# ADR-0004 — On-device rendering + emote pack pipeline

## Context

The device face is a cute, high-refresh **2D-anime character** (Liz「栗子」, see
[ADR-0005](ADR-0005-character-liz.md)) with **telegram-sticker-smooth** per-agent-state emotes on the
128×128 GC9107. The spike proved pure-Rust animation is fast (99/53 fps,
[finding-rust-animation-feasibility](../findings/finding-rust-animation-feasibility.md)) and the colour
pipeline is fixed + hardware-confirmed
([finding-gc9107-color-order](../findings/finding-gc9107-color-order.md)). Hand-authored shapes cannot
deliver "cute anime girl"; we need **designed art played from a resource pack**.

Espressif ships an emote stack (`esp_emote_gen_player`, Apache-2.0) + a browser GIF→`.bin` packer that does
exactly this, with **region-flush** (≈2× fps vs decoding a GIF every frame). But it is a **C / ESP-IDF**
stack (`esp_emote_gfx` + `esp_mmap_assets`) and adopting it would push the firmware off pure no_std Rust
([ADR-0002](ADR-0002-firmware-rust-esp-rs.md)).

## Options considered

1. **Adopt Espressif's C/ESP-IDF emote stack directly** (switch firmware to `esp-idf-hal`). Fastest +
   proven + would also resolve the esp-wifi risk — but abandons pure no_std Rust.
2. **Pure Rust, reimplement a player that consumes Espressif's `.bin`** (reverse-engineer their format).
   Keeps pure Rust, but chases a moving dev-tool format.
3. **Pure Rust, our OWN emote pack format + our OWN host packer (CHOSEN).** Full control, AGPL-clean, keeps
   pure no_std Rust. We write the packer (so we forgo Espressif's browser tool). The user blessed
   wheel-reinvention and chose this explicitly.
4. *(earlier draft of this ADR)* EmoteLab PNG→RGB565 naive frame-blitter — superseded: no region-flush
   (slower), no segment/loop model, not built around a designed character.

## Decision

**Pure Rust, end to end.** Adopt the *technique* from Espressif's stack (region-flush, pre-decoded RGB565
frames, named clips with intro/loop/tail segment plans) with **our own format and tooling**:

- **Vibird Emote Pack (`.veap`)** — our binary resource pack: header + manifest (named clips; layout
  x/y/w/h; segment plan: `stop_frame` / `loop_start` / `loop_end`) + per-clip frames in **RGB565 with
  per-region delta** (store and flush only changed rectangles → the 2× win + telegram-smoothness). The byte
  format is specified alongside the implementation (`docs/agent/emote-pack-format.md` + a follow-up ADR).
- **Host packer (Rust)** — `vibird-emote-pack`: GIF / PNG-sequence (Liz's art) → `.veap`. CLI first; compile
  to **wasm** later for an in-browser packer (our own equivalent of Espressif's tool).
- **Firmware player (Rust, esp-hal no_std)** — mmap the pack from a flash partition, play a clip by name
  (`idle` / `listening` / …), region-flush dirty rects to the GC9107. Reuses the spike framebuffer and the
  confirmed `ColorOrder::Bgr` / `ColorInversion::Normal` / `offset_y = 32` config.
- The hand-coded AA-vector renderer is demoted to a **no-assets fallback / boot animation** only.

Pure no_std Rust firmware ([ADR-0002](ADR-0002-firmware-rust-esp-rs.md)) is **preserved**. Unlike option 1,
this path does **not** resolve the esp-wifi reliability risk — that stays open and is tracked separately.

## Consequences

### Positive
- Pure Rust kept; format + tooling fully ours → clean AGPL + commercial story, no external dev-tool drift.
- Region-flush delta delivers the telegram-smooth target; designed-art quality via GIF source.
- Per-agent-state emotes map 1:1 to named clips (mirrors the `AgentState` protocol enum).

### Negative / Risk
- We build the packer **and** the player **and** define the format (real work; wheel-reinvention accepted).
- We forgo Espressif's ready browser tool + sample Emote Pack (their Apache-2.0 source remains a readable
  reference for the region-flush technique).
- esp-wifi reliability must still be de-risked on its own.
- Frame/delta storage budget tracked as Liz's emote set grows (8 MB flash).

## Cross-references
[ADR-0002](ADR-0002-firmware-rust-esp-rs.md) (pure-Rust firmware — preserved),
[ADR-0005](ADR-0005-character-liz.md) (Liz character),
[finding-rust-animation-feasibility](../findings/finding-rust-animation-feasibility.md),
[finding-gc9107-color-order](../findings/finding-gc9107-color-order.md),
[atoms3r-hardware](../atoms3r-hardware.md). Apache-2.0 reference: `esp_emote_gen_player` / `esp_emote_gfx`.
