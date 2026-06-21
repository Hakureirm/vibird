//! 设备编译期配置:WiFi 凭据 + bridge 地址,经环境变量在编译时注入。
//!
//! 烧录时传入(缺省则为占位,能编译,运行时在串口报缺配置):
//! ```text
//! VIBIRD_WIFI_SSID=myssid VIBIRD_WIFI_PASS=mypass \
//!   VIBIRD_BRIDGE_ADDR=192.168.1.50:8137 cargo run --release
//! ```
//! 将来由 USB 串口 `vibird config` 流程写入 NVS 取代,实现零配置开箱。

/// WiFi SSID(空 = 未配置)。
pub const WIFI_SSID: &str = match option_env!("VIBIRD_WIFI_SSID") {
    Some(s) => s,
    None => "",
};

/// WiFi 密码(空 = 开放网络 / 未配置)。
pub const WIFI_PASS: &str = match option_env!("VIBIRD_WIFI_PASS") {
    Some(s) => s,
    None => "",
};

/// bridge 地址 `a.b.c.d:port`(空 = 未配置)。默认端口见 [`DEFAULT_BRIDGE_PORT`]。
pub const BRIDGE_ADDR: &str = match option_env!("VIBIRD_BRIDGE_ADDR") {
    Some(s) => s,
    None => "",
};

/// 桥接默认 WebSocket 端口(与 host 侧 `vibird_bridge::DEFAULT_PORT` 一致)。
pub const DEFAULT_BRIDGE_PORT: u16 = 8137;

/// 麦克风调试模式(编译期 `VIBIRD_MIC_TEST=1`):忽略按键,自动循环采集 / 上传,
/// 方便没法按按键时验证麦克风采集 + 音频格式。正式构建不设此变量。
pub const MIC_TEST: bool = option_env!("VIBIRD_MIC_TEST").is_some();

/// 配置是否齐全(SSID + bridge 地址都非空)。
pub fn is_configured() -> bool {
    !WIFI_SSID.is_empty() && !BRIDGE_ADDR.is_empty()
}

/// 解析 `BRIDGE_ADDR`(`a.b.c.d:port`,port 可省 → [`DEFAULT_BRIDGE_PORT`])成 `([u8;4], u16)`。
/// 解析失败返回 `None`。手写解析以避免 no_std 下没有 `FromStr` for IP 的麻烦。
pub fn parse_bridge_addr() -> Option<([u8; 4], u16)> {
    parse_addr(BRIDGE_ADDR)
}

fn parse_addr(s: &str) -> Option<([u8; 4], u16)> {
    let (ip_str, port) = match s.split_once(':') {
        Some((ip, p)) => (ip, p.parse::<u16>().ok()?),
        None => (s, DEFAULT_BRIDGE_PORT),
    };
    let mut octets = [0u8; 4];
    let mut it = ip_str.split('.');
    for o in octets.iter_mut() {
        *o = it.next()?.parse::<u8>().ok()?;
    }
    if it.next().is_some() {
        return None; // 多于 4 段
    }
    Some((octets, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ip_and_port() {
        assert_eq!(parse_addr("192.168.1.50:8137"), Some(([192, 168, 1, 50], 8137)));
    }

    #[test]
    fn defaults_port_when_missing() {
        assert_eq!(parse_addr("10.0.0.1"), Some(([10, 0, 0, 1], DEFAULT_BRIDGE_PORT)));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_addr("not-an-ip"), None);
        assert_eq!(parse_addr("1.2.3.4.5:1"), None);
        assert_eq!(parse_addr("1.2.3:1"), None);
    }
}
