//! IMU 手势检测(纯整数逻辑,喂陀螺仪原始值)。
//!
//! 点头 = **俯仰轴**振荡(上下),摇头 = **偏航轴**振荡(左右)。一次有意的点头/摇头是
//! 「速率强烈正 → 强烈反向」的一组振荡:窗口内出现 ≥2 次过阈反向才算,噪声/单次漂移不算。
//! 阈值/轴向需真机校准(设备朝向决定俯仰是 x 还是 y;偏航通常是 z)。

use vibird_protocol::GestureKind;

const TH: i32 = 80; // 算「有意」的速率阈值(deg/s)
const NEED_REVERSALS: u8 = 2; // 确认一个手势所需的过阈反向次数
const WINDOW: u16 = 100; // 收集反向的最大采样数(~1s @100Hz)
const REFRACTORY: u16 = 30; // 触发后静默采样数(去抖)

/// ±500dps 量程:原始 i16 → deg/s(`raw * 500 / 32768`,无需 FPU)。
#[inline]
pub fn gyr_dps_500(raw: i16) -> i32 {
    (raw as i32 * 500) >> 15
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Pos,
    Neg,
}

/// 单轴手势检测状态机。
struct GestureAxis {
    phase: Phase,
    reversals: u8,
    timer: u16,
    cooldown: u16,
}

impl GestureAxis {
    const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            reversals: 0,
            timer: 0,
            cooldown: 0,
        }
    }

    /// 喂一帧速率(deg/s);确认手势时返回一次 true。
    fn update(&mut self, rate_dps: i32) -> bool {
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return false;
        }
        let dir = if rate_dps > TH {
            Phase::Pos
        } else if rate_dps < -TH {
            Phase::Neg
        } else {
            Phase::Idle
        };
        if dir == Phase::Idle {
            if self.timer > 0 {
                self.timer -= 1;
                if self.timer == 0 {
                    self.reset_seq();
                }
            }
            return false;
        }
        match self.phase {
            Phase::Idle => {
                self.phase = dir;
                self.reversals = 0;
                self.timer = WINDOW;
            }
            p if p != dir => {
                self.phase = dir;
                self.reversals += 1;
                if self.reversals >= NEED_REVERSALS {
                    self.cooldown = REFRACTORY;
                    self.reset_seq();
                    return true;
                }
            }
            _ => {}
        }
        if self.timer > 0 {
            self.timer -= 1;
            if self.timer == 0 {
                self.reset_seq();
            }
        }
        false
    }

    fn reset_seq(&mut self) {
        self.phase = Phase::Idle;
        self.reversals = 0;
        self.timer = 0;
    }
}

/// 双轴手势检测器:俯仰(点头)+ 偏航(摇头)。
pub struct Detector {
    nod: GestureAxis,
    shake: GestureAxis,
}

impl Detector {
    pub const fn new() -> Self {
        Self {
            nod: GestureAxis::new(),
            shake: GestureAxis::new(),
        }
    }

    /// 喂原始陀螺仪 `(x, y, z)`;检测到手势则返回。
    /// 轴向约定(设备屏朝上):偏航=z(摇头),俯仰=y(点头)—— 真机校准可改成 x。
    pub fn update(&mut self, _x: i16, y: i16, z: i16) -> Option<GestureKind> {
        if self.shake.update(gyr_dps_500(z)) {
            return Some(GestureKind::Shake);
        }
        if self.nod.update(gyr_dps_500(y)) {
            return Some(GestureKind::Nod);
        }
        None
    }
}
