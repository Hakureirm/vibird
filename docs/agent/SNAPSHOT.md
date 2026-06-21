---
doc_kind: snapshot
last_verified_commit: eb40d35
schema_invariant: |
  Every ADR mentioned anywhere in this file appears in the ADR roster table.
  Every finding mentioned appears in the Findings ledger table.
  Docs live under docs/agent/ (agent line) + docs/human/{zh,en}/ (human line); README + docs/human are
  projections of this file.
  Roadmap status reflects the latest commit, not intentions.
---

# Vibird — Snapshot (canonical state)

**Source of truth** for the project's current state. `README.md` and the human docs under
[`../human/`](../human/) are projections — if they disagree, this file wins until they are synced.

## Wedge (one sentence)

Vibird is a **zero-config, cross-agent voice + status companion for vibe coding**: speak intent to a cute
desk companion (the mascot **Liz「栗子」**) that feeds your AI coding agent (Claude Code first), shows the
agent's live state with expressive animation, and takes physical approvals — aimed at the three gaps the
desk-pet space leaves open (**Claude-native voice · cross-agent · zero-config**). Full rationale:
[ADR-0001](adr/ADR-0001-positioning.md).

## Repo state

- **HEAD:** `f7a8613` on `main`, **public at https://github.com/Hakureirm/vibird** (AGPL; pushed over SSH).
  The `feat/firmware-wifi` line (Gates 1–5 + the embassy/WiFi/mic firmware) was fast-forward-merged into
  `main` on 2026-06-21. ~27 commits.
- **Firmware-wifi commits:** `bb111b6` Gate 1 (embassy + esp-radio/esp-rtos/embassy-net + ws.rs) · `87f8382`
  Gates 1–4 HW-verified · `a16ce4d` Gate 5 mic uplink (ES8311+I2S → WS PCM) · `2a3e1c2` mute ES8311 DAC ·
  `3ea4db7` full PI4IOE speaker-PA shutdown · `f7a8613` docs sync.
- **Builds:** `vibird-protocol` (2 tests ✓) · `host/` (5-gate ✓) · `firmware/` (xtensa release ✓, **the full
  WiFi+mic firmware flashed + Gates 1–5 verified on the real AtomS3R**).

## What works (verified on hardware)

- Firmware on the AtomS3R drives LP5562 backlight, GC9107 display (SPI2), a full-frame animation loop.
- **Frame rate:** 99 fps (pixel) / ~53 fps (AA vector) on the real panel
  ([finding-rust-animation-feasibility](findings/finding-rust-animation-feasibility.md)).
- **Colours: fixed AND user-confirmed on hardware (2026-06-21)** — the panel is BGR + Normal inversion
  ([finding-gc9107-color-order](findings/finding-gc9107-color-order.md)).
- Current device render: the firmware drives the **.veap `Player`** (region-flush, dirty-rect only) and
  cycles all 7 agent-state placeholder emotes (distinct colour + motion), switching every 2.5 s via
  `Player::next_clip`. The AA-vector renderer is the no-pack fallback.
- *(Tried + dropped, 2026-06-21:* a BMI270 gravity "auto-upright" that counter-rotated the whole frame — the
  full-frame rotate cost too much fps for too little value, so region-flush was kept. The BMI270 **does**
  work and sits at I2C **0x68** on this unit (chip `0x24`), noted in `atoms3r-hardware.md` if ever revisited.)

## Voice loop — closed end-to-end (full software chain, verified on Mac 2026-06-21)

The whole **"speak → ASR → inject into Claude Code"** loop is **closed and verified on the Mac**, zero hardware:

- `vibird simulate <wav>` impersonates a device: connects to the bridge → streams a WAV's PCM (push-to-talk)
  → receives state downlinks.
- The bridge runs **local ASR** (`--asr local` → shells out to `scripts/asr_local.py` = mlx-whisper, fully
  local, no network, no cloud).
- **E2E run:** Mac `say "list all the python files in this folder"` → `afconvert` 16 kHz mono → `simulate`
  streams it → bridge logs `ASR → "List all the python files in this folder."` → the text lands via `tmux`
  injection. State sequence `Idle → Listening → Thinking → Working` drives the device face. In a real Claude
  Code session that sentence *is* the prompt.
- Gap: only the **device firmware** side of mic capture (ES8311 + I2S + WiFi/WS) is unbuilt; `simulate`
  stands in for it for software testing.

## Decisions just locked (2026-06-21)

- **Rendering architecture** ([ADR-0004](adr/ADR-0004-on-device-rendering.md)): **pure-Rust, our own emote
  pack format (`.veap`) + host packer + region-flush player** (Option C). Firmware stays no_std esp-hal pure
  Rust (ADR-0002 preserved). We do *not* use Espressif's C stack / browser tool, and this path does *not*
  resolve the esp-wifi risk.
- **Character** ([ADR-0005](adr/ADR-0005-character-liz.md)): the mascot is **Liz「栗子」** — a
  2D-anime girl (黑中长直; Lolita 女儿服 + JK 水手服), half-body, telegram-smooth emotes. "Vibird" stays the
  product name.

## In flight / not built

- **Emote pipeline** (ADR-0004): **complete** ✓ — the `.veap` format + `vibird-emote` crate (parser +
  `Player` + packer-lib, 6 tests) + the `vibird-emote-pack` CLI (GIF→.veap, e2e-verified) + the **firmware
  region-flush player** (embeds `assets/placeholder.veap`). **HW-verified on the real AtomS3R (2026-06-21):
  it cycles all 7 agent-state placeholder emotes at ~18 fps via region-flush** (`Player::next_clip` every
  2.5 s; serial: `emote clip → listening …`). Remaining: real **Liz art** (Live2D).
- **Liz art** (ADR-0005): not produced; **the production approach (Live2D / commission / AI) is the next
  decision.**
- **Host bridge** (`host/`): **voice loop + status display built (host side), and the software loop is now
  E2E-closed (see above)** ✓ — WS server + audio framing + pluggable ASR (stub / cloud Whisper / **local
  mlx-whisper**) + tmux injection (`vibird serve --tmux … --asr …`) + **`vibird simulate <wav>`** (device
  impersonator for HW-free self-test); plus a local **control plane** (TCP) + `vibird hook` that pushes
  agent-state to the device face from Claude Code hooks; the **MCP server** (`vibird mcp`, hand-rolled stdio
  JSON-RPC) and the **Claude Code plugin** (`claude-plugin/`: hooks + MCP + zero-config skill) are built ✓.
  The **`pip install` scaffold** (`python/`, PyO3 + maturin, `vibird.serve(...)`) builds ✓ too.
- **Device-side network firmware (branch `feat/firmware-wifi`):** the firmware was **migrated from a blocking
  loop to embassy async** and now carries the full on-device stack — **WiFi STA (`esp-radio` 0.18, the
  esp-hal-1.1 successor to `esp-wifi`) + `esp-rtos` scheduler + `embassy-net` + a hand-rolled no_std
  WebSocket client (`ws.rs`) + the shared `vibird-protocol` types via no_std `serde_json`**. It connects to
  the bridge, sends `Hello`, and maps `SetState` downlinks → `.veap` emote clips. **Gates 1–4 HW-verified on
  the real AtomS3R (2026-06-21):** boots under embassy (~18 fps emotes), joins WiFi `Firefly-2.4G` (DHCP
  `192.168.127.73`), the **hand-rolled WS client handshakes with the bridge's tokio-tungstenite + sends
  `Hello`** (bridge logs `hello from atoms3r-vibird`), and **every pushed `SetState`
  (idle/listening/thinking/working/awaiting_approval/done/error) drives the device face** — the
  status-display half of the loop is **closed on hardware**. WiFi creds inject at build time
  (`VIBIRD_WIFI_SSID/PASS` via `config.rs`); the **bridge address auto-discovers via mDNS** (`_vibird._tcp`,
  hand-rolled no_std query over embassy-net UDP — `VIBIRD_BRIDGE_ADDR` optional, HW-verified 2026-06-21).
  **Gate 5 also HW-verified (2026-06-21):** PTT button
  → **ES8311 mic (analog) + I2S RX (Data32, one-shot DMA into a static DMA buffer) → mono-i16 16 kHz PCM →
  WS binary upload → the bridge's mlx-whisper transcribes it recognizably → tmux inject**. The **full hardware
  voice loop is closed** (speak → device → ASR → inject). Notes: the Echo Base speaker PA (NS4150B via the
  PI4IOE5V6408 @0x43) is **muted** for this record-only device; audio quality has room to refine (the
  one-shot per-chunk DMA leaves small gaps — a circular path would be gap-free). **With Gates 1–5 the AtomS3R
  is a complete Vibird device** (voice in + status face), end-to-end on real hardware.
- **Physical-approval pillar (the 3rd pillar) built (2026-06-21):** the **BMI270 IMU** (`bmi2` 0.1.2, I2C0
  @0x68 — shares the bus with the LP5562 backlight, which writes once then hands the bus to the IMU) is
  initialised on-device (8 KB config blob upload OK, gyro @100 Hz ±500 dps). A no_std integer gesture
  detector (`gesture.rs`, two-reversal state machine) maps **nod → Approval(Allow) / shake → Approval(Deny)**
  when an `AwaitingApproval` is pending, else `Uplink::Gesture`. **BMI270 init HW-verified; the gesture
  axis/threshold still needs a physical nod/shake calibration pass** (nod guessed as gyro-Y, shake as gyro-Z).
- Still TODO: serial `config`/`service` CLI (so WiFi creds aren't build-time either — the last config gap).

## Documentation map

- **Agent line** (`docs/agent/`, dense, English): [SNAPSHOT.md](SNAPSHOT.md) · [adr/](adr/) ·
  [findings/](findings/) · [atoms3r-hardware.md](atoms3r-hardware.md) ·
  [emote-pack-format.md](emote-pack-format.md).
- **Human line** (`docs/human/{zh,en}/`, narrative, bilingual): `design` · `getting-started` · `hardware`.
  **Status: the bilingual human line is complete ✓ (design / getting-started / hardware / index, en + zh).**
- **Landing:** `README.md` (EN) ✓ + `README.zh-CN.md` (中文) ✓.

## ADR roster

| ADR | Title | Status |
|---|---|---|
| [0001](adr/ADR-0001-positioning.md) | Positioning & wedge — voice / cross-agent / zero-config | accepted |
| [0002](adr/ADR-0002-firmware-rust-esp-rs.md) | Firmware pure Rust on esp-hal, not Zephyr / C++ | accepted |
| [0003](adr/ADR-0003-device-bridge-protocol.md) | Device ↔ bridge protocol (WS client, mDNS, serial config) | accepted |
| [0004](adr/ADR-0004-on-device-rendering.md) | Rendering — pure-Rust emote pack + region-flush player | accepted |
| [0005](adr/ADR-0005-character-liz.md) | Character & art direction — Liz「栗子」 | accepted |

## Findings ledger

| id | title | severity | status |
|---|---|---|---|
| [gc9107-color-order](findings/finding-gc9107-color-order.md) | GC9107 panel is BGR + Normal inversion | P1 | closed (`eb40d35`), HW-confirmed |
| [rust-animation-feasibility](findings/finding-rust-animation-feasibility.md) | pure-Rust ≥60 fps feasible on the S3R | P2 (positive) | closed (`eb40d35`) |

## Roadmap status (vs design §4)

- **v0.1 voice loop:** **closed end-to-end on REAL HARDWARE (2026-06-21)** — the AtomS3R does PTT → ES8311
  mic → I2S → WiFi/WS → mlx-whisper ASR → tmux inject (Gate 5), plus the status-display downlink driving the
  face (Gates 1–4). The Mac-only `simulate` software loop also passes. **mDNS auto-discovery of the bridge is
  done (2026-06-21).** Refinements left: audio quality (gap-free DMA), serial `config` provisioning (WiFi
  creds still build-time), real Liz art.
- **Spikes:** animation ✓, colours ✓ (HW-confirmed). **esp-wifi WS streaming spike — open** (ADR-0004
  Option C does not resolve it).

## Open items / next

1. **Decide Liz's art-production approach** (Live2D / commission / AI) — unblocks the whole character track.
2. Build the emote pipeline: `.veap` format spec → `vibird-emote-pack` packer → firmware region-flush player.
3. ~~Bilingual human docs~~ — ✅ done (design / getting-started / hardware / index, en + zh).
4. ~~Host v0.1 voice loop (ASR + tmux injection)~~ — ✅ **software loop closed + E2E-tested on Mac** (`simulate` + local mlx-whisper + tmux inject); **device-side audio capture** (firmware WiFi/WS/I2S) is the remaining half.
5. **esp-wifi WS streaming** spike.
6. ~~github: install `gh`, then push~~ — ✅ published 2026-06-21 → github.com/Hakureirm/vibird.

## Build / flash quickref

- host: `cd host && cargo run -- serve`
- **voice loop self-test (no hardware):** term A — `VIBIRD_ASR_SCRIPT=scripts/asr_local.py vibird serve --asr local --tmux <sess>`;
  term B — `say "list the python files" -o /tmp/c.aiff && afconvert /tmp/c.aiff -f WAVE -d LEI16@16000 /tmp/c.wav && vibird simulate /tmp/c.wav`.
- firmware: `. ~/export-esp.sh && cd firmware && cargo run --release` (builds + flashes via espflash)
- network: keep `all_proxy`/`http_proxy` **unset** (router transparent proxy; explicit proxy double-wraps
  and fails). crates.io / github / raw.githubusercontent.com reach directly.
