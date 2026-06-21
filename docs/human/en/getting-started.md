# Getting started (build from source)

> Language: **English** · [中文](../zh/getting-started.md)

> **Pre-alpha.** There's no released binary yet — you build from source. What works today: the full host
> **"speak → ASR → inject into Claude Code" loop is closed and verified end-to-end on the Mac** (see
> "Voice self-test without hardware" below) + the firmware **animation** on the AtomS3R (the `.veap` emote
> pipeline is done and HW-verified). What's left: the device-firmware mic capture (WiFi/WS/I2S) and the real
> Liz artwork. See [`../../agent/SNAPSHOT.md`](../../agent/SNAPSHOT.md) for the precise state.

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

## Voice self-test without hardware (end-to-end)

You can exercise the whole voice loop without an AtomS3R — let the Mac speak as the audio source and have
`vibird simulate` stand in for a device:

```bash
# One-time: install the local ASR (mlx-whisper, fast on Apple silicon)
pip install mlx-whisper

# Terminal A (from host/): run the bridge with local mlx-whisper, injecting into a tmux session "dev"
tmux new -d -s dev
cd host
VIBIRD_ASR_SCRIPT=../scripts/asr_local.py cargo run -- serve --asr local --tmux dev

# Terminal B (from host/): synthesize a sentence → 16 kHz mono WAV → stream it as a simulated device
say "list all the python files in this folder" -o /tmp/cmd.aiff
afconvert /tmp/cmd.aiff -f WAVE -d LEI16@16000 /tmp/cmd.wav
cd host && cargo run -- simulate /tmp/cmd.wav
```

The bridge logs `ASR → "..."` and injects the transcript into the `dev` tmux session — point it at a real
Claude Code session and that sentence becomes your prompt. The device-side state sequence is
`Idle → Listening → Thinking → Working`, which is exactly what drives the face animation.

## Where to go next

- The design + roadmap: [design.md](design.md).
- The hardware pinout: [hardware.md](hardware.md).
