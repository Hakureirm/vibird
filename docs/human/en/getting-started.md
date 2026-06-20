# Getting started (build from source)

> Language: **English** · [中文](../zh/getting-started.md)

> **Pre-alpha.** There's no released binary yet — you build from source. What works today: the host bridge
> skeleton and the firmware **animation** on the AtomS3R. The voice loop (v0.1) is in progress; the Liz
> emote-pack pipeline is not built yet. See [`../../agent/SNAPSHOT.md`](../../agent/SNAPSHOT.md) for the
> precise state.

## Prerequisites

- **Rust** (stable) via `rustup` — for the host.
- **esp toolchain** via `espup` (Xtensa ESP32-S3); source the env each shell: `. ~/export-esp.sh` — for firmware.
- **espflash** — for flashing.
- An **M5 AtomS3R** (the reference device) — only needed for firmware.
- **Network:** behind a router transparent proxy, keep `all_proxy` / `http_proxy` **unset** (an explicit
  proxy double-wraps and fails); crates.io / github then reach directly.

## Host bridge

```bash
cd host
cargo run -- serve      # WebSocket bridge on :8137, advertised over mDNS as _vibird._tcp
```

## Firmware (AtomS3R)

```bash
. ~/export-esp.sh
cd firmware
cargo run --release     # builds for xtensa-esp32s3, flashes via espflash, then monitors
```

You should see the animated placeholder on the 128×128 screen. (Build first, then flash — a failed build
otherwise re-flashes the stale binary.)

## Where to go next

- The design + roadmap: [design.md](design.md).
- The hardware pinout: [hardware.md](hardware.md).
