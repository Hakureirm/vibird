# Vibird — Design

> Language: **English** · [中文](../zh/design.md)

> **Vibird** is a zero-config, cross-agent **voice + status companion for vibe coding**.
> A tiny desktop companion you talk to: hold-to-speak your intent → it feeds your words to your
> AI coding agent (Claude Code first; Cursor / Codex next); it shows the agent's live state with
> high-refresh, expressive animation; you physically approve / deny risky actions.
> The on-device character is **Liz「栗子」**, a 2D-anime companion (see
> [ADR-0005](../../agent/adr/ADR-0005-character-liz.md)).

Status: **design draft** (2026-06). License: **AGPL-3.0 + commercial** dual license (see `LICENSE` / `README`).

---

## 1. Why Vibird — positioning

Market scan (2026-06): the obvious idea — a *"Claude desk pet + on-device approve/deny"* — is
**already covered by the platform** (`anthropics/claude-desktop-buddy`: ESP32 + BLE, sleeps/wakes/shows
impatience, on-device approve-deny, plus a commercial product and several community ports). So Vibird aims
elsewhere.

**Vibird deliberately targets the three real gaps the scan found empty:**

- **A — Voice in.** Every ESP32 voice assistant today talks to OpenAI / Gemini / Qwen; every Claude voice tool
  is software-only. **Nobody has made an ESP32 microphone a first-class dictation input for Claude Code.**
- **B — Cross-agent.** Cursor / Copilot / Codex have *no* hardware control at all.
- **C — Zero-config.** `pip install`, then **the agent reads a bundled skill and configures the
  device itself**. Config friction is the #1 killer of developer hardware; Vibird turns it into the headline
  feature. No competitor is doing this.

**Design laws** (non-negotiable):

1. **Narrow + reliable.** Do one thing — voice + status — extremely well. Don't try to replace anything.
2. **Passive-first.** A glance tells you the agent's state; active input (voice / button) is the bonus, not the burden.
3. **Zero-config by default.**
4. **Never brick.** Open protocol, OTA-able firmware, graceful offline degradation.
5. **Only make noise when it matters** (alert-fatigue discipline).
6. **Price band $49–120.**

**Vibird is NOT:** another Claude pet · a phone replacement · a macro pad you must reprogram.

---

## 2. Architecture

```
   Device (reference: M5 AtomS3R + Echo Base)        Host bridge / SDK (Rust)            AI agent
   ┌───────────────────────────────┐    WiFi /      ┌──────────────────────────┐  hooks  ┌────────────┐
   │ • hold-to-talk → 16k PCM ──────┼── WebSocket ──▶│ WS server (device dials in)│── MCP ─▶│ Claude Code│
   │ • high-refresh state animation │   (binary)     │ dual-engine ASR            │  tmux   │ (Cursor /  │
   │   (Liz emotes)                 │◀── state ──────│ Claude Code integration    │◀────────│  Codex →)  │
   │ • button + IMU input           │   push         │ mDNS-advertised            │ approve └────────────┘
   └───────────────────────────────┘                └──────────────────────────┘
   not bound to S3R: any device speaking          ships as: `cargo install` + `pip install` (PyO3/maturin)
   the Vibird protocol works                      + a one-command Claude Code plugin
```

Three layers:

- **Device** — a thin, expressive WebSocket **client**. Captures voice, shows agent state, reads button/IMU.
  S3R is the *reference* client, not a hard dependency.
- **Bridge / SDK (Rust — the product core)** — WebSocket **server** + ASR + agent integration. The reusable,
  cross-device, cross-agent, open + commercial core.
- **Agent** — Claude Code first, via hooks (status + physical approve/deny), MCP (device tools), and prompt
  injection (voice → text). Cursor / Codex adapters next.

### Key decisions (research-backed)

| Area | Decision | Why |
|---|---|---|
| Roles | **Device = WS client, Mac = WS server** | ESP32 WS *server* libs are buggy; *client* is mature; survives WiFi changes; simpler NAT/discovery. |
| Discovery | **mDNS** — Mac advertises `_vibird._tcp`; device queries once at boot (retried), then dials | Puts the error-prone query on the reliable side; zero-config on the LAN. |
| First config | **Serial JSON command → NVS**, network thereafter | Matches "network-first, serial only for first setup"; dodges AtomS3R native-USB-CDC Improv reset flakiness. |
| Voice | **Hold-to-talk, 16 kHz mono PCM, streamed while held**, ~30 ms frames; no device-side Opus/VAD | PTT gives perfect boundaries; LAN bandwidth is ample; keeps the device simple. |
| ASR (host) | **Dual engine:** `parakeet-mlx` (English, ~80 ms, default) + `mlx-whisper large-v3-turbo` (Chinese / code-switch). Cloud (Deepgram / gpt-4o) optional. | Parakeet is fastest+most accurate for English but **has no Chinese**; Whisper covers zh / mixed. |
| **Spoken intent, not syntax** | Vibird sends *what you said*; **Claude Code turns intent into code** | **No ASR emits code symbols or casing** (`()`, `->`, camelCase). The agent is the parser — that's its strength. |
| Prompt injection | **tmux send-keys** (v0.1) → Agent SDK / stream-json (later) | No official IPC into a live interactive session; tmux is the reliable path today. |
| Status + approve | **Claude Code hooks**: `Notification`/`Stop`/`PostToolUse` → push state; **`PreToolUse` → device returns `allow`/`deny`/`ask`** | Hooks are the stable, documented surface; PreToolUse is literally a physical permission gate. |
| Latency | ~250–450 ms release-to-text (local English Parakeet) | "Feels instant" threshold; stream-while-held + resident models. |

---

## 3. Character — Liz「栗子」

The device's face is **Liz「栗子」** — a **17-year-old 2D-anime girl** (full art bible in
[ADR-0005](../../agent/adr/ADR-0005-character-liz.md)):

- **Hair:** black, medium-long, straight (黑中长直).
- **Wardrobe:** loves **Lolita「女儿服」** and **JK sailor uniforms (水手服)**.
- **Framing:** **half-body (bust)** on the 128×128 screen.
- **Motion:** **telegram-sticker-smooth**, one emote per agent state plus transitions.

Positioning: everyone is building robots; Vibird's face goes the **2D-anime companion** route, not a
robot/pet. "Vibird" is the product/brand name; **Liz「栗子」is the mascot character**. The emote set maps 1:1
to the `AgentState` enum (idle / listening / thinking / working / awaiting / done / error).

---

## 4. Rendering & emote pipeline

The firmware is **pure Rust** (esp-hal, no_std; see
[ADR-0002](../../agent/adr/ADR-0002-firmware-rust-esp-rs.md),
[ADR-0004](../../agent/adr/ADR-0004-on-device-rendering.md)). We borrow the *technique* from Espressif's emote
stack (region-flush, pre-decoded RGB565 frames, named intro/loop/tail clips) but the **format and tooling are
ours**:

- **Vibird Emote Pack (`.veap`)** — our binary pack: header + manifest (named clips, layout, segment plans) +
  per-clip frames in **RGB565 with per-region delta** (store/flush only changed rectangles → ≈2× faster than
  decoding GIF, i.e. the "telegram-smooth" feel).
- **Host packer (Rust)** — `vibird-emote-pack`: GIF / PNG-sequence (Liz's art) → `.veap`. CLI first, then a
  wasm in-browser packer.
- **Firmware player (Rust, esp-hal no_std)** — mmap the pack from a flash partition, play a clip by name,
  flush only dirty rects to the GC9107. Reuses the confirmed `ColorOrder::Bgr` / `ColorInversion::Normal` /
  `offset_y = 32` ([finding](../../agent/findings/finding-gc9107-color-order.md), hardware-confirmed).
- The hand-coded anti-aliased vector renderer is demoted to a **no-assets fallback / boot animation**.

> Measured: 99 fps (pixel) / ~53 fps (vector)
> ([finding](../../agent/findings/finding-rust-animation-feasibility.md)) — the pure-Rust high-refresh risk is
> closed.

---

## 5. Tech stack

- **Host (Rust):** `tokio`, `tungstenite`/`axum` (WS), `rmcp` (MCP), whisper/parakeet bindings, the CLI, and
  the `vibird-emote-pack` packer. Shipped as a **pip wheel via PyO3 + maturin** *and* `cargo install`. The
  reusable core.
- **Firmware (Rust):** **esp-rs / `esp-hal`** (no_std, bare-metal, pure Rust) on ESP32-S3. `embedded-graphics`
  + a DMA double-buffered framebuffer + our `.veap` region-flush player; `esp-wifi` + a WS client; I2S +
  ES8311 codec; BMI270 IMU; RGB. **Not Zephyr** (see ADR-0002).
- **Distribution:** a Claude Code **plugin** (hooks + skills + root `.mcp.json`, one-command install) +
  `pip install vibird` (the bridge) + `cargo install vibird`.
- **License:** **AGPL-3.0** (community) + **commercial license** + **CLA**. All
  dependencies kept permissive (MIT/Apache/BSD) so commercial relicensing stays clean.
- **Build network:** crates.io / PyPI reachable directly via the router's transparent proxy (rsproxy /
  Tsinghua mirrors as fallback). GitHub reachable.

---

## 6. Roadmap

| Version | Deliverable |
|---|---|
| **v0.1 Voice loop** | S3R hold-to-talk → injected into Claude Code; basic idle/listening/thinking emotes. **De-risking spikes:** (1) pure-Rust animation on real S3R — ✅ **done** (99/53 fps); (2) esp-wifi WebSocket audio streaming reliability — ⏳ open. |
| **v0.2 Status + physical approve** | Hooks push agent state to the device; `PreToolUse` physical allow/deny. |
| **v0.3 Zero-config** | Claude Code plugin + pip package; the agent configures the device. **← the key differentiator.** |
| **v0.4 Cross-agent** | Cursor / Codex adapters. |
| **v0.5 Commercial-grade** | Liz emote polish, AGPL/CLA/docs site, OTA, offline degradation, packaging. |

---

## 7. Repository layout (monorepo)

```
vibird/
├── firmware/        # Rust (esp-hal) — S3R reference client + .veap player
├── host/            # Rust workspace — core / bridge / cli / mcp / emote packer  (the product core)
├── python/          # PyO3 + maturin wrapper → `pip install vibird`
├── claude-plugin/   # Claude Code plugin (hooks / skills / .mcp.json)
├── protocol/        # device ↔ bridge protocol spec (versioned)
├── docs/            # agent line (SNAPSHOT/ADRs/findings) + human line (bilingual)
└── assets/          # art, Liz emote source
```

---

## 8. Open risks — spike early

1. **esp-wifi reliability** for sustained WS audio streaming (fallback: `esp-idf-svc` / ESP-IDF WiFi, still Rust). **Open.**
2. ~~**60 fps pure-Rust animation** on the GC9107~~ — ✅ **resolved on hardware** (99/53 fps); the colour pipeline is fixed + confirmed too.
3. **Our own emote pipeline** — the `.veap` format + packer + region-flush player are all to be written from scratch (pure-Rust own-format chosen).
4. **Prompt-injection ergonomics** (tmux requirement in v0.1; cleaner SDK path later).
5. **Chinese / code-switch ASR** quality (dual engine; cloud fallback).
6. **Liz art production** (Live2D / commission / AI) — the next pending decision.

---

## 9. Research basis

Three parallel research tracks (Claude Code integration · market & feature mining · technical selection),
2026-06, with adversarial multi-source verification of load-bearing claims. Key sources: Claude Code docs
(hooks / mcp / plugins), `anthropics/claude-desktop-buddy`, Open ASR Leaderboard, `esp_websocket_client`
changelog, `parakeet-mlx`; the emote pipeline borrows the region-flush technique from Espressif's
`esp_emote_gen_player` (Apache-2.0). Source-verified hardware pinout:
[atoms3r-hardware](../../agent/atoms3r-hardware.md).
