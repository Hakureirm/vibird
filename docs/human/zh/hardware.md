# 硬件 —— M5 AtomS3R(参考机)

> 语言:[English](../en/hardware.md) · **中文**

Vibird 不绑定 AtomS3R,但它是第一台落地硬件。这是给人看的接线指南;密集、源码核实的参考见
[`../../agent/atoms3r-hardware.md`](../../agent/atoms3r-hardware.md)。

## 概览

- **MCU:** ESP32-S3(Xtensa 双核,WiFi/BLE),8 MB flash,原生 USB。
- **屏幕:** GC9107 128×128 彩色 LCD,SPI。*(部分批次是 ST7735S —— 如果黑屏就换驱动 model。)*
- **背光:** LP5562 LED 驱动,挂在内部 I2C 总线上(**不是** GPIO)。
- **IMU:** BMI270。**按键:** GPIO41。**音频(Atomic Echo Base 扩展):** ES8311 编解码 + NS4150B 功放,I2S。

## 屏幕(GC9107,SPI2)

| SCK | MOSI | DC | CS | RST |
|---|---|---|---|---|
| GPIO15 | GPIO21 | GPIO42 | GPIO14 | GPIO48 |

40 MHz,SPI mode 0,**y 偏移 32**。面板是 **BGR + Normal 反色** —— 见
[finding-gc9107-color-order](../../agent/findings/finding-gc9107-color-order.md)。

## 背光(LP5562)

内部 I2C,地址 **0x30** —— SDA **GPIO45**,SCL **GPIO0**。使能芯片并驱动白色通道(见固件 init 序列)。

## IMU / 按键

- BMI270,I2C 地址 **0x69**。
- 按键在 **GPIO41**。

## 音频(Atomic Echo Base,I2S)

- 编解码 ES8311(**0x18**)+ IO 扩展 PI4IOE(**0x43**),I2C —— SDA **GPIO38**,SCL **GPIO39**。
- I2S:BCLK / LRCLK / DIN / DOUT 在 **GPIO8 / 6 / 5 / 7**。

## 烧录

原生 USB-CDC。用 **espflash stub 烧录器**(默认)—— **不要**加 `--no-stub`(这块板会报错)。`firmware/` 下
`cargo run --release` 一条龙构建 + 烧录 + 监视。
