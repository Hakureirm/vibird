//! Vibird device ↔ bridge wire protocol.
//!
//! These types are the single source of truth for the messages exchanged between a Vibird device
//! (e.g. the AtomS3R reference firmware) and the host bridge. The crate is `no_std + alloc` so the
//! exact same types compile into both the firmware and the host.
//!
//! Transport: WebSocket. Control messages are **JSON text frames** ([`Uplink`] / [`Downlink`]);
//! microphone audio is sent as **raw binary frames** (16 kHz mono PCM, little-endian `i16`) framed by
//! [`Uplink::AudioStart`] … binary frames … [`Uplink::AudioEnd`]. First-time provisioning happens over
//! USB serial with [`SerialCommand`] (also JSON), after which everything is network.
//!
//! AGPL-3.0-only OR commercial. See the repository `LICENSE` / `README`.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Wire-protocol version. Bumped on any breaking change to the message types.
pub const PROTOCOL_VERSION: u16 = 1;

/// The agent's state, rendered by the device as expressive animation. Owned by the bridge, pushed to
/// the device via [`Downlink::SetState`]. This enum *is* the device's "face".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentState {
    /// Nothing happening — gentle idle animation.
    Idle,
    /// User is holding to talk; capturing audio.
    Listening,
    /// Agent is reasoning (no tool running yet).
    Thinking,
    /// Agent is running a tool. `tool` is a short label (e.g. "bash", "edit") when known.
    Working { tool: Option<String> },
    /// Agent wants the user to approve/deny a tool call — the device should grab attention.
    AwaitingApproval { request_id: u32, summary: String },
    /// Turn finished successfully.
    Done,
    /// Something went wrong.
    Error { message: String },
}

/// Bridge → device (JSON text frames).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Downlink {
    /// Handshake reply to [`Uplink::Hello`].
    Welcome { protocol: u16, bridge_version: String },
    /// Update the displayed agent state (drives the animation).
    SetState(AgentState),
    /// Request a physical approval for a tool call; device replies with [`Uplink::Approval`].
    Approval {
        request_id: u32,
        summary: String,
        detail: Option<String>,
        /// How long the device should wait before auto-denying (0 = no timeout).
        timeout_ms: u32,
    },
    /// A transient notice — show only when it matters (alert-fatigue discipline).
    Notify { level: NotifyLevel, text: String },
    /// Liveness probe; device replies with [`Uplink::Pong`].
    Ping { nonce: u32 },
}

/// Device → bridge (JSON text frames). Microphone PCM travels as separate binary frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Uplink {
    /// First message after the WebSocket opens.
    Hello {
        device_id: String,
        fw_version: String,
        protocol: u16,
        caps: Caps,
    },
    /// Push-to-talk started: binary PCM frames follow until [`Uplink::AudioEnd`].
    AudioStart { sample_rate: u32, format: AudioFormat },
    /// Push-to-talk ended: the bridge finalizes ASR over the buffered audio.
    AudioEnd,
    /// A physical button event.
    Button { event: ButtonEvent },
    /// The user's answer to a [`Downlink::Approval`].
    Approval { request_id: u32, decision: Decision },
    /// An IMU gesture (optional capability).
    Gesture { kind: GestureKind },
    /// Reply to [`Downlink::Ping`].
    Pong { nonce: u32 },
}

/// Severity for [`Downlink::Notify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyLevel {
    Info,
    Success,
    Warn,
    Error,
}

/// Audio sample encoding for the binary frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    /// 16-bit signed little-endian PCM, mono.
    Pcm16Le,
}

/// Physical button events. The whole AtomS3R front face is one button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonEvent {
    Press,
    Release,
    Hold,
    DoubleTap,
}

/// A physical approval answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
}

/// IMU gestures (best-effort; device may not support all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureKind {
    Nod,
    Shake,
    Tilt,
}

/// What a device can do — sent in [`Uplink::Hello`] so the bridge adapts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    pub mic: bool,
    pub speaker: bool,
    pub display: bool,
    pub imu: bool,
}

/// First-time provisioning over USB serial (JSON, newline-delimited). After this the device talks to the
/// bridge over the network; these may also be re-issued later for re-config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum SerialCommand {
    /// Store WiFi credentials in NVS.
    Wifi { ssid: String, pass: String },
    /// Pin the bridge address, or `host = "auto"` to discover via mDNS.
    Bridge { host: String, port: u16 },
    /// Ask the device to report its current status.
    Status,
    /// Wipe NVS and reboot into the unconfigured state.
    Factory,
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn agent_state_round_trips_as_json() {
        let s = AgentState::Working { tool: Some(String::from("bash")) };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#"{"state":"working","tool":"bash"}"#);
        let back: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn uplink_hello_round_trips() {
        let h = Uplink::Hello {
            device_id: String::from("atoms3r-abc"),
            fw_version: String::from("0.1.0"),
            protocol: PROTOCOL_VERSION,
            caps: Caps { mic: true, speaker: true, display: true, imu: true },
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: Uplink = serde_json::from_str(&json).unwrap();
        // tag is present
        assert!(json.contains(r#""type":"hello""#));
        if let Uplink::Hello { device_id, .. } = back {
            assert_eq!(device_id, "atoms3r-abc");
        } else {
            panic!("wrong variant");
        }
    }
}
