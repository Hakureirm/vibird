# Vibird — 设计

> 语言:[English](../en/design.md) · **中文**

> **Vibird** 是一个零配置、跨 Agent 的 **vibe coding 语音 + 状态陪伴设备**。
> 一只你可以对话的桌面小伙伴 —— 长按说出你的意图 → 它把你的话喂给 AI 编码 Agent(先做 Claude Code,
> 之后 Cursor / Codex);它用高刷新、有表情的动画显示 Agent 的实时状态;危险操作你按一下物理确认 / 拒绝。
> 设备上的角色是 **Liz「栗子」**(二次元半身萌妹子,见 [ADR-0005](../../agent/adr/ADR-0005-character-liz.md))。

状态:**设计草案**(2026-06)。许可:**AGPL-3.0 + 商用双授权**(见 `LICENSE` / `README`)。

---

## 1. 为什么是 Vibird —— 定位

市场调研(2026-06):最显而易见的点子 ——「**Claude 桌宠 + 端侧批准/拒绝**」—— **已被平台方覆盖**
(`anthropics/claude-desktop-buddy`:ESP32 + BLE,会睡醒、闹脾气、端侧 approve/deny,还有商用产品 + 多个
社区移植)。所以 Vibird 另辟蹊径。

**Vibird 刻意打调研发现的三个真空:**

- **A —— 语音输入。** 现在所有 ESP32 语音助手都接 OpenAI / Gemini / Qwen;所有 Claude 语音工具都是纯软件。
  **没有人把 ESP32 麦克风做成 Claude Code 的一等输入。**
- **B —— 跨 Agent。** Cursor / Copilot / Codex 完全没有硬件控制。
- **C —— 零配置。** `pip install`,然后 **Agent 读一个内置 skill 自己把设备配好**。配置摩擦是开发者硬件
  最大的痛点;Vibird 把它变成卖点。没有竞品在做。

**设计法则**(不可妥协):

1. **窄而可靠。** 把一件事 —— 语音 + 状态 —— 做到极致。不要试图取代任何东西。
2. **被动优先。** 一眼看到 Agent 状态;主动输入(语音 / 按键)是加分项,不是负担。
3. **默认零配置。**
4. **永不变砖。** 开放协议、可 OTA 固件、优雅离线降级。
5. **只在该响的时候出声**(反告警疲劳)。
6. **价格带 $49–120。**

**Vibird 不是:** 又一个 Claude 桌宠 · 手机替代品 · 要你重新编程的宏键盘。

---

## 2. 架构

```
   设备(参考机:M5 AtomS3R + Echo Base)         Host bridge / SDK(Rust)            AI Agent
   ┌───────────────────────────────┐    WiFi /      ┌──────────────────────────┐  hooks  ┌────────────┐
   │ • 长按说话 → 16k PCM ──────────┼── WebSocket ──▶│ WS 服务端(设备主动连入)  │── MCP ─▶│ Claude Code│
   │ • 高刷新状态动画(Liz 表情)   │   (二进制)     │ 双引擎 ASR                 │  tmux   │ (Cursor /  │
   │ • 按键 + IMU 输入              │◀── 状态 ───────│ Claude Code 集成           │◀────────│  Codex →)  │
   │ • RGB / 扬声器                 │   下推         │ mDNS 广播                  │ approve └────────────┘
   └───────────────────────────────┘                └──────────────────────────┘
   不绑定 S3R:任何讲 Vibird 协议的设备都行       交付:`cargo install` + `pip install`(PyO3/maturin)+ 一键 Claude Code 插件
```

三层:

- **设备** —— 一个轻薄、有表情的 WebSocket **客户端**。采集语音、显示 Agent 状态、读按键/IMU。S3R 是
  *参考*机,不是硬依赖。
- **Bridge / SDK(Rust —— 产品核心)** —— WebSocket **服务端** + ASR + Agent 集成。可复用、跨设备、跨 Agent
  的开源 + 商用核心。
- **Agent** —— 先做 Claude Code:hooks(状态 + 物理批准/拒绝)、MCP(设备工具)、prompt 注入(语音→文本)。
  之后接 Cursor / Codex。

### 关键决策(有调研支撑)

| 方面 | 决策 | 为什么 |
|---|---|---|
| 角色 | **设备=WS 客户端,Mac=WS 服务端** | ESP32 的 WS *服务端*库都有 bug;*客户端*成熟;能扛 WiFi 切换;NAT/发现更简单。 |
| 发现 | **mDNS** —— Mac 广播 `_vibird._tcp`;设备开机查询一次(带重试)再连 | 把易错的查询放在可靠的一侧;局域网零配置。 |
| 首次配置 | **串口 JSON 指令 → NVS**,之后走网络 | 契合「网络优先,只首配走串口」;绕开 AtomS3R 原生 USB-CDC 的 Improv reset 不稳。 |
| 语音 | **长按说话,16 kHz 单声道 PCM,按住即流式上传**,约 30 ms 一帧;端侧不做 Opus/VAD | PTT 给出完美边界;局域网带宽充足;保持设备简单。 |
| ASR(host) | **双引擎:** `parakeet-mlx`(英文,~80 ms,默认)+ `mlx-whisper large-v3-turbo`(中文 / 中英混)。云端(Deepgram / gpt-4o)可选。 | Parakeet 英文最快最准但**没有中文**;Whisper 覆盖中文/混说。 |
| **传意图,不传语法** | Vibird 传*你说的话*;**Claude Code 把意图变成代码** | **没有 ASR 能吐出代码符号或大小写**(`()`、`->`、camelCase)。Agent 才是解析器 —— 这正是它的强项。 |
| prompt 注入 | **tmux send-keys**(v0.1)→ 之后 Agent SDK / stream-json | 现在没有官方 IPC 注入活动的交互会话;tmux 是当下最可靠的路。 |
| 状态 + 批准 | **Claude Code hooks**:`Notification`/`Stop`/`PostToolUse` → 推状态;**`PreToolUse` → 设备返回 `allow`/`deny`/`ask`** | hooks 是稳定、有文档的接口;PreToolUse 就是一个物理权限闸门。 |
| 延迟 | 松手到出文 ~250–450 ms(本地英文 Parakeet) | 「感觉即时」的阈值;按住即流 + 常驻模型。 |

---

## 3. 角色 —— Liz「栗子」

设备的脸是 **Liz「栗子」** —— 一个 **17 岁的二次元女孩**(详见 [ADR-0005](../../agent/adr/ADR-0005-character-liz.md)):

- **发型:** 黑中长直。
- **着装:** 喜欢 **Lolita「女儿服」** 和 **JK 水手服**。
- **构图:** 128×128 屏上的**半身**像。
- **动态:** 像 telegram 动态表情一样**丝滑**,每个 Agent 状态一套表情 + 过渡。

定位:大家都在做机器人;Vibird 的脸走**二次元伙伴**路线,而不是机器人/宠物。「Vibird」是产品/品牌名,
**Liz「栗子」是吉祥物角色**。表情集与协议里的 `AgentState`(idle / listening / thinking / working /
awaiting / done / error)一一对应。

---

## 4. 渲染 & 表情管线

固件**全程纯 Rust**(esp-hal,no_std;见 [ADR-0002](../../agent/adr/ADR-0002-firmware-rust-esp-rs.md)、
[ADR-0004](../../agent/adr/ADR-0004-on-device-rendering.md))。借乐鑫 emote 方案的**技术思路**(按区域刷新、
预解码 RGB565 帧、命名片段 intro/loop/tail),但**格式和工具全自研**:

- **Vibird Emote Pack(`.veap`)** —— 自研二进制资源包:头 + 清单(命名片段、布局、片段计划)+ 每片段
  **RGB565 + 按区域 delta** 的帧(只存/只刷变化的矩形 → 比直接解 GIF 快约 2×,也就是「telegram 丝滑」)。
- **Host 打包器(Rust)** —— `vibird-emote-pack`:GIF / PNG 序列(Liz 的美术)→ `.veap`;先做 CLI,之后编
  wasm 出浏览器版。
- **固件播放器(Rust,esp-hal no_std)** —— 从 flash 分区 mmap 资源包,按名字播片段,只刷脏矩形到 GC9107。
  复用已验证的 `ColorOrder::Bgr` / `ColorInversion::Normal` / `offset_y=32`
  ([finding](../../agent/findings/finding-gc9107-color-order.md),已硬件确认)。
- 之前手搓的抗锯齿矢量渲染器降级为**无美术资源时的兜底 / 开机动画**。

> 实测:像素路径 99 fps、矢量路径 ~53 fps([finding](../../agent/findings/finding-rust-animation-feasibility.md))
> —— 纯 Rust 高刷新这个头号风险已关闭。

---

## 5. 技术栈

- **Host(Rust):** `tokio`、`tungstenite`/`axum`(WS)、`rmcp`(MCP)、whisper/parakeet 绑定、CLI、以及
  `vibird-emote-pack` 打包器。以 **PyO3 + maturin 的 pip wheel** *和* `cargo install` 交付。这是可复用核心。
- **固件(Rust):** **esp-rs / `esp-hal`**(no_std,裸机,纯 Rust)on ESP32-S3。`embedded-graphics` + DMA
  双缓冲帧缓冲 + 自研 `.veap` 区域刷新播放器;`esp-wifi` + WS 客户端;I2S + ES8311 编解码;BMI270 IMU;RGB。
  **不用 Zephyr**(见 ADR-0002)。
- **分发:** Claude Code **插件**(hooks + skills + 根 `.mcp.json`,一键装)+ `pip install vibird`(bridge)
  + `cargo install vibird`。
- **许可:** **AGPL-3.0**(社区)+ **商用许可** + **CLA**。所有依赖保持宽松
  (MIT/Apache/BSD),商用再授权才干净。
- **构建网络:** crates.io / PyPI 经路由器透明代理直连(rsproxy / 清华镜像兜底);GitHub 可达。

---

## 6. 路线图

| 版本 | 交付物 |
|---|---|
| **v0.1 语音闭环** | S3R 长按说话 → 注入 Claude Code;基础 idle/listening/thinking 表情。**前置去风险 spike:** (1) 纯 Rust 动画 —— ✅ **已完成**(99/53 fps);(2) esp-wifi WS 音频流可靠性 —— ⏳ 待做。 |
| **v0.2 状态 + 物理批准** | hooks 把 Agent 状态推到设备;`PreToolUse` 物理 allow/deny。 |
| **v0.3 零配置** | Claude Code 插件 + pip 包;Agent 自己配设备。**← 核心差异点。** |
| **v0.4 跨 Agent** | Cursor / Codex 适配。 |
| **v0.5 商用级** | Liz 表情打磨、AGPL/CLA/文档站、OTA、离线降级、量产打包。 |

---

## 7. 仓库结构(monorepo)

```
vibird/
├── firmware/        # Rust(esp-hal)—— S3R 参考客户端 + .veap 播放器
├── host/            # Rust 工作区 —— core / bridge / cli / mcp / emote 打包器(产品核心)
├── python/          # PyO3 + maturin → `pip install vibird`
├── claude-plugin/   # Claude Code 插件(hooks / skills / .mcp.json)
├── protocol/        # 设备 ↔ bridge 协议(版本化)
├── docs/            # agent 线(SNAPSHOT/ADR/findings)+ human 线(中英双语)
└── assets/          # 美术、Liz 表情素材
```

---

## 8. 待消除的风险 —— 尽早 spike

1. **esp-wifi 可靠性** —— 持续 WS 音频流(兜底:`esp-idf-svc` / ESP-IDF WiFi,仍是 Rust)。**开放**。
2. ~~**纯 Rust 高刷新动画**~~ —— ✅ **硬件已验证**(99 / 53 fps);颜色管线也已修复 + 确认。
3. **自研 emote 管线** —— `.veap` 格式 + 打包器 + 区域刷新播放器都要从零写(已选定纯 Rust 自研格式)。
4. **prompt 注入体验**(v0.1 依赖 tmux;之后走更干净的 SDK 路)。
5. **中文 / 中英混说 ASR** 质量(双引擎;云端兜底)。
6. **Liz 美术产出方式**(Live2D / 约稿 / AI)—— 下一个待定决策。

---

## 9. 调研基础

三条并行调研线(Claude Code 集成 · 市场与功能挖掘 · 技术选型),2026-06,对承重结论做了多源对抗式核实。
主要来源:Claude Code 文档(hooks / mcp / plugins)、`anthropics/claude-desktop-buddy`、Open ASR
Leaderboard、`esp_websocket_client` changelog、`parakeet-mlx`;表情管线参考乐鑫 `esp_emote_gen_player`
(Apache-2.0)的区域刷新思路。源码核实的硬件引脚见 [atoms3r-hardware](../../agent/atoms3r-hardware.md)。
