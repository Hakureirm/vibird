---
doc_kind: adr
adr_id: 0004
title: "On-device rendering — designed-art frames over hand-coded shapes"
status: accepted
date: 2026-06-21
last_verified_commit: eb40d35
supersedes: []
superseded_by: []
---

# ADR-0004 — On-device rendering + art pipeline

## Context

The device's face must be a **cute, high-refresh** character on a 128×128 colour LCD (GC9107) — a headline
product requirement. The firmware spike proved pure-Rust animation is fast enough
([finding-rust-animation-feasibility](../findings/finding-rust-animation-feasibility.md)), but hand-authored
art quality was the sticking point: crude geometric shapes and chunky pixel-art both read as "ugly" and the
pixel style wastes a real colour LCD. (Also surfaced + fixed here: the panel is BGR + Normal inversion,
[finding-gc9107-color-order](../findings/finding-gc9107-color-order.md).)

## Options considered

1. **Hand-coded geometric primitives.** Rejected — too crude; reads as ugly; fake shading creates illusions.
2. **Chunky pixel-art sprites.** Rejected — wastes the colour LCD's resolution/depth; retro-blocky.
3. **Smooth anti-aliased vector (SDF coverage).** Good — clean, ~53 fps on hardware; **kept as the
   procedural fallback + the animation harness**, but hand-authoring a *charming character* from shapes is
   still hard.
4. **Character DESIGNED in an external tool, exported as frames (chosen).** Best art quality; decouples art
   from firmware; future-proofs per-agent-state expressions.

## Decision

The Vibird character is **designed in an external art tool** (EmoteLab — which exports PNG + GIF/APNG — or
any tool that exports raster frames; tool-agnostic), exported as frames, then:

```
designed frames (PNG/GIF) → host converter (PNG → RGB565 + transparency) → embedded const data
                          → firmware frame-blitter plays the frames per agent-state
```

The **smooth AA-vector renderer stays** as (a) a no-art-assets fallback and (b) the timing/animation harness
(bob, blink, state transitions). Panel config is fixed: GC9107, `ColorOrder::Bgr`, `ColorInversion::Normal`,
`offset_y = 32`.

Note: EmoteLab is Windows-only; if the user has no Windows host, any PNG/GIF exporter (Aseprite, etc.) feeds
the same pipeline.

## Consequences

### Positive
- Art quality is bounded by a real design tool, not by hand-coding shapes.
- Per-agent-state expressions (idle/listening/thinking/working/awaiting/done/error) become designed frames.
- 8 MB flash holds hundreds of 32 KB RGB565 frames.

### Negative / Risk
- The **PNG→RGB565 converter and the firmware frame-blitter are not built yet** (next deliverable).
- Transparency handling (alpha → composite over the device backdrop) needs the converter to be correct.
- Frame storage budget must be tracked as states/frames grow.

## Cross-references
- [finding-gc9107-color-order](../findings/finding-gc9107-color-order.md),
  [finding-rust-animation-feasibility](../findings/finding-rust-animation-feasibility.md),
  `docs/research/atoms3r-hardware.md`, [ADR-0002](ADR-0002-firmware-rust-esp-rs.md).
