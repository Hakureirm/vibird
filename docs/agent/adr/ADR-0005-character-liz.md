---
doc_kind: adr
adr_id: 0005
title: "Character & art direction — Liz 栗子, a 2D-anime companion"
status: accepted
date: 2026-06-21
last_verified_commit: eb40d35
supersedes: []
superseded_by: []
---

# ADR-0005 — Character & art direction: Liz「栗子」

## Context

Everyone is building *robots*. Vibird's device face should differentiate as a **2D-anime companion
(二次元伙伴)**, not a robot or a generic pet. The animation must feel **telegram-sticker smooth**
(region-flush emotes, [ADR-0004](ADR-0004-on-device-rendering.md)). The character is the product's emotional
surface — it carries the agent-state display (idle / listening / thinking / working / awaiting-approval /
done / error).

## Options considered

1. **Geometric / pixel / abstract mascot** (e.g. the placeholder bird). Rejected — not the 二次元 vision;
   reads as toy/robot.
2. **A generic anime mascot.** Weak, forgettable identity.
3. **A specific, named, well-defined character (CHOSEN).** Gives a memorable identity and a consistent art
   bible for the whole emote set.

## Decision

The mascot is **Liz「栗子」** — a **2D-anime girl**:

- **Hair:** black, medium-long, straight (黑中长直).
- **Wardrobe:** loves **Lolita「女儿服」** and **JK sailor uniforms (水手服)**.
- **Framing:** **half-body (bust)** on the 128×128 screen.
- **Motion:** telegram-sticker-smooth, expressive emotes — one per agent state, plus transitions.

**"Vibird" remains the product / brand name; Liz「栗子」is the mascot character.** (Revisit if a product
rename is wanted.)

The emote set maps 1:1 to the `AgentState` enum in `vibird-protocol`: a designed clip per state, authored as
GIFs and packed via [ADR-0004](ADR-0004-on-device-rendering.md)'s pipeline.

## Consequences

### Positive
- Strong, memorable identity distinct from the robot crowd; a clear art bible for consistent emotes.
- Clean 1:1 mapping agent-state → designed emote.

### Negative / Risk
- Needs real character art + animation. **Production approach is TBD** — Live2D rig / commissioned artist /
  AI-assisted; the "telegram-smooth" bar favours Live2D or a skilled artist. (The next decision.)
- A defined minor character + outfits → keep art **SFW and tasteful**; brand / marketing implications.
- Until Liz art exists, the firmware shows the AA-vector placeholder.

## Cross-references
[ADR-0004](ADR-0004-on-device-rendering.md) (rendering / pipeline), `docs/human/en/design.md`,
`README.md`, `protocol/src/lib.rs` (`AgentState`).
