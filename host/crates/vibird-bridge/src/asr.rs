//! ASR 后端(可插拔)。
//!
//! - `Stub`:返回固定文本,用来在没有模型 / 网络时跑通整条语音闭环。
//! - `Cloud`:OpenAI Whisper 兼容的 `/audio/transcriptions`(把 PCM 包成 WAV 后 multipart 上传)。
//! - `Local`:本地 mlx-whisper(Mac M 系),shell 出去跑 `scripts/asr_local.py`(读 WAV → 打印文本),
//!   纯本地、零网络、零云。配合 `vibird simulate` 可在没有硬件时端到端自测语音闭环。

use anyhow::{Context, Result};

/// 云端 ASR 配置(OpenAI Whisper 兼容)。
#[derive(Clone)]
pub struct CloudConfig {
    /// 例:`https://api.openai.com/v1/audio/transcriptions`
    pub endpoint: String,
    pub api_key: String,
    /// 例:`whisper-1`
    pub model: String,
    /// 可选语言提示(如 `zh`)。
    pub language: Option<String>,
}

/// 本地 ASR 配置(shell 出去跑 mlx-whisper 脚本)。
#[derive(Clone)]
pub struct LocalConfig {
    /// python 解释器。
    pub python: String,
    /// 转写脚本路径(读 WAV → 打印文本)。
    pub script: String,
    /// HF 模型(如 `mlx-community/whisper-tiny`)。
    pub model: String,
}

/// ASR 后端。
#[derive(Clone)]
pub enum Asr {
    /// 占位:返回固定文本(无需模型 / 网络)。
    Stub { canned: String },
    /// 云端 Whisper 兼容。
    Cloud(CloudConfig),
    /// 本地 mlx-whisper(经脚本)。
    Local(LocalConfig),
}

impl Default for Asr {
    fn default() -> Self {
        Asr::stub()
    }
}

impl Asr {
    /// 默认占位后端。
    pub fn stub() -> Self {
        Asr::Stub {
            canned: String::from("(语音转写占位 —— 用 --asr cloud 接入真实 ASR)"),
        }
    }

    /// 从环境变量装配云端后端:`VIBIRD_ASR_ENDPOINT` / `_KEY` / `_MODEL` / `_LANG`。
    pub fn cloud_from_env() -> Result<Self> {
        let endpoint = std::env::var("VIBIRD_ASR_ENDPOINT")
            .unwrap_or_else(|_| "https://api.openai.com/v1/audio/transcriptions".to_string());
        let api_key = std::env::var("VIBIRD_ASR_KEY").context("缺环境变量 VIBIRD_ASR_KEY")?;
        let model = std::env::var("VIBIRD_ASR_MODEL").unwrap_or_else(|_| "whisper-1".to_string());
        let language = std::env::var("VIBIRD_ASR_LANG").ok();
        Ok(Asr::Cloud(CloudConfig {
            endpoint,
            api_key,
            model,
            language,
        }))
    }

    /// 从环境变量装配本地后端:`VIBIRD_ASR_PY` / `VIBIRD_ASR_SCRIPT` / `VIBIRD_ASR_MODEL`。
    pub fn local_from_env() -> Result<Self> {
        let python = std::env::var("VIBIRD_ASR_PY").unwrap_or_else(|_| "python3".to_string());
        let script = std::env::var("VIBIRD_ASR_SCRIPT")
            .context("缺环境变量 VIBIRD_ASR_SCRIPT(本地转写脚本路径,如 scripts/asr_local.py)")?;
        let model = std::env::var("VIBIRD_ASR_MODEL")
            .unwrap_or_else(|_| "mlx-community/whisper-tiny".to_string());
        Ok(Asr::Local(LocalConfig {
            python,
            script,
            model,
        }))
    }

    /// 把单声道 16-bit PCM(给定采样率)转写成文本。
    pub async fn transcribe(&self, pcm: &[i16], sample_rate: u32) -> Result<String> {
        match self {
            Asr::Stub { canned } => Ok(canned.clone()),
            Asr::Cloud(c) => cloud_transcribe(c, pcm, sample_rate).await,
            Asr::Local(c) => local_transcribe(c, pcm, sample_rate).await,
        }
    }
}

/// 把单声道 16-bit PCM 包成 WAV 字节(44 字节头 + 数据)。
pub fn pcm_to_wav(pcm: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let byte_rate = sample_rate * 2; // 单声道 16bit:每秒字节 = 采样率 * 2
    let mut w = Vec::with_capacity(44 + pcm.len() * 2);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk 大小
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    w.extend_from_slice(&sample_rate.to_le_bytes());
    w.extend_from_slice(&byte_rate.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes()); // block align
    w.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

async fn cloud_transcribe(c: &CloudConfig, pcm: &[i16], sample_rate: u32) -> Result<String> {
    let wav = pcm_to_wav(pcm, sample_rate);
    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = reqwest::multipart::Form::new()
        .text("model", c.model.clone())
        .part("file", part);
    if let Some(lang) = &c.language {
        form = form.text("language", lang.clone());
    }
    let resp = reqwest::Client::new()
        .post(&c.endpoint)
        .bearer_auth(&c.api_key)
        .multipart(form)
        .send()
        .await
        .context("ASR 请求失败")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    anyhow::ensure!(status.is_success(), "ASR HTTP {status}:{body}");
    let v: serde_json::Value = serde_json::from_str(&body).context("ASR 响应非 JSON")?;
    Ok(v.get("text")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// 本地转写:把 PCM 写成临时 WAV,shell 出去跑 mlx-whisper 脚本,读 stdout。
async fn local_transcribe(c: &LocalConfig, pcm: &[i16], sample_rate: u32) -> Result<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let wav = pcm_to_wav(pcm, sample_rate);
    let path = std::env::temp_dir().join(format!(
        "vibird_asr_{}_{}.wav",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::write(&path, &wav).await?;
    let out = tokio::process::Command::new(&c.python)
        .arg(&c.script)
        .arg(&path)
        .arg(&c.model)
        .output()
        .await
        .context("启动本地 ASR 脚本失败")?;
    let _ = tokio::fs::remove_file(&path).await;
    anyhow::ensure!(
        out.status.success(),
        "本地 ASR 失败:{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_is_44_byte_header_plus_data() {
        let wav = pcm_to_wav(&[0i16; 8], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + 16); // 8 样本 × 2 字节
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 16); // data 大小
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            16_000
        ); // 采样率
    }

    #[tokio::test]
    async fn stub_returns_canned_text() {
        let a = Asr::Stub {
            canned: String::from("你好世界"),
        };
        assert_eq!(a.transcribe(&[1, 2, 3], 16_000).await.unwrap(), "你好世界");
    }
}
