---
doc_kind: finding
finding_id: gc9107-color-order
last_verified_commit: eb40d35
discovered_by: hardware bring-up — colour-bar test on the real AtomS3R
severity: P1
status: closed_by_eb40d35
related: [rust-animation-feasibility]
---

# Finding: the AtomS3R GC9107 panel is BGR with Normal inversion

## Hypothesis
The cream (near-white) belly rendering as **black**, while other colours looked "off", was a colour-mapping
error — either panel inversion or a channel/byte-order mismatch — not a drawing bug. Reasoning in circles
(flipping `ColorInversion` flipped *which* parts looked right) meant a definitive test was needed.

## Method
Flashed eight full-width solid colour bars, top→bottom, with `ColorInversion::Normal`:
`RED, GREEN, BLUE, WHITE, BLACK, YELLOW, CYAN, CREAM(belly)` and asked for the **actual** displayed colour
of each bar.

## Result (raw)
User reported, top→bottom: **蓝 绿 红 白 黑 青 黄 灰** (blue, green, red, white, black, cyan, yellow, gray).

Mapping intended → shown:
- RED → blue, BLUE → red (swapped)
- YELLOW(R+G) → cyan(G+B), CYAN(G+B) → yellow(R+G) (consistent with R↔B swap)
- GREEN → green (unchanged), WHITE → white, BLACK → black (unchanged)
- CREAM(near-white) → gray (≈ unchanged)

## Conclusion
**Red ↔ Blue are swapped ⇒ the panel is BGR, not RGB.** White and black unchanged ⇒ **no inversion is
needed (Normal, not Inverted).** The earlier guess `ColorInversion::Inverted` turned the brightest colour
(cream, ≈0xFFBE) into ≈black — that was the "black belly". With Normal but RGB, the near-white belly was
unaffected by the R↔B swap while the coloured body was wrong — which is exactly the confusing
"belly correct, body inverted" symptom.

**Fix (in `eb40d35`):** `.color_order(ColorOrder::Bgr)` + `.invert_colors(ColorInversion::Normal)` in the
mipidsi builder. (Some AtomS3R units ship ST7735S instead of GC9107 — if a unit shows nothing, switch the
model; colour-order finding still applies.)

## Cross-references
[ADR-0004](../adr/ADR-0004-on-device-rendering.md), [ADR-0002](../adr/ADR-0002-firmware-rust-esp-rs.md),
`docs/research/atoms3r-hardware.md`.
