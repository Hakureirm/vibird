# 快速上手(从源码构建)

> 语言:[English](../en/getting-started.md) · **中文**

> **预览期(pre-alpha)。** 还没有发布版二进制,需从源码构建。**完整产品环已在真机闭合(2026-06-21):**
> AtomS3R 按键说话 → ES8311 麦克风 → I2S → WiFi/WebSocket → mlx-whisper 转写 → 注入 Claude Code;反向
> Claude Code 状态 → bridge → 设备表情脸(7 态)。外加 Mac 纯软件替身环(见下「免硬件语音自测」)。剩下的是
> 打磨:mDNS 自动发现、串口配网(免编译期填凭据)、Liz 真人设美术。精确状态见
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

## 免硬件语音自测(端到端)

没有 AtomS3R 也能把整条语音闭环跑通 —— 用 Mac 自己发声当音源,`vibird simulate` 当一台虚拟设备:

```bash
# 一次性:装本地 ASR(mlx-whisper,Apple 芯片快)
pip install mlx-whisper

# 终端 A(在 host/ 下):起 bridge,用本地 mlx-whisper 转写,注入到名为 dev 的 tmux 会话
tmux new -d -s dev
cd host
VIBIRD_ASR_SCRIPT=../scripts/asr_local.py cargo run -- serve --asr local --tmux dev

# 终端 B(在 host/ 下):Mac 合成一句话 → 转 16kHz 单声道 WAV → 模拟设备推给 bridge
say "list all the python files in this folder" -o /tmp/cmd.aiff
afconvert /tmp/cmd.aiff -f WAVE -d LEI16@16000 /tmp/cmd.wav
cd host && cargo run -- simulate /tmp/cmd.wav
```

bridge 日志会打印 `ASR → "..."`,转写文本随即被注入 tmux `dev` 会话 —— 把 `dev` 换成正在跑 Claude Code 的
会话,这句话就是你的 prompt。设备侧状态序列是 `Idle → Listening → Thinking → Working`,正是表情切换的驱动。

## 接下来

- 设计与路线图:[design.md](design.md)。
- 硬件引脚:[hardware.md](hardware.md)。
