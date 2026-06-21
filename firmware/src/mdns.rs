//! 极简 no_std mDNS 客户端 —— 只够设备发现 bridge 用。
//!
//! 经 embassy-net 的 `UdpSocket` 发一个 `_vibird._tcp.local` 的 **PTR 查询**,解析响应里的
//! **SRV**(端口)+ **A**(IPv4)记录,得到 bridge 的 `(IP, port)`,免硬编码 bridge 地址。
//!
//! 为何手写而不用 `edge-mdns`:后者唯一的 embassy 绑定 `edge-nal-embassy` 锁 `embassy-net ^0.8`,
//! 跟本项目的 0.9.1 不兼容(会静默拉两份 embassy-net 导致类型对不上)。手写零新依赖,只需给
//! embassy-net 开 `udp` + `multicast` 特性。

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, IpListenEndpoint, Ipv4Address, Stack};
use embassy_time::{Duration, with_timeout};
use log::{info, warn};

const MDNS_ADDR: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

/// 构造一个 PTR 查询包(问 `qname` 的 PTR 记录),返回写入 `buf` 的长度。
fn build_ptr_query(buf: &mut [u8], qname: &[&str]) -> usize {
    // DNS 头:ID=0,FLAGS=0(标准查询),QDCOUNT=1,AN/NS/AR=0
    buf[..12].copy_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    let mut i = 12;
    for label in qname {
        buf[i] = label.len() as u8;
        i += 1;
        buf[i..i + label.len()].copy_from_slice(label.as_bytes());
        i += label.len();
    }
    buf[i] = 0; // 根标签
    i += 1;
    buf[i..i + 4].copy_from_slice(&[0, 12, 0, 1]); // QTYPE=PTR(12), QCLASS=IN(1)
    i + 4
}

#[inline]
fn rd16(b: &[u8], o: usize) -> u16 {
    ((b[o] as u16) << 8) | b[o + 1] as u16
}

/// 跳过一个 DNS 名(处理 0xC0 压缩指针),返回名之后的偏移。
fn skip_name(msg: &[u8], mut pos: usize) -> usize {
    loop {
        if pos >= msg.len() {
            return pos;
        }
        let len = msg[pos];
        if len == 0 {
            return pos + 1; // 根 → 结束
        }
        if len & 0xC0 == 0xC0 {
            return pos + 2; // 压缩指针(2 字节)即终止本名
        }
        pos += 1 + len as usize; // 普通标签
    }
}

#[derive(Default)]
struct Resolved {
    ip: Option<Ipv4Address>,
    port: Option<u16>,
}

/// 解析 mDNS 响应,取 SRV(端口,RDATA+4 大端)+ A(IPv4,4 字节)。
fn parse_response(msg: &[u8]) -> Resolved {
    let mut out = Resolved::default();
    if msg.len() < 12 {
        return out;
    }
    let qd = rd16(msg, 4) as usize;
    let counts = rd16(msg, 6) as usize + rd16(msg, 8) as usize + rd16(msg, 10) as usize; // AN+NS+AR
    let mut pos = 12;
    for _ in 0..qd {
        pos = skip_name(msg, pos);
        pos += 4; // QTYPE + QCLASS
    }
    for _ in 0..counts {
        pos = skip_name(msg, pos);
        if pos + 10 > msg.len() {
            break;
        }
        let rtype = rd16(msg, pos);
        let rdlen = rd16(msg, pos + 8) as usize; // TYPE,CLASS,TTL(4),RDLENGTH
        let rdata = pos + 10;
        if rdata + rdlen > msg.len() {
            break;
        }
        match rtype {
            33 if rdlen >= 6 => out.port = Some(rd16(msg, rdata + 4)), // SRV: prio,weight,PORT,target
            1 if rdlen == 4 => {
                out.ip = Some(Ipv4Address::new(
                    msg[rdata],
                    msg[rdata + 1],
                    msg[rdata + 2],
                    msg[rdata + 3],
                ))
            }
            _ => {}
        }
        pos = rdata + rdlen;
    }
    out
}

/// 经 mDNS 发现 bridge:发 `qname` 的 PTR 查询,收 SRV+A,返回 `(IP, port)`。
/// 最多查 `rounds` 轮、每轮等 ~1s;失败返回 `None`。
pub async fn resolve(stack: Stack<'_>, qname: &[&str], rounds: u8) -> Option<(Ipv4Address, u16)> {
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buf = [0u8; 1536];
    let mut tx_buf = [0u8; 256];
    let mut sock = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    if sock
        .bind(IpListenEndpoint {
            addr: None,
            port: MDNS_PORT,
        })
        .is_err()
    {
        warn!("mDNS:bind 5353 失败");
        return None;
    }
    let _ = stack.join_multicast_group(IpAddress::Ipv4(MDNS_ADDR));

    let mut q = [0u8; 64];
    let n = build_ptr_query(&mut q, qname);
    let dst = IpEndpoint::new(IpAddress::Ipv4(MDNS_ADDR), MDNS_PORT);

    let mut recv = [0u8; 1536];
    let mut ip = None;
    let mut port = None;
    for _ in 0..rounds {
        if sock.send_to(&q[..n], dst).await.is_err() {
            continue;
        }
        // 一轮内反复收,直到拿齐 IP+port 或 1s 超时。
        while let Ok(Ok((len, _))) =
            with_timeout(Duration::from_millis(1000), sock.recv_from(&mut recv)).await
        {
            let r = parse_response(&recv[..len]);
            if r.ip.is_some() {
                ip = r.ip;
            }
            if r.port.is_some() {
                port = r.port;
            }
            if let (Some(i), Some(p)) = (ip, port) {
                info!("mDNS 命中 bridge:{i}:{p}");
                return Some((i, p));
            }
        }
    }
    None
}
