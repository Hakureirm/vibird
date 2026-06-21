# AtomS3R + Atomic Echo Base — hardware reference (source-verified)

All pins verified against M5Stack source (`M5GFX`, `M5Unified`, `M5Atomic-EchoBase`), not guessed.
The firmware (`firmware/`) targets exactly this. Confirm the ⚠️ items on real hardware before trusting them.

## Display — GC9107, 0.85" 128×128, SPI
Source: `M5GFX/src/M5GFX.cpp` 2374–2437.

| Signal | GPIO | Note |
|---|---|---|
| SCLK | **15** | |
| MOSI / SDA | **21** | 3-wire SPI (`spi_3wire = true`) |
| DC | **42** | |
| CS | **14** | also the panel-id read pin |
| RST | **48** | |
| Backlight | — | **not a GPIO** → LP5562 (below) |

SPI bus `SPI3_HOST`, mode 0, write **40 MHz** / read 16 MHz. Panel: 128×128, **`offset_y = 32`**
(GC9107 is natively 128×160), `readable = false`.

## Backlight — LP5562 LED driver (★ not a GPIO)
Source: `M5GFX/src/M5GFX.cpp` 583–601. On the **internal I2C** bus (SDA **45**, SCL **0**), addr **0x30**.
Init then set brightness:
```
reg 0x00 = 0x40   # ENABLE (chip enable)
reg 0x08 = 0x01   # CONFIG (internal clock)
reg 0x70 = 0x00   # LED map
reg 0x0E = <0..255>  # B-channel PWM = LCD brightness
```

## I2C buses (two separate ones — don't mix)
| Bus | SDA | SCL | Devices |
|---|---|---|---|
| **Internal** | **45** | **0** | BMI270 IMU (**0x68** — verified on-device 2026-06-21; M5 docs say 0x69 but this unit ACKs 0x68, so probe both), LP5562 backlight (0x30) |
| **Base / Grove** (Echo Base) | **38** | **39** | ES8311 codec (**0x18**), PI4IOE5V6408 IO-expander (**0x43**) |

## Audio — Atomic Echo Base (ES8311 + NS4150B)
Source: `M5Atomic-EchoBase`. I2S, 16-bit, full-duplex, master:

| Signal | GPIO |
|---|---|
| BCLK | **8** |
| WS / LRCK | **6** |
| DOUT → speaker | **5** |
| DIN ← mic | **7** |
| MCLK | none (derived from SCK) |

ES8311 configured over the base I2C (0x18). **PA enable / mute is via the PI4IOE expander (0x43)**:
init direction `0x6F`, then `setMute` writes output reg `0x05` (`0x00` mute / `0xFF` unmute). Must
`pi4ioe_init()` before any sound.

## Button / RGB / reset
- **Button** (the whole front face = one button): **GPIO41**, active-low.
- **No onboard RGB LED** on AtomS3R (it's all screen; RGB exists only on AtomS3U/Lite).
- Reset: hold the reset key ~2 s for download mode; standard ESP32-S3 strapping otherwise.

## Rust driver feasibility
- **Display:** `mipidsi` 0.10 has `models::GC9107` and its init sequence matches M5GFX **byte-for-byte**.
  Use `Builder::new(GC9107, di).display_size(128, 128).display_offset(0, 32)`. ⚠️ Some AtomS3R units ship
  **ST7735S** instead (panel-id `0x7683/0x897C` vs `0x079100`); read panel-id (cmd `0x04`) or try GC9107
  then ST7735s.
- **esp-hal SPI + DMA:** feasible for 60 fps. 128×128×16-bit = 32 KB/frame ≈ 1.9 MB/s @ 60 fps; 40 MHz SPI
  has ample headroom. Use `Spi::…with_dma(…).with_buffers(rx, tx)` → `SpiDmaBus` (a `SpiDevice` for mipidsi).
  Known extra-copy inefficiency (esp-hal #3125) — acceptable at 32 KB/frame; **keep DMA buffers in internal
  SRAM**, the framebuffer may live in PSRAM.
- **Backlight:** write the 4 LP5562 registers over an esp-hal I2C on (45, 0).

## Confirm on hardware (⚠️)
1. `i2cdetect` the internal bus (45/0) → expect `0x30` + `0x69`; the base bus (38/39) → `0x18` + `0x43`.
2. Display controller: read panel-id, or try GC9107 then ST7735s.

## Dev-machine network note
This machine's Clash runs as a **TUN transparent proxy** (DNS → fake-IP 198.18.x). For `git`/`curl`,
**`unset all_proxy http_proxy https_proxy`** (system LibreSSL double-proxies through 127.0.0.1:7890 and
fails otherwise). With them unset, github / crates.io / raw.githubusercontent.com are directly reachable.

---
*Source: research track 3 (2026-06), cross-checked against cloned M5Stack repos + `almindor/mipidsi`.*
