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

use embassy_executor::Spawner;
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
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
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
use vibird_protocol::{AgentState, Caps, Downlink, Uplink, PROTOCOL_VERSION};

esp_bootloader_esp_idf::esp_app_desc!();

const W: u16 = 128;
const H: u16 = 128;
const LP5562_ADDR: u8 = 0x30;

/// 内嵌占位表情包(7 态);Liz 美术到位后换正式 .veap。
static PLACEHOLDER: &[u8] = include_bytes!("../../../assets/placeholder.veap");

/// bridge 下行的最新 AgentState → 显示循环(Signal 自动只留最新值)。
static STATE_SIG: Signal<CriticalSectionRawMutex, AgentState> = Signal::new();

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

    // ---- Echo Base 探测(base I2C 38/39)----
    {
        let mut base = I2c::new(p.I2C1, I2cConfig::default())
            .unwrap()
            .with_sda(p.GPIO38)
            .with_scl(p.GPIO39);
        let mut pb = [0u8; 1];
        if base.read(0x18u8, &mut pb).is_ok() {
            info!("Echo Base ES8311 @0x18 present");
        } else {
            warn!("Echo Base 未检测到(麦克风上行 Gate 5 需要它)");
        }
    }

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
                // 0.10:#[task] 函数返回 Result<SpawnToken,_>(池满则 Err);各起一次,unwrap 安全。
                spawner.spawn(net_task(runner).unwrap());
                spawner.spawn(wifi_conn(controller).unwrap());
                spawner.spawn(bridge_task(stack).unwrap());
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
        // 3. PTT 本地即时反馈(Gate 5 再接 ES8311 麦克风上传)
        let held = button.is_low();
        if held && !was_held {
            player.set_clip("listening");
            info!("PTT down");
        } else if !held && was_held {
            info!("PTT up");
            clip_t0 = Instant::now();
        }
        was_held = held;
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

/// 连 bridge:TCP → WebSocket 握手 → Hello → 循环收下行,SetState 经 STATE_SIG 驱动表情。
#[embassy_executor::task]
async fn bridge_task(stack: Stack<'static>) {
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

        // 发 Hello
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

        // 收下行帧
        let mut buf = [0u8; 1024];
        loop {
            match ws::read_frame(&mut sock, &mut buf).await {
                Ok((ws::TEXT, n)) => match serde_json::from_slice::<Downlink>(&buf[..n]) {
                    Ok(Downlink::Welcome { protocol, .. }) => info!("Welcome(proto={protocol})"),
                    Ok(Downlink::SetState(state)) => STATE_SIG.signal(state),
                    Ok(Downlink::Ping { nonce }) => {
                        if let Ok(j) = serde_json::to_string(&Uplink::Pong { nonce }) {
                            let _ = ws::send_text(&mut sock, &j).await;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => warn!("下行 JSON 解析失败"),
                },
                Ok((ws::PING, n)) => {
                    let _ = ws::send_pong(&mut sock, &buf[..n]).await;
                }
                Ok((ws::CLOSE, _)) => {
                    warn!("bridge 关闭了连接;重连");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("读帧错误:{e:?};重连");
                    break;
                }
            }
        }
        Timer::after(Duration::from_secs(2)).await;
    }
}
