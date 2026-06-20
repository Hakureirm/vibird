---
doc_kind: adr
adr_id: 0003
title: "Device ↔ bridge protocol — WebSocket client, mDNS discovery, serial first-config"
status: accepted
date: 2026-06-21
last_verified_commit: eb40d35
supersedes: []
superseded_by: []
---

# ADR-0003 — Device ↔ bridge protocol

## Context

The device must talk to the host bridge **over the network** (the user's explicit constraint: network for
flexibility, serial only for first-time setup). The transport must reliably stream microphone audio and
push agent-state down to the device, on an ESP32-S3 whose WebSocket libraries are uneven.

## Options considered

1. **Device as WebSocket server.** Rejected — every ESP32 WS *server* library path is the buggy one
   (crashes on new clients, silent frame drops, long-run stalls; see
   [atoms3r-hardware](../atoms3r-hardware.md) and the tech-selection research).
2. **Device as WebSocket client + mDNS discovery (chosen).** ESP32 WS *client* is mature; the device
   survives WiFi changes and dials in; NAT/discovery is simpler.
3. **MQTT broker / cloud relay.** Rejected for v1 — adds an external dependency (and the "GFW: home can't
   reach cloud" reality), heavier than a LAN WebSocket.

## Decision

- **Roles:** the **device is the WebSocket client**, the **host bridge is the server**. The device
  discovers the bridge via **mDNS** (`_vibird._tcp`, one retried query at boot), then dials in.
- **Wire format:** **JSON text frames** for control (`Uplink` / `Downlink` in the `vibird-protocol` crate)
  + **raw binary frames** for microphone audio (16 kHz mono PCM, little-endian `i16`), framed by
  `AudioStart` … binary … `AudioEnd`.
- **Voice:** **push-to-talk** — stream PCM while the button is held; no device-side VAD or compression.
- **First-time config:** newline-delimited JSON `SerialCommand`s over USB serial (`wifi`, `bridge`,
  `status`, `factory`), stored in NVS; everything after is network.
- **Single source of truth:** the message types live once in `vibird-protocol` (`no_std + serde`) and
  compile into both the firmware and the host. Spec: `protocol/PROTOCOL.md`. Version: `PROTOCOL_VERSION`.

## Consequences

### Positive
- Robust transport (the mature client path); auto-discovery; one shared type definition (no drift).
- Matches the "network-first, serial-only-first-config" constraint exactly.

### Negative / Risk
- mDNS on the ESP32 is the one error-prone step (mitigated: query at boot with retries, bridge advertises).
- Binary-audio + JSON-control multiplexing on one socket needs care (documented in PROTOCOL.md).

## Cross-references
- `protocol/PROTOCOL.md`, `protocol/src/lib.rs` (`vibird-protocol`), [ADR-0001](ADR-0001-positioning.md),
  [atoms3r-hardware](../atoms3r-hardware.md).
