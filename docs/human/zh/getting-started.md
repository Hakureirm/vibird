# 快速上手(从源码构建)

> 语言:[English](../en/getting-started.md) · **中文**

> **预览期(pre-alpha)。** 还没有发布版二进制,需从源码构建。当前能跑的:host bridge 骨架 + AtomS3R 上的
> 固件**动画**。语音闭环(v0.1)开发中;Liz 表情包管线尚未实现。精确状态见
> [`../../agent/SNAPSHOT.md`](../../agent/SNAPSHOT.md)。

## 前置

- **Rust**(stable),用 `rustup` —— host 端。
- **esp 工具链**,用 `espup`(Xtensa ESP32-S3);每个 shell 先 `source`:`. ~/export-esp.sh` —— 固件端。
- **espflash** —— 烧录。
- 一台 **M5 AtomS3R**(参考机)—— 仅固件需要。
- **网络:** 在路由器透明代理下,保持 `all_proxy` / `http_proxy` **未设置**(显式代理会双重包裹而失败);
  之后 crates.io / github 直连。

## Host bridge

```bash
cd host
cargo run -- serve      # WebSocket bridge 监听 :8137,mDNS 广播为 _vibird._tcp
```

## 固件(AtomS3R)

```bash
. ~/export-esp.sh
cd firmware
cargo run --release     # 构建 xtensa-esp32s3,经 espflash 烧录,再串口监视
```

屏幕(128×128)上应出现动画占位。(先构建再烧录 —— 否则构建失败时会把旧的二进制重新烧上去。)

## 接下来

- 设计与路线图:[design.md](design.md)。
- 硬件引脚:[hardware.md](hardware.md)。
