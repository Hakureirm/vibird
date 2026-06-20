# Hardware — M5 AtomS3R (reference device)

> Language: **English** · [中文](../zh/hardware.md)

Vibird isn't bound to the AtomS3R, but it's the first landing hardware. This is the human-readable wiring
guide; the dense, source-verified reference is [`../../agent/atoms3r-hardware.md`](../../agent/atoms3r-hardware.md).

## At a glance

- **MCU:** ESP32-S3 (Xtensa dual-core, WiFi/BLE), 8 MB flash, native USB.
- **Display:** GC9107 128×128 colour LCD over SPI. *(Some units ship ST7735S — swap the driver model if the
  screen stays blank.)*
- **Backlight:** LP5562 LED driver on an internal I2C bus (it is **not** a GPIO).
- **IMU:** BMI270. **Button:** GPIO41. **Audio (Atomic Echo Base add-on):** ES8311 codec + NS4150B amp over I2S.

## Display (GC9107, SPI2)

| SCK | MOSI | DC | CS | RST |
|---|---|---|---|---|
| GPIO15 | GPIO21 | GPIO42 | GPIO14 | GPIO48 |

40 MHz, SPI mode 0, **y-offset 32**. The panel is **BGR + Normal inversion** — see
[finding-gc9107-color-order](../../agent/findings/finding-gc9107-color-order.md).

## Backlight (LP5562)

Internal I2C, address **0x30** — SDA **GPIO45**, SCL **GPIO0**. Enable the chip and drive the white channel
(see the firmware init sequence).

## IMU / button

- BMI270 on I2C, address **0x69**.
- Button on **GPIO41**.

## Audio (Atomic Echo Base, I2S)

- Codec ES8311 (**0x18**) + IO expander PI4IOE (**0x43**) on I2C — SDA **GPIO38**, SCL **GPIO39**.
- I2S: BCLK / LRCLK / DIN / DOUT on **GPIO8 / 6 / 5 / 7**.

## Flashing

Native USB-CDC. Use the **espflash stub flasher** (the default) — do **not** pass `--no-stub` (it errors on
this board). `cargo run --release` in `firmware/` handles build + flash + monitor.
