//! Vibird 固件 —— 完整端到端(embassy 异步 + WiFi/WebSocket + 表情)。
//!
//! 架构(单核 thread executor,由 esp-rtos 提供):
//!   - `main`(本任务):初始化外设 → 起 WiFi/网络/桥接任务 → 跑**显示循环**(.veap 区域刷新)。
//!   - `net_task`:embassy-net 协议栈。
//!   - `wifi_conn`:STA 连接 / 重连。
//!   - `bridge_task`:TCP+WebSocket 连 bridge,发 Hello,收 SetState 下行 → 经 `STATE_SIG` 驱动表情。
//!
//! 下行状态(bridge → 设备)闭合「状态显示」半环;按键 push-to-talk 的麦克风上行(ES8311+I2S)是 Gate 5。
//! WiFi 凭据 + bridge 地址在编译期由 `config` 注入(见 config.rs)。
//!
//! 引脚(源码核对 —— 见 docs/agent/atoms3r-hardware.md):
//!   显示 GC9107(SPI2): SCLK=15, MOSI=21, CS=14, DC=42, RST=48;40 MHz;128x128,offset_y=32。
//!   背光 LP5562:内部 I2C(SDA=45, SCL=0),地址 0x30。
//!   按键(整面):GPIO41 低有效。Echo Base:base I2C(SDA=38, SCL=39),ES8311 @0x18。

#![no_std]
#![no_main]

extern crate alloc;

// config.rs / ws.rs 在 src/ 下(放 src/bin/ 会被 cargo 当成独立 binary),用 #[path] 引入。
#[path = "../config.rs"]
mod config;
#[path = "../ws.rs"]
mod ws;

use alloc::string::String;
use core::net::Ipv4Addr;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, IpEndpoint, Runner, Stack, StackResources};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::dma_buffers;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::i2s::master::{Channels, Config as I2sConfig, DataFormat, I2s, I2sRx};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::Async;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, Interface, WifiController};
use log::{info, warn};
use mipidsi::interface::SpiInterface;
use mipidsi::models::GC9107;
use mipidsi::options::{ColorInversion, ColorOrder};
use mipidsi::Builder;
use vibird_emote::{Pack, Player};
use vibird_protocol::{AgentState, AudioFormat, Caps, Downlink, Uplink, PROTOCOL_VERSION};

esp_bootloader_esp_idf::esp_app_desc!();

const W: u16 = 128;
const H: u16 = 128;
const LP5562_ADDR: u8 = 0x30;

/// 内嵌占位表情包(7 态);Liz 美术到位后换正式 .veap。
static PLACEHOLDER: &[u8] = include_bytes!("../../../assets/placeholder.veap");

/// bridge 下行的最新 AgentState → 显示循环(Signal 自动只留最新值)。
static STATE_SIG: Signal<CriticalSectionRawMutex, AgentState> = Signal::new();

/// 按住 PTT 期间为 true(显示循环按按键置位 → bridge_task 据此上传麦克风音频)。
static MIC_ON: AtomicBool = AtomicBool::new(false);

/// 把值放进 'static StaticCell,返回 &'static mut(embassy 任务 / 资源需要 'static)。
macro_rules! mk_static {
    ($t:ty, $v:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($v)
    }};
}

#[inline]
fn rgb565_from_raw(c: u16) -> Rgb565 {
    Rgb565::new((c >> 11) as u8, ((c >> 5) & 0x3f) as u8, (c & 0x1f) as u8)
}

/// AgentState → 表情包 clip 名(与 assets/placeholder.veap 的 7 个 clip 一一对齐)。
fn clip_for(state: &AgentState) -> &'static str {
    match state {
        AgentState::Idle => "idle",
        AgentState::Listening => "listening",
        AgentState::Thinking => "thinking",
        AgentState::Working { .. } => "working",
        AgentState::AwaitingApproval { .. } => "awaiting_approval",
        AgentState::Done => "done",
        AgentState::Error { .. } => "error",
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    // 双堆(对齐 esp-radio 官方示例):reclaimed 区(二级 bootloader 用完回收的 RAM)主要给 esp-radio,
    // 常规区给显示 + 表情 + serde。真机 OOM 再调。
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 72 * 1024);
    let mut delay = Delay::new();

    // ---- 背光 LP5562(内部 I2C SDA=45 SCL=0)----
    {
        let mut i2c = I2c::new(p.I2C0, I2cConfig::default())
            .unwrap()
            .with_sda(p.GPIO45)
            .with_scl(p.GPIO0);
        let _ = i2c.write(LP5562_ADDR, &[0x00, 0x40]);
        let _ = i2c.write(LP5562_ADDR, &[0x08, 0x01]);
        let _ = i2c.write(LP5562_ADDR, &[0x70, 0x00]);
        let _ = i2c.write(LP5562_ADDR, &[0x0E, 0xC0]);
        info!("backlight (LP5562) on");
    }

    // ---- 显示 GC9107(SPI2)----
    let spi = Spi::new(
        p.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(p.GPIO15)
    .with_mosi(p.GPIO21);
    let cs = Output::new(p.GPIO14, Level::High, OutputConfig::default());
    let dc = Output::new(p.GPIO42, Level::Low, OutputConfig::default());
    let rst = Output::new(p.GPIO48, Level::High, OutputConfig::default());
    let spi_dev = ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();
    let mut if_buf = alloc::vec![0u8; 4096];
    let di = SpiInterface::new(spi_dev, dc, &mut if_buf);
    let mut display = Builder::new(GC9107, di)
        .display_size(W, H)
        .display_offset(0, 32)
        .invert_colors(ColorInversion::Normal)
        .color_order(ColorOrder::Bgr) // 此 AtomS3R 面板是 BGR(已真机确认)
        .reset_pin(rst)
        .init(&mut delay)
        .unwrap();
    info!("display (GC9107) init ok");

    // ---- 按键 GPIO41(低有效)= push-to-talk ----
    let button = Input::new(p.GPIO41, InputConfig::default().with_pull(Pull::Up));

    // ---- esp-rtos 调度器(必须在 radio 之前)----
    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_int = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // ---- WiFi + 网络(仅当配置齐全才起;否则离线轮播表情)----
    let online = config::is_configured();
    if online {
        match esp_radio::wifi::new(p.WIFI, ControllerConfig::default()) {
            Ok((controller, interfaces)) => {
                let seed = 0x1234_5678_9abc_def0u64; // 固定 seed(无 RNG;局域网够用)
                let resources = mk_static!(StackResources<4>, StackResources::new());
                let (stack, runner) = embassy_net::new(
                    interfaces.station,
                    embassy_net::Config::dhcpv4(Default::default()),
                    resources,
                    seed,
                );
                // ---- ES8311 麦克风初始化(base I2C1 38/39 @0x18)----
                {
                    let mut base = I2c::new(p.I2C1, I2cConfig::default())
                        .unwrap()
                        .with_sda(p.GPIO38)
                        .with_scl(p.GPIO39);
                    let codec = es8311::Es8311::new(0x18);
                    let clk = es8311::ClockConfig {
                        mclk_inverted: false,
                        sclk_inverted: false,
                        mclk_from_mclk_pin: false, // 无 MCLK 引脚 → 从 SCLK/BCLK 派生
                        mclk_frequency: 0,
                        sample_frequency: 16_000,
                    };
                    // 16kHz 无 MCLK:用 Bits32(派生 MCLK=1.024MHz 命中 coeff 表),配 I2S Data32Channel32
                    match codec.init(
                        &mut base,
                        &clk,
                        es8311::Resolution::Bits32,
                        es8311::Resolution::Bits32,
                        &mut delay,
                    ) {
                        Ok(()) => {
                            let _ = codec.microphone_config(&mut base, false); // 模拟麦克风
                            let _ = codec.microphone_gain_set(&mut base, es8311::MicGain::Gain30dB);
                            let _ = codec.mute(&mut base, true); // 关 DAC 输出:录音设备不发声,消 PTT 时喇叭噪声
                            info!("ES8311 麦克风初始化 ok");
                        }
                        Err(e) => warn!("ES8311 初始化失败:{e:?}(麦克风不可用)"),
                    }
                    // 静音 Echo Base 喇叭功放(PI4IOE5V6408 @0x43 → NS4150B)——录音设备别让喇叭滋滋响
                    let _ = base.write(0x43u8, &[0x03, 0x6F]); // P0..P3 方向=输出
                    let _ = base.write(0x43u8, &[0x05, 0x00]); // 输出全低 → 功放关断(静音)
                }
                // ---- I2S RX(BCLK=8 WS=6 DIN=7;16kHz;32bit slot;异步 DMA,PTT 时一次性读)----
                // 4088 字节(≤CHUNK 4092 → 单描述符;8 的倍数 → 整 stereo 帧);读进这块**静态 DMA 缓冲**
                // (DMA 不能读任务栈,否则 DescriptorError)。
                let (rx_buffer, rx_descriptors, _, _) = dma_buffers!(4088, 0);
                let i2s = I2s::new(
                    p.I2S0,
                    p.DMA_CH0,
                    I2sConfig::new_tdm_philips()
                        .with_sample_rate(Rate::from_hz(16_000))
                        .with_data_format(DataFormat::Data32Channel32)
                        .with_channels(Channels::STEREO),
                )
                .unwrap()
                .into_async();
                let i2s_rx = i2s
                    .i2s_rx
                    .with_bclk(p.GPIO8)
                    .with_ws(p.GPIO6)
                    .with_din(p.GPIO7)
                    .build(rx_descriptors);

                // 0.10:#[task] 函数返回 Result<SpawnToken,_>(池满则 Err);各起一次,unwrap 安全。
                spawner.spawn(net_task(runner).unwrap());
                spawner.spawn(wifi_conn(controller).unwrap());
                spawner.spawn(bridge_task(stack, i2s_rx, rx_buffer).unwrap());
                info!("WiFi/bridge 任务已起(SSID={})", config::WIFI_SSID);
            }
            Err(e) => warn!("esp_radio::wifi::new 失败:{e:?}"),
        }
    } else {
        warn!("未配置 WiFi/bridge(构建时传 VIBIRD_WIFI_SSID/PASS/BRIDGE_ADDR);离线轮播表情");
    }

    // ---- 显示循环(本任务)----
    let pack = match Pack::parse(PLACEHOLDER) {
        Ok(pk) => pk,
        Err(_) => {
            warn!("表情包解析失败;停在黑屏");
            loop {
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    };
    let (cw, ch) = pack.canvas();
    info!("emote pack: {} clips, canvas {cw}x{ch}", pack.clip_count());
    let mut player = Player::new(pack);
    const STEP_MS: u32 = 5;
    let mut clip_t0 = Instant::now();
    let mut was_held = false;
    let mut fps_t0 = Instant::now();
    let mut frames_n = 0u32;
    let mut mic_t0 = Instant::now();
    let mut mic_test_on = false;
    loop {
        // 1. bridge 下行状态 → 切表情(在线时唯一的表情驱动源)
        if let Some(st) = STATE_SIG.try_take() {
            let name = clip_for(&st);
            player.set_clip(name);
            clip_t0 = Instant::now();
            info!("SetState → {name}");
        }
        // 2. tick + 区域刷新(只刷脏矩形)
        if let Some(frame) = player.tick(STEP_MS) {
            for r in frame.rects() {
                let x1 = r.x + r.w.saturating_sub(1);
                let y1 = r.y + r.h.saturating_sub(1);
                display
                    .set_pixels(r.x, r.y, x1, y1, r.pixels_rgb565().map(rgb565_from_raw))
                    .ok();
            }
            frames_n += 1;
        }
        // 3. PTT:本地切 listening + 置 MIC_ON(bridge_task 据此上传 ES8311 麦克风音频)
        let held = button.is_low();
        if held && !was_held {
            player.set_clip("listening");
            MIC_ON.store(true, Ordering::Relaxed);
            info!("PTT down");
        } else if !held && was_held {
            MIC_ON.store(false, Ordering::Relaxed);
            info!("PTT up");
            clip_t0 = Instant::now();
        }
        was_held = held;
        // 调试模式:忽略按键,自动 4s 采集 / 1.5s 静默(静默触发 AudioEnd → bridge 转写)
        if config::MIC_TEST {
            let dur = if mic_test_on { 4000 } else { 1500 };
            if mic_t0.elapsed() >= Duration::from_millis(dur) {
                mic_test_on = !mic_test_on;
                MIC_ON.store(mic_test_on, Ordering::Relaxed);
                if mic_test_on {
                    player.set_clip("listening");
                }
                mic_t0 = Instant::now();
            }
        }
        // 4. fps 日志
        if fps_t0.elapsed() >= Duration::from_secs(1) {
            info!("emote {frames_n} fps (online={online})");
            frames_n = 0;
            fps_t0 = Instant::now();
        }
        // 5. 离线时自动轮播展示全部表情;在线时只由 SetState 驱动
        if !online && !held && clip_t0.elapsed() >= Duration::from_millis(2500) {
            player.next_clip();
            clip_t0 = Instant::now();
        }
        Timer::after(Duration::from_millis(STEP_MS as u64)).await;
    }
}

/// embassy-net 协议栈后台任务。
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

/// WiFi STA 连接 / 断线重连。
#[embassy_executor::task]
async fn wifi_conn(mut controller: WifiController<'static>) {
    let cfg = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(config::WIFI_SSID)
            .with_password(String::from(config::WIFI_PASS)),
    );
    if let Err(e) = controller.set_config(&cfg) {
        warn!("WiFi set_config 失败:{e:?}");
    }
    loop {
        if !controller.is_connected() {
            match controller.connect_async().await {
                Ok(_) => info!("WiFi 已连接 → {}", config::WIFI_SSID),
                Err(e) => {
                    warn!("WiFi 连接失败:{e:?};5s 后重试");
                    Timer::after(Duration::from_secs(5)).await;
                }
            }
        }
        Timer::after(Duration::from_secs(2)).await;
    }
}

/// 连 bridge:TCP → WS 握手 → Hello → **并发**:收下行 SetState 驱动表情 / 按 PTT 上传麦克风 PCM。
#[embassy_executor::task]
async fn bridge_task(
    stack: Stack<'static>,
    mut i2s_rx: I2sRx<'static, Async>,
    rx_buffer: &'static mut [u8],
) {
    stack.wait_config_up().await;
    if let Some(cfg) = stack.config_v4() {
        info!("DHCP 拿到 IP = {}", cfg.address);
    }
    let Some((octets, port)) = config::parse_bridge_addr() else {
        warn!("bridge 地址解析失败:{}", config::BRIDGE_ADDR);
        return;
    };
    let ep = IpEndpoint::new(
        IpAddress::Ipv4(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])),
        port,
    );
    let host = config::BRIDGE_ADDR;

    let mut rx = [0u8; 2048];
    let mut tx = [0u8; 2048];
    loop {
        let mut sock = TcpSocket::new(stack, &mut rx, &mut tx);
        info!("连 bridge {host} …");
        if let Err(e) = sock.connect(ep).await {
            warn!("TCP 连接失败:{e:?};3s 重试");
            Timer::after(Duration::from_secs(3)).await;
            continue;
        }
        if let Err(e) = ws::connect(&mut sock, host).await {
            warn!("WS 握手失败:{e:?};3s 重试");
            Timer::after(Duration::from_secs(3)).await;
            continue;
        }
        info!("WebSocket 已连上 bridge");

        // 发 Hello(拆分 socket 前顺序发)
        let hello = Uplink::Hello {
            device_id: String::from("atoms3r-vibird"),
            fw_version: String::from(env!("CARGO_PKG_VERSION")),
            protocol: PROTOCOL_VERSION,
            caps: Caps {
                mic: true,
                speaker: false,
                display: true,
                imu: false,
            },
        };
        match serde_json::to_string(&hello) {
            Ok(json) => {
                if ws::send_text(&mut sock, &json).await.is_err() {
                    warn!("发 Hello 失败;重连");
                    continue;
                }
            }
            Err(_) => continue,
        }

        // 拆分:读半边收下行,写半边传音频,二者并发。
        let (mut reader, mut writer) = sock.split();

        // 下行:SetState → 表情;CLOSE / 读错则结束本连接。
        let down = async {
            let mut buf = [0u8; 1024];
            loop {
                match ws::read_frame(&mut reader, &mut buf).await {
                    Ok((ws::TEXT, n)) => match serde_json::from_slice::<Downlink>(&buf[..n]) {
                        Ok(Downlink::Welcome { protocol, .. }) => info!("Welcome(proto={protocol})"),
                        Ok(Downlink::SetState(state)) => STATE_SIG.signal(state),
                        Ok(_) => {}
                        Err(_) => warn!("下行 JSON 解析失败"),
                    },
                    Ok((ws::CLOSE, _)) => {
                        warn!("bridge 关闭了连接");
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        };

        // 上行:PTT 期间一次性读 I2S(await 让出执行器)→ 取单声道 → 上传;空闲 Timer 让出。
        let up = async {
            let mut pcm = [0u8; 2048]; // 提取出的单声道 i16 LE(rx_buffer 是 DMA 静态缓冲)
            let mut sending = false;
            loop {
                if MIC_ON.load(Ordering::Relaxed) {
                    if !sending {
                        if let Ok(j) = serde_json::to_string(&Uplink::AudioStart {
                            sample_rate: 16_000,
                            format: AudioFormat::Pcm16Le,
                        }) {
                            if ws::send_text(&mut writer, &j).await.is_err() {
                                break;
                            }
                        }
                        sending = true;
                    }
                    // 一次性读满 rx_buffer(await 让出);失败停 20ms 再试,不拖垮连接。
                    if let Err(e) = i2s_rx.read_dma_async(&mut rx_buffer[..]).await {
                        warn!("I2S 读取出错:{e:?}");
                        Timer::after(Duration::from_millis(20)).await;
                        continue;
                    }
                    // Data32 stereo:每帧 8 字节 [L(4)|R(4)];取左声道高 16 位作单声道 16k。
                    let mut mi = 0;
                    for f in rx_buffer.chunks_exact(8) {
                        let left = i32::from_le_bytes([f[0], f[1], f[2], f[3]]);
                        let s = (left >> 16) as i16;
                        let b = s.to_le_bytes();
                        if mi + 2 <= pcm.len() {
                            pcm[mi] = b[0];
                            pcm[mi + 1] = b[1];
                            mi += 2;
                        }
                    }
                    if mi > 0 && ws::send_binary(&mut writer, &pcm[..mi]).await.is_err() {
                        break;
                    }
                } else {
                    if sending {
                        if let Ok(j) = serde_json::to_string(&Uplink::AudioEnd) {
                            let _ = ws::send_text(&mut writer, &j).await;
                        }
                        sending = false;
                    }
                    Timer::after(Duration::from_millis(20)).await; // 空闲让出
                }
            }
        };

        match select(down, up).await {
            Either::First(_) => warn!("下行循环结束;重连"),
            Either::Second(_) => warn!("上行循环结束;重连"),
        }
        Timer::after(Duration::from_secs(2)).await;
    }
}
