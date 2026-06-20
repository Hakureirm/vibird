---
doc_kind: spec
last_verified_commit: eb40d35
---

# Vibird Emote Pack (`.veap`) — byte format v1

The on-device animation format ([ADR-0004](adr/ADR-0004-on-device-rendering.md)). Designed for **region-flush**
playback on a tiny MCU: the firmware keeps one canvas buffer, applies each frame's changed rectangles, and
flushes only those rectangles to the panel — ≈2× faster than decoding a GIF every frame, which is what makes
Liz's emotes telegram-smooth. Implemented by the `vibird-emote` crate (`emote/`): a no_std zero-copy parser
(firmware) + a std packer (host).

All integers are **little-endian**. Pixels are **RGB565** (the GC9107 BGR quirk is handled by the display
driver at flush time, so packs store ordinary RGB565).

## Header (16 bytes)

| off | type | field | notes |
|---|---|---|---|
| 0 | `[u8;4]` | magic | `"VEAP"` |
| 4 | u16 | version | `1` |
| 6 | u16 | flags | reserved, 0 |
| 8 | u16 | canvas_w | |
| 10 | u16 | canvas_h | |
| 12 | u16 | clip_count | |
| 14 | u16 | reserved | 0 |

## Clip table (`clip_count` entries × 48 bytes, right after the header)

| off | type | field | notes |
|---|---|---|---|
| 0 | `[u8;32]` | name | NUL-padded UTF-8 (logical name: `idle`, `listening`, …) |
| 32 | u16 | frame_count | |
| 34 | u16 | fps | playback rate |
| 36 | u16 | loop_start | frame index |
| 38 | u16 | loop_end | inclusive; `0xFFFF` = last frame |
| 40 | u16 | flags | bit0 = looping |
| 42 | u16 | reserved | 0 |
| 44 | u32 | data_offset | absolute byte offset to this clip's frame block |

## Frame block (per clip; `frame_count` frames concatenated)

Each frame:

| off | type | field |
|---|---|---|
| 0 | u16 | rect_count |
| 2 | u16 | reserved (0) |
| 4 | rects… | `rect_count` rectangles |

Each rectangle:

| off | type | field |
|---|---|---|
| 0 | u16 | x |
| 2 | u16 | y |
| 4 | u16 | w |
| 6 | u16 | h |
| 8 | `[u16; w*h]` | pixels (RGB565, row-major) |

## Semantics

- **Frame 0 of every clip is a full-canvas frame** (a single rect `0,0,canvas_w,canvas_h`) so the player can
  start from a clean state when a clip begins.
- **Frames 1..N carry only changed rectangles** (delta vs the previous frame in the same clip). A frame with
  `rect_count == 0` is a no-op (hold the current canvas).
- The player keeps a `canvas_w × canvas_h` RGB565 buffer: for each frame, copy every rect into the canvas at
  `(x,y)`, then flush each rect region to the panel. Clip switching restarts from the target clip's frame 0.
- **Segment plan:** play `0..frame_count`; if `looping`, loop `loop_start..=loop_end` afterwards. (Future:
  intro + loop + tail handoff, mirroring the agent-state transitions.)

## Notes / future

- v1 packer emits a single bounding-box delta rect per changed frame (`diff_bbox`). Multi-rect / row-span
  deltas are a later optimisation (smaller packs); the format already supports many rects per frame.
- The parser is panic-free on malformed input (bounds-checked reads → `Error` / truncated iteration), so the
  firmware can mmap an untrusted flash partition safely.
