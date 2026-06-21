//! 极简 no_std WebSocket **客户端** —— 只够 Vibird 设备用,跑在任意 `embedded-io-async`
//! 的 `Read + Write` 之上(embassy-net 的 `TcpSocket` 正好实现)。
//!
//! 取舍(都为了 no_std 下尽量小且无额外依赖):
//! - **握手**用一个固定合法的 `Sec-WebSocket-Key`(只要是 16 字节的 base64,tungstenite 就收;
//!   可信局域网无需随机),且**不校验** `Sec-WebSocket-Accept`(省掉 sha1/base64 依赖)。
//! - **client→server 帧用全零掩码**:RFC 要求客户端帧置掩码位,但掩码值任意;用全零 key 时
//!   `payload XOR 0 = payload`,于是 PCM 大块**零拷贝**直接发。
//! - 自实现 exact-read(只用 `Read::read`),避免 `ReadExactError` 类型路径在不同版本的差异。
//!
//! 只实现设备需要的:发 TEXT(JSON)/ BINARY(PCM),收 TEXT/Ping/Close。

use embedded_io_async::{Read, Write};

/// WebSocket 操作码。
pub const TEXT: u8 = 0x1;
pub const BINARY: u8 = 0x2;
pub const CLOSE: u8 = 0x8;
#[allow(dead_code)] // 拆分读写后没主动回 ping;留着备用 + 文档化协议
pub const PING: u8 = 0x9;
#[allow(dead_code)]
pub const PONG: u8 = 0xA;

/// 固定 `Sec-WebSocket-Key`:base64(0x01,0x02,…,0x10)。可信局域网客户端无需随机。
const WS_KEY: &str = "AQIDBAUGBwgJCgsMDQ4PEA==";

/// WebSocket 客户端错误。
#[derive(Debug)]
pub enum WsError<E> {
    /// 底层 IO 出错。
    Io(E),
    /// 握手失败(没拿到 101)。
    Handshake,
    /// 收到的帧超过调用方缓冲区。
    FrameTooBig,
    /// 连接关闭 / EOF。
    Closed,
}

/// 精确读满 `buf`(只用 `Read::read`)。
async fn read_exact<T: Read>(io: &mut T, mut buf: &mut [u8]) -> Result<(), WsError<T::Error>> {
    while !buf.is_empty() {
        let n = io.read(buf).await.map_err(WsError::Io)?;
        if n == 0 {
            return Err(WsError::Closed);
        }
        buf = &mut buf[n..];
    }
    Ok(())
}

/// WebSocket 客户端握手(在 TCP `connect` 之后调用)。`host` 形如 `192.168.1.50:8137`。
pub async fn connect<T: Read + Write>(io: &mut T, host: &str) -> Result<(), WsError<T::Error>> {
    io.write_all(b"GET / HTTP/1.1\r\nHost: ")
        .await
        .map_err(WsError::Io)?;
    io.write_all(host.as_bytes()).await.map_err(WsError::Io)?;
    io.write_all(b"\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ")
        .await
        .map_err(WsError::Io)?;
    io.write_all(WS_KEY.as_bytes()).await.map_err(WsError::Io)?;
    io.write_all(b"\r\nSec-WebSocket-Version: 13\r\n\r\n")
        .await
        .map_err(WsError::Io)?;

    // 逐字节读响应直到 `\r\n\r\n`(逐字节 → 绝不越界吃进后续 WS 帧);校验首行是 101。
    let mut last4 = [0u8; 4];
    let mut first12 = [0u8; 12];
    let mut n = 0usize;
    loop {
        let mut b = [0u8; 1];
        read_exact(io, &mut b).await?;
        if n < 12 {
            first12[n] = b[0];
        }
        last4 = [last4[1], last4[2], last4[3], b[0]];
        n += 1;
        if n >= 4 && &last4 == b"\r\n\r\n" {
            break;
        }
        if n > 4096 {
            return Err(WsError::Handshake);
        }
    }
    if &first12 != b"HTTP/1.1 101" {
        return Err(WsError::Handshake);
    }
    Ok(())
}

/// 发一帧(client→server,全零掩码)。
async fn write_frame<T: Write>(
    io: &mut T,
    opcode: u8,
    payload: &[u8],
) -> Result<(), WsError<T::Error>> {
    let mut hdr = [0u8; 14]; // 1(fin/op)+1(mask/len)+最多 8(扩展长度)+4(掩码 key,全零)
    hdr[0] = 0x80 | opcode; // FIN=1
    let n = payload.len();
    let hi = if n < 126 {
        hdr[1] = 0x80 | (n as u8);
        2
    } else if n <= 0xFFFF {
        hdr[1] = 0x80 | 126;
        hdr[2] = (n >> 8) as u8;
        hdr[3] = n as u8;
        4
    } else {
        hdr[1] = 0x80 | 127;
        hdr[2..10].copy_from_slice(&(n as u64).to_be_bytes());
        10
    };
    // hdr[hi..hi+4] 即 4 字节掩码 key,已是全零;掩码位(0x80)已置。
    io.write_all(&hdr[..hi + 4]).await.map_err(WsError::Io)?;
    // 全零掩码 → payload 原样(零拷贝)。
    io.write_all(payload).await.map_err(WsError::Io)?;
    Ok(())
}

/// 发一个 TEXT 帧(JSON 控制消息)。
pub async fn send_text<T: Write>(io: &mut T, s: &str) -> Result<(), WsError<T::Error>> {
    write_frame(io, TEXT, s.as_bytes()).await
}

/// 发一个 BINARY 帧(PCM 音频块)。
pub async fn send_binary<T: Write>(io: &mut T, data: &[u8]) -> Result<(), WsError<T::Error>> {
    write_frame(io, BINARY, data).await
}

/// 发一个 PONG 帧(回应服务端 Ping)。拆分读写后暂未主动用,留作备用。
#[allow(dead_code)]
pub async fn send_pong<T: Write>(io: &mut T, data: &[u8]) -> Result<(), WsError<T::Error>> {
    write_frame(io, PONG, data).await
}

/// 读一帧到 `buf`,返回 `(opcode, 长度)`。服务端帧通常不带掩码;带了也照样解。
pub async fn read_frame<T: Read>(
    io: &mut T,
    buf: &mut [u8],
) -> Result<(u8, usize), WsError<T::Error>> {
    let mut h = [0u8; 2];
    read_exact(io, &mut h).await?;
    let opcode = h[0] & 0x0f;
    let masked = h[1] & 0x80 != 0;
    let mut len = (h[1] & 0x7f) as usize;
    if len == 126 {
        let mut e = [0u8; 2];
        read_exact(io, &mut e).await?;
        len = u16::from_be_bytes(e) as usize;
    } else if len == 127 {
        let mut e = [0u8; 8];
        read_exact(io, &mut e).await?;
        len = u64::from_be_bytes(e) as usize;
    }
    let mask = if masked {
        let mut m = [0u8; 4];
        read_exact(io, &mut m).await?;
        Some(m)
    } else {
        None
    };
    if len > buf.len() {
        return Err(WsError::FrameTooBig);
    }
    read_exact(io, &mut buf[..len]).await?;
    if let Some(m) = mask {
        for (i, b) in buf[..len].iter_mut().enumerate() {
            *b ^= m[i & 3];
        }
    }
    Ok((opcode, len))
}
