# Vibird Protocol (v1)

Device ↔ host-bridge wire protocol. The message types are the [`vibird-protocol`](./src/lib.rs) crate —
shared `no_std` Rust compiled into **both** the firmware and the host, so there is one source of truth.

## Transport
- **WebSocket.** The **device is the client**, the **bridge is the server** (the device discovers the
  bridge via mDNS `_vibird._tcp`, then dials in). Rationale in [`../docs/DESIGN.md`](../docs/DESIGN.md).
- **Control** messages = **JSON text frames** (`Uplink` / `Downlink`).
- **Audio** = **raw binary frames**: 16 kHz mono PCM, little-endian `i16`, ~30 ms per frame, streamed
  between `AudioStart` and `AudioEnd`.

## Handshake
1. Device connects → `Uplink::Hello { device_id, fw_version, protocol, caps }`.
2. Bridge → `Downlink::Welcome { protocol, bridge_version }`.

A protocol-version mismatch lets the bridge refuse or downgrade.

## Voice — push-to-talk
```
button Press  →  Uplink::AudioStart { sample_rate, format }
              →  [binary PCM frames …]            (streamed while held)
button Release→  Uplink::AudioEnd
              →  bridge runs ASR, injects the transcribed text into the agent
```
No device-side VAD or compression — push-to-talk gives exact boundaries and the LAN has ample bandwidth.

## Status & approval
- As the agent's state changes (driven by Claude Code hooks), the bridge pushes
  `Downlink::SetState(AgentState)` and the device animates accordingly (`idle / listening / thinking /
  working / awaiting_approval / done / error`).
- A risky tool call → bridge sends `Downlink::Approval { request_id, summary, detail, timeout_ms }` →
  device grabs attention and shows it → user presses → `Uplink::Approval { request_id, Allow | Deny }` →
  the bridge returns that decision from the `PreToolUse` hook (`allow` / `deny`).

## First-time provisioning — USB serial
Newline-delimited JSON `SerialCommand`s (the firmware also listens here at boot):
- `{"cmd":"wifi","ssid":"…","pass":"…"}` — stored in NVS
- `{"cmd":"bridge","host":"auto","port":0}` — `auto` = discover via mDNS; or pin host/port
- `{"cmd":"status"}` · `{"cmd":"factory"}`

After provisioning, everything is over the network; serial is only the bootstrap.

## Versioning
`PROTOCOL_VERSION` (currently **1**), bumped on any breaking change to the message shapes.
