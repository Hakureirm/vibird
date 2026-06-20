//! vibird-emote —— Vibird 表情包(`.veap`)格式。
//!
//! 固件侧只用**解析器**(no_std、零拷贝,可直接借用 mmap 的 flash);host 侧开启 `std`
//! 特性使用**写入器**(打包器)。字节布局见 `docs/agent/emote-pack-format.md`。
//!
//! 解析器对畸形输入**不 panic**:所有读取都做边界检查,越界即返回 `Error` 或让迭代器停止 ——
//! 这样固件挂载不可信的 flash 分区也安全。
#![no_std]
#![forbid(unsafe_code)]

#[cfg(any(test, feature = "std"))]
extern crate std;

/// 文件魔数。
pub const MAGIC: [u8; 4] = *b"VEAP";
/// 当前格式版本。
pub const VERSION: u16 = 1;
/// 文件头长度(字节)。
pub const HEADER_LEN: usize = 16;
/// 单个片段表项长度(字节)。
pub const CLIP_ENTRY_LEN: usize = 48;
/// 片段名最大字节数。
pub const NAME_LEN: usize = 32;
/// `loop_end` 哨兵:循环到最后一帧。
pub const LOOP_END_LAST: u16 = 0xFFFF;

/// 解析错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 数据短于必需的头/表。
    TooShort,
    /// 魔数不是 `VEAP`。
    BadMagic,
    /// 版本号不被支持。
    BadVersion,
}

#[inline]
fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
#[inline]
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(o)?,
        *b.get(o + 1)?,
        *b.get(o + 2)?,
        *b.get(o + 3)?,
    ]))
}

/// 一个已校验的表情包视图(零拷贝,借用底层字节)。
#[derive(Clone, Copy)]
pub struct Pack<'a> {
    data: &'a [u8],
    canvas_w: u16,
    canvas_h: u16,
    clip_count: u16,
}

impl<'a> Pack<'a> {
    /// 解析并校验头 + 片段表边界。
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < HEADER_LEN {
            return Err(Error::TooShort);
        }
        if data[0..4] != MAGIC {
            return Err(Error::BadMagic);
        }
        // 头长度已保证 4..16 可读。
        if u16le(data, 4).unwrap_or(0) != VERSION {
            return Err(Error::BadVersion);
        }
        let canvas_w = u16le(data, 8).unwrap_or(0);
        let canvas_h = u16le(data, 10).unwrap_or(0);
        let clip_count = u16le(data, 12).unwrap_or(0);
        // 片段表必须完整放得下。
        let table_end = HEADER_LEN + clip_count as usize * CLIP_ENTRY_LEN;
        if data.len() < table_end {
            return Err(Error::TooShort);
        }
        Ok(Pack {
            data,
            canvas_w,
            canvas_h,
            clip_count,
        })
    }

    /// 画布尺寸 `(宽, 高)`。
    pub fn canvas(&self) -> (u16, u16) {
        (self.canvas_w, self.canvas_h)
    }

    /// 片段数量。
    pub fn clip_count(&self) -> usize {
        self.clip_count as usize
    }

    /// 取第 `i` 个片段。
    pub fn clip(&self, i: usize) -> Option<Clip<'a>> {
        if i >= self.clip_count as usize {
            return None;
        }
        let base = HEADER_LEN + i * CLIP_ENTRY_LEN; // parse() 已保证落在范围内
        let e = &self.data[base..base + CLIP_ENTRY_LEN];
        let name_bytes = &e[0..NAME_LEN];
        let nlen = name_bytes.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
        let name = core::str::from_utf8(&name_bytes[..nlen]).ok()?;
        let frame_count = u16le(e, 32)?;
        let fps = u16le(e, 34)?;
        let loop_start = u16le(e, 36)?;
        let loop_end = u16le(e, 38)?;
        let flags = u16le(e, 40)?;
        let data_offset = u32le(e, 44)? as usize;
        let block = self.data.get(data_offset..)?;
        Some(Clip {
            block,
            name,
            frame_count,
            fps,
            loop_start,
            loop_end,
            flags,
        })
    }

    /// 按名字查片段。
    pub fn clip_by_name(&self, name: &str) -> Option<Clip<'a>> {
        (0..self.clip_count as usize)
            .filter_map(|i| self.clip(i))
            .find(|c| c.name == name)
    }

    /// 遍历所有片段。
    pub fn clips(&self) -> ClipIter<'a> {
        ClipIter { pack: *self, i: 0 }
    }
}

/// 片段迭代器。
pub struct ClipIter<'a> {
    pack: Pack<'a>,
    i: usize,
}
impl<'a> Iterator for ClipIter<'a> {
    type Item = Clip<'a>;
    fn next(&mut self) -> Option<Clip<'a>> {
        let c = self.pack.clip(self.i)?;
        self.i += 1;
        Some(c)
    }
}

/// 一个动画片段(如 `idle`)。
#[derive(Clone, Copy)]
pub struct Clip<'a> {
    block: &'a [u8],
    name: &'a str,
    frame_count: u16,
    fps: u16,
    loop_start: u16,
    loop_end: u16,
    flags: u16,
}
impl<'a> Clip<'a> {
    /// 逻辑名。
    pub fn name(&self) -> &'a str {
        self.name
    }
    /// 帧数。
    pub fn frame_count(&self) -> u16 {
        self.frame_count
    }
    /// 播放帧率。
    pub fn fps(&self) -> u16 {
        self.fps
    }
    /// 是否循环。
    pub fn looping(&self) -> bool {
        self.flags & 1 != 0
    }
    /// 循环起始帧。
    pub fn loop_start(&self) -> u16 {
        self.loop_start
    }
    /// 循环结束帧(已把 `0xFFFF` 哨兵解析为最后一帧)。
    pub fn loop_end(&self) -> u16 {
        if self.loop_end == LOOP_END_LAST {
            self.frame_count.saturating_sub(1)
        } else {
            self.loop_end
        }
    }
    /// 遍历帧。
    pub fn frames(&self) -> FrameIter<'a> {
        FrameIter {
            data: self.block,
            pos: 0,
            left: self.frame_count,
        }
    }
    /// 取第 `idx` 帧(顺序走查)。
    pub fn frame(&self, idx: u16) -> Option<Frame<'a>> {
        self.frames().nth(idx as usize)
    }
}

/// 帧迭代器(帧长度可变,需顺序走查)。
pub struct FrameIter<'a> {
    data: &'a [u8],
    pos: usize,
    left: u16,
}
impl<'a> Iterator for FrameIter<'a> {
    type Item = Frame<'a>;
    fn next(&mut self) -> Option<Frame<'a>> {
        if self.left == 0 {
            return None;
        }
        let rect_count = u16le(self.data, self.pos)?;
        // 计算本帧字节长度:4(rect_count + reserved)+ 各矩形(8 头 + 2*w*h 像素)。
        let mut p = self.pos.checked_add(4)?;
        for _ in 0..rect_count {
            let w = u16le(self.data, p + 4)? as usize;
            let h = u16le(self.data, p + 6)? as usize;
            let nbytes = w.checked_mul(h)?.checked_mul(2)?;
            p = p.checked_add(8)?.checked_add(nbytes)?;
            if p > self.data.len() {
                return None;
            }
        }
        let frame = Frame {
            data: self.data.get(self.pos..p)?,
        };
        self.pos = p;
        self.left -= 1;
        Some(frame)
    }
}

/// 单帧(一组矩形)。
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    data: &'a [u8], // [rect_count u16][reserved u16][rects...]
}
impl<'a> Frame<'a> {
    /// 本帧矩形数。
    pub fn rect_count(&self) -> u16 {
        u16le(self.data, 0).unwrap_or(0)
    }
    /// 遍历矩形。
    pub fn rects(&self) -> RectIter<'a> {
        RectIter {
            data: self.data,
            pos: 4,
            left: self.rect_count(),
        }
    }
}

/// 矩形迭代器。
pub struct RectIter<'a> {
    data: &'a [u8],
    pos: usize,
    left: u16,
}
impl<'a> Iterator for RectIter<'a> {
    type Item = Rect<'a>;
    fn next(&mut self) -> Option<Rect<'a>> {
        if self.left == 0 {
            return None;
        }
        let x = u16le(self.data, self.pos)?;
        let y = u16le(self.data, self.pos + 2)?;
        let w = u16le(self.data, self.pos + 4)?;
        let h = u16le(self.data, self.pos + 6)?;
        let nbytes = (w as usize).checked_mul(h as usize)?.checked_mul(2)?;
        let end = self.pos.checked_add(8)?.checked_add(nbytes)?;
        let px = self.data.get(self.pos + 8..end)?;
        self.pos = end;
        self.left -= 1;
        Some(Rect {
            x,
            y,
            w,
            h,
            pixels: px,
        })
    }
}

/// 一个脏矩形:位置 + 尺寸 + RGB565 像素(行优先,LE)。
#[derive(Clone, Copy)]
pub struct Rect<'a> {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// 像素字节:`2 * w * h` 字节,RGB565 小端。
    pub pixels: &'a [u8],
}
impl<'a> Rect<'a> {
    /// 第 `i` 个像素(RGB565)。
    pub fn pixel(&self, i: usize) -> Option<u16> {
        u16le(self.pixels, i * 2)
    }
    /// 按行优先遍历像素(RGB565)。
    pub fn pixels_rgb565(&self) -> impl Iterator<Item = u16> + '_ {
        (0..self.w as usize * self.h as usize).map(move |i| u16le(self.pixels, i * 2).unwrap_or(0))
    }
}

// ----------------------------------------------------------------------------
// 写入器(host 打包器用,需 std)
// ----------------------------------------------------------------------------

/// host 侧打包写入器。固件无需此模块。
#[cfg(feature = "std")]
pub mod build {
    use super::*;
    use std::string::String;
    use std::vec;
    use std::vec::Vec;

    /// 一个脏矩形的拥有式数据。
    pub struct RectData {
        pub x: u16,
        pub y: u16,
        pub w: u16,
        pub h: u16,
        /// RGB565 小端,长度 `2 * w * h`。
        pub pixels: Vec<u8>,
    }

    /// 一个片段定义。
    pub struct ClipDef {
        pub name: String,
        pub fps: u16,
        pub looping: bool,
        pub loop_start: u16,
        pub loop_end: u16,
        /// 每帧一组矩形;第 0 帧应为全画布。
        pub frames: Vec<Vec<RectData>>,
    }

    /// 表情包构建器。
    pub struct Builder {
        pub canvas_w: u16,
        pub canvas_h: u16,
        pub clips: Vec<ClipDef>,
    }

    impl Builder {
        /// 新建指定画布尺寸的构建器。
        pub fn new(canvas_w: u16, canvas_h: u16) -> Self {
            Self {
                canvas_w,
                canvas_h,
                clips: Vec::new(),
            }
        }

        /// 追加一个片段。
        pub fn add_clip(
            &mut self,
            name: &str,
            fps: u16,
            looping: bool,
            loop_start: u16,
            loop_end: u16,
            frames: Vec<Vec<RectData>>,
        ) -> &mut Self {
            self.clips.push(ClipDef {
                name: String::from(name),
                fps,
                looping,
                loop_start,
                loop_end,
                frames,
            });
            self
        }

        /// 序列化为 `.veap` 字节。
        pub fn to_bytes(&self) -> Vec<u8> {
            let mut out = Vec::new();
            // 头
            out.extend_from_slice(&MAGIC);
            out.extend_from_slice(&VERSION.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&self.canvas_w.to_le_bytes());
            out.extend_from_slice(&self.canvas_h.to_le_bytes());
            out.extend_from_slice(&(self.clips.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // reserved
                                                        // 片段表占位,稍后回填 data_offset
            let table_start = out.len();
            out.resize(table_start + self.clips.len() * CLIP_ENTRY_LEN, 0);
            // 帧数据
            for (ci, clip) in self.clips.iter().enumerate() {
                let data_offset = out.len() as u32;
                for frame in &clip.frames {
                    out.extend_from_slice(&(frame.len() as u16).to_le_bytes()); // rect_count
                    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
                    for r in frame {
                        out.extend_from_slice(&r.x.to_le_bytes());
                        out.extend_from_slice(&r.y.to_le_bytes());
                        out.extend_from_slice(&r.w.to_le_bytes());
                        out.extend_from_slice(&r.h.to_le_bytes());
                        out.extend_from_slice(&r.pixels);
                    }
                }
                // 回填片段表项
                let e = table_start + ci * CLIP_ENTRY_LEN;
                let nb = clip.name.as_bytes();
                let n = nb.len().min(NAME_LEN);
                out[e..e + n].copy_from_slice(&nb[..n]); // 其余 name 字节已是 0
                out[e + 32..e + 34].copy_from_slice(&(clip.frames.len() as u16).to_le_bytes());
                out[e + 34..e + 36].copy_from_slice(&clip.fps.to_le_bytes());
                out[e + 36..e + 38].copy_from_slice(&clip.loop_start.to_le_bytes());
                out[e + 38..e + 40].copy_from_slice(&clip.loop_end.to_le_bytes());
                out[e + 40..e + 42].copy_from_slice(&(clip.looping as u16).to_le_bytes());
                out[e + 44..e + 48].copy_from_slice(&data_offset.to_le_bytes());
            }
            out
        }
    }

    /// 把整帧打成一个全画布矩形(每个片段第 0 帧用)。
    pub fn full_frame(cur: &[u16], w: u16, h: u16) -> Vec<RectData> {
        let mut pixels = Vec::with_capacity(cur.len() * 2);
        for &p in cur {
            pixels.extend_from_slice(&p.to_le_bytes());
        }
        vec![RectData {
            x: 0,
            y: 0,
            w,
            h,
            pixels,
        }]
    }

    /// 计算两帧间变化的**包围盒**单矩形(v1 delta)。无变化返回空 `Vec`(该帧 0 矩形 = 保持)。
    /// `prev` / `cur` 为画布的 RGB565 像素(行优先,长度 `w*h`)。
    pub fn diff_bbox(prev: &[u16], cur: &[u16], w: u16, h: u16) -> Vec<RectData> {
        let (wu, hu) = (w as usize, h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (wu, hu, 0usize, 0usize);
        let mut any = false;
        for y in 0..hu {
            for x in 0..wu {
                let i = y * wu + x;
                if prev.get(i) != cur.get(i) {
                    any = true;
                    if x < x0 {
                        x0 = x;
                    }
                    if y < y0 {
                        y0 = y;
                    }
                    if x + 1 > x1 {
                        x1 = x + 1;
                    }
                    if y + 1 > y1 {
                        y1 = y + 1;
                    }
                }
            }
        }
        if !any {
            return Vec::new();
        }
        let (rw, rh) = (x1 - x0, y1 - y0);
        let mut pixels = Vec::with_capacity(rw * rh * 2);
        for y in y0..y1 {
            for x in x0..x1 {
                pixels.extend_from_slice(&cur[y * wu + x].to_le_bytes());
            }
        }
        vec![RectData {
            x: x0 as u16,
            y: y0 as u16,
            w: rw as u16,
            h: rh as u16,
            pixels,
        }]
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::build::*;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    #[test]
    fn roundtrip_full_and_delta() {
        let mut b = Builder::new(4, 4);
        let f0: Vec<u16> = (0..16u16).collect();
        let frame0 = full_frame(&f0, 4, 4);
        // 改 (1,1)(2,1)(1,2)(2,2) 四个像素 → 包围盒应是 (1,1) 的 2x2
        let mut f1 = f0.clone();
        // 画布宽 4 → 行1列1/2、行2列1/2 = 索引 5/6/9/10
        f1[5] = 0xABCD;
        f1[6] = 0x1234;
        f1[9] = 0x5678;
        f1[10] = 0x9ABC;
        let frame1 = diff_bbox(&f0, &f1, 4, 4);
        b.add_clip("idle", 10, true, 0, LOOP_END_LAST, vec![frame0, frame1]);
        let bytes = b.to_bytes();

        let pack = Pack::parse(&bytes).unwrap();
        assert_eq!(pack.canvas(), (4, 4));
        assert_eq!(pack.clip_count(), 1);

        let clip = pack.clip_by_name("idle").unwrap();
        assert_eq!(clip.name(), "idle");
        assert_eq!(clip.frame_count(), 2);
        assert_eq!(clip.fps(), 10);
        assert!(clip.looping());
        assert_eq!(clip.loop_end(), 1); // 哨兵 → 最后一帧

        let frames: Vec<_> = clip.frames().collect();
        assert_eq!(frames.len(), 2);

        let r0: Vec<_> = frames[0].rects().collect();
        assert_eq!(r0.len(), 1);
        assert_eq!((r0[0].x, r0[0].y, r0[0].w, r0[0].h), (0, 0, 4, 4));
        assert_eq!(r0[0].pixel(0), Some(0));
        assert_eq!(r0[0].pixel(15), Some(15));

        let r1: Vec<_> = frames[1].rects().collect();
        assert_eq!(r1.len(), 1);
        assert_eq!((r1[0].x, r1[0].y, r1[0].w, r1[0].h), (1, 1, 2, 2));
        assert_eq!(r1[0].pixel(0), Some(0xABCD));
        assert_eq!(r1[0].pixel(1), Some(0x1234));
        assert_eq!(r1[0].pixel(2), Some(0x5678));
        assert_eq!(r1[0].pixel(3), Some(0x9ABC));
    }

    #[test]
    fn unchanged_frame_has_no_rects() {
        let a: Vec<u16> = vec![1, 2, 3, 4];
        let rects = diff_bbox(&a, &a, 2, 2);
        assert!(rects.is_empty());
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(
            Pack::parse(b"XXXX000000000000"),
            Err(Error::BadMagic)
        ));
    }

    #[test]
    fn rejects_short() {
        assert!(matches!(Pack::parse(b"VE"), Err(Error::TooShort)));
    }

    #[test]
    fn truncated_frame_data_does_not_panic() {
        // 合法头 + 一个片段,但帧块被截断 —— 迭代器应安全停止,不 panic。
        let mut b = Builder::new(2, 2);
        let f0: Vec<u16> = vec![10, 20, 30, 40];
        b.add_clip("x", 5, false, 0, LOOP_END_LAST, vec![full_frame(&f0, 2, 2)]);
        let mut bytes = b.to_bytes();
        bytes.truncate(bytes.len() - 3); // 砍掉尾部像素
        let pack = Pack::parse(&bytes).unwrap();
        let clip = pack.clip(0).unwrap();
        let n = clip.frames().count(); // 不应 panic;截断帧被丢弃
        assert_eq!(n, 0);
    }
}
