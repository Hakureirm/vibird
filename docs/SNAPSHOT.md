---
doc_kind: snapshot
last_verified_commit: eb40d35
schema_invariant: |
  Every ADR mentioned anywhere in this file appears in the ADR roster table.
  Every finding mentioned appears in the Findings ledger table.
  The HEAD field reconciles with `git log -1 --format=%h`.
  Roadmap status reflects the latest commit, not intentions.
---

# Vibird — Snapshot (canonical state)

This is the **source of truth** for the project's current state. `README.md`, `docs/DESIGN.md`, and the
firmware docs are projections — if they disagree with this file, this file wins until they are synced.

## Wedge (one sentence)

Vibird is a **zero-config, cross-agent voice + status companion for vibe coding**: speak intent to a cute
desk creature that feeds your AI coding agent (Claude Code first), shows the agent's live state with
expressive animation, and takes physical approvals — aimed at the three gaps the desk-pet space leaves
open (**Claude-native voice · cross-agent · zero-config**). Full rationale: [ADR-0001](adr/ADR-0001-positioning.md).

## Repo state

- **Verified at:** `eb40d35` (the firmware code state this snapshot describes); the snapshot itself ships in
  the immediately-following `docs:` commit. Branch `main`, **local only** (github reachable via the
  transparent proxy, but `gh` is not installed and a push has not been requested yet).
- **Commits:** `fadc439` foundation · `3589e8e` host skeleton · `ee95b5f` firmware spike · `eb40d35`
  firmware vector render + BGR fix.
- **Builds:** `vibird-protocol` (2 tests ✓) · `host/` workspace (`cargo check` ✓) · `firmware/`
  (xtensa release build ✓, flashed to real AtomS3R).

## What works (verified on hardware)

- Firmware on the AtomS3R drives: LP5562 backlight, GC9107 display (SPI2), a full-frame animation loop.
- **Frame rate:** pixel-blit path **99 fps**; anti-aliased vector path **~53 fps** — both measured on the
  real panel. ([finding-rust-animation-feasibility](findings/finding-rust-animation-feasibility.md))
- **Colours fixed:** the panel is **BGR + Normal inversion**; the "black belly" was the brightest colour
  mis-mapped. ([finding-gc9107-color-order](findings/finding-gc9107-color-order.md))
- Current device render: a smooth AA-vector placeholder bird (breathing + blinking).

## In flight / decided-but-not-built

- **Art-pipeline pivot** ([ADR-0004](adr/ADR-0004-on-device-rendering.md)): the character will be **designed
  in an external tool** (EmoteLab or any PNG/GIF exporter), exported as frames, converted PNG→RGB565 by a
  host tool, embedded, and played per agent-state by a firmware frame-blitter. **Not built yet**; pending
  the user's art-tool choice. The AA-vector renderer stays as the procedural fallback + animation harness.
- **Host bridge** is a WebSocket-server skeleton only. ASR, prompt injection, mDNS advertise, MCP server,
  and the Claude Code hook handlers are **not built**.

## ADR roster

| ADR | Title | Status |
|---|---|---|
| [0001](adr/ADR-0001-positioning.md) | Positioning & wedge — voice / cross-agent / zero-config | accepted |
| [0002](adr/ADR-0002-firmware-rust-esp-rs.md) | Firmware in Rust on esp-rs (esp-hal), not Zephyr / C++ | accepted |
| [0003](adr/ADR-0003-device-bridge-protocol.md) | Device ↔ bridge protocol (WS client, mDNS, serial config) | accepted |
| [0004](adr/ADR-0004-on-device-rendering.md) | On-device rendering + designed-art pipeline | accepted |

## Findings ledger

| id | title | severity | status |
|---|---|---|---|
| [gc9107-color-order](findings/finding-gc9107-color-order.md) | AtomS3R GC9107 panel is BGR + Normal inversion | P1 | closed (`eb40d35`) |
| [rust-animation-feasibility](findings/finding-rust-animation-feasibility.md) | pure-Rust ≥60 fps animation feasible on the S3R | P2 (positive) | closed (`eb40d35`) |

## Roadmap status (vs DESIGN §4)

- **v0.1 voice loop:** not started. The firmware **animation spike** (the headline de-risk) is done; the
  host voice loop (ASR + injection) is pending.
- **Spikes:** animation ✓ (99/53 fps), colours ✓. The **esp-wifi WebSocket audio-streaming** spike (the
  other open risk in ADR-0002) is **not done**.

## Open items / next

1. Visually confirm the BGR colour fix on hardware (flashed; awaiting the user's eye).
2. Decide the art tool → build the **PNG→RGB565 converter + firmware frame-blitter** (ADR-0004).
3. Host **v0.1 voice loop**: ASR engine + prompt injection (tmux) wired to the bridge.
4. **esp-wifi WS streaming** spike (the second firmware risk).
5. github: install `gh`, then push (deferred; net reachable via transparent proxy).
6. Pending user decisions: repo language (English assumed), bilingual README?

## Build / flash quickref

- host: `cd host && cargo run -- serve`
- firmware: `. ~/export-esp.sh && cd firmware && cargo run --release` (builds + flashes via espflash)
- network on this machine: keep `all_proxy`/`http_proxy` **unset** (router transparent proxy; explicit
  proxy double-wraps and fails). crates.io / github / raw.githubusercontent.com reach directly.
