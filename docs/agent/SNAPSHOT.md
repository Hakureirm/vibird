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

- **Verified at:** `eb40d35` (firmware code state). Docs evolve on top in `docs:` commits. Branch `main`,
  **local only** (`gh` not installed; no push requested yet — github reachable via the transparent proxy).
- **Commits:** `fadc439` foundation · `3589e8e` host skeleton · `ee95b5f` firmware spike · `eb40d35`
  firmware vector + BGR fix · `fc08b82` docs (snapshot/ADRs/findings) · + the current restructure commit.
- **Builds:** `vibird-protocol` (2 tests ✓) · `host/` (`cargo check` ✓) · `firmware/` (xtensa build ✓,
  flashed to real AtomS3R).

## What works (verified on hardware)

- Firmware on the AtomS3R drives LP5562 backlight, GC9107 display (SPI2), a full-frame animation loop.
- **Frame rate:** 99 fps (pixel) / ~53 fps (AA vector) on the real panel
  ([finding-rust-animation-feasibility](findings/finding-rust-animation-feasibility.md)).
- **Colours: fixed AND user-confirmed on hardware (2026-06-21)** — the panel is BGR + Normal inversion
  ([finding-gc9107-color-order](findings/finding-gc9107-color-order.md)).
- Current device render: the firmware drives the **.veap `Player`** (region-flush) playing an embedded
  procedural placeholder (breathing dot); the AA-vector renderer is the no-pack fallback. Built for xtensa;
  not flashed this iteration (device offline).

## Decisions just locked (2026-06-21)

- **Rendering architecture** ([ADR-0004](adr/ADR-0004-on-device-rendering.md)): **pure-Rust, our own emote
  pack format (`.veap`) + host packer + region-flush player** (Option C). Firmware stays no_std esp-hal pure
  Rust (ADR-0002 preserved). We do *not* use Espressif's C stack / browser tool, and this path does *not*
  resolve the esp-wifi risk.
- **Character** ([ADR-0005](adr/ADR-0005-character-liz.md)): the mascot is **Liz「栗子」** — a 17-y/o
  2D-anime girl (黑中长直; Lolita 女儿服 + JK 水手服), half-body, telegram-smooth emotes. "Vibird" stays the
  product name.

## In flight / not built

- **Emote pipeline** (ADR-0004): **complete** ✓ — the `.veap` format + `vibird-emote` crate (parser +
  `Player` + packer-lib, 6 tests) + the `vibird-emote-pack` CLI (GIF→.veap, e2e-verified) + the **firmware
  region-flush player** (embeds `assets/placeholder.veap`, builds for xtensa). Remaining: on-device flash
  (device was offline) + real **Liz art** (Live2D).
- **Liz art** (ADR-0005): not produced; **the production approach (Live2D / commission / AI) is the next
  decision.**
- **Host bridge** (`host/`): **voice loop + status display built (host side)** ✓ — WS server + audio
  framing + pluggable ASR (stub / cloud Whisper) + tmux injection (`vibird serve --tmux … --asr …`); plus a
  local **control plane** (TCP) + `vibird hook` that pushes agent-state to the device face from Claude Code
  hooks. Still TODO: mDNS advertise, MCP server, the Claude plugin manifest, and **device-side** audio
  capture (firmware WiFi/WS/I2S) for true end-to-end.

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

- **v0.1 voice loop:** not started. Firmware animation spike done; host voice loop pending.
- **Spikes:** animation ✓, colours ✓ (HW-confirmed). **esp-wifi WS streaming spike — open** (ADR-0004
  Option C does not resolve it).

## Open items / next

1. **Decide Liz's art-production approach** (Live2D / commission / AI) — unblocks the whole character track.
2. Build the emote pipeline: `.veap` format spec → `vibird-emote-pack` packer → firmware region-flush player.
3. ~~Bilingual human docs~~ — ✅ done (design / getting-started / hardware / index, en + zh).
4. ~~Host v0.1 voice loop (ASR + tmux injection)~~ — ✅ host side done; **device-side audio capture** (firmware WiFi/WS/I2S) is the remaining half.
5. **esp-wifi WS streaming** spike.
6. github: install `gh`, then push.

## Build / flash quickref

- host: `cd host && cargo run -- serve`
- firmware: `. ~/export-esp.sh && cd firmware && cargo run --release` (builds + flashes via espflash)
- network: keep `all_proxy`/`http_proxy` **unset** (router transparent proxy; explicit proxy double-wraps
  and fails). crates.io / github / raw.githubusercontent.com reach directly.
