---
doc_kind: adr
adr_id: 0001
title: "Positioning & wedge — voice / cross-agent / zero-config, not another desk pet"
status: accepted
date: 2026-06-21
last_verified_commit: eb40d35
supersedes: []
superseded_by: []
---

# ADR-0001 — Positioning & wedge

## Context

A 2026-06 market scan (key sources cited below) found the obvious idea — a *Claude desk pet with
on-device approve/deny* — is **already officially owned by Anthropic** (`anthropics/claude-desktop-buddy`:
ESP32 + BLE; sleeps/wakes/shows impatience; on-device approve-deny) with a commercial product and 6+
community ports. Cloning it means a head-on collision with the platform owner. The scan also found three
**empty** gaps and a clear set of "winner vs e-waste" rules for developer hardware.

## Options considered

1. **Clone the Claude desk-pet + on-device approve/deny.** Rejected — Anthropic owns it; the space is dense
   and platform-blessed; differentiation is near-zero → e-waste risk.
2. **A voice-dictation-only tool (no hardware, or a generic mic puck).** Partial — useful, but the software
   space is crowded (superwhisper, Wispr Flow, …) and a generic puck has no defensible wedge.
3. **A zero-config, cross-agent voice + status companion (chosen).** Hits all three empty gaps at once.

## Decision

Build Vibird as **"the zero-config, cross-agent voice + status controller for vibe coding."** It targets
the three gaps the scan found empty:

- **A — Voice in.** Every ESP32 voice assistant today talks to OpenAI/Gemini/Qwen; every Claude voice tool
  is software-only. Nobody has made an ESP32 mic a first-class dictation input for Claude Code.
- **B — Cross-agent.** Cursor / Copilot / Codex have *no* hardware control at all.
- **C — Zero-config (the soul).** `pip install`, then the agent reads a bundled skill and configures the
  device itself. No competitor does this.

The cute high-refresh animation is the device's **face** (ambient agent-state display, embodied by the
mascot Liz「栗子」, see [ADR-0005](ADR-0005-character-liz.md)) — it makes the status legible; it is not
"another pet".

**Design laws** (from the winners-vs-e-waste analysis, non-negotiable): narrow + reliable · passive-first ·
zero-config · never brick (open protocol, OTA, offline degrade) · only make noise when it matters ·
price band $49–120.

## Consequences

### Positive
- Defensible wedge in three directions a competitor can't trivially copy.
- Each gap maps to a concrete deliverable (voice loop, cross-agent adapters, the install plugin).

### Negative / Risk
- Three gaps = more surface than a single-feature gadget; must stage tightly (the roadmap in the design doc).
- "Cross-agent" depends on each agent's integration surface (Claude Code is strongest; others vary).

## Cross-references
- [full design](../../human/en/design.md), [atoms3r-hardware](../atoms3r-hardware.md) (hardware reference),
  [ADR-0003](ADR-0003-device-bridge-protocol.md), [ADR-0004](ADR-0004-on-device-rendering.md),
  [ADR-0005](ADR-0005-character-liz.md).
