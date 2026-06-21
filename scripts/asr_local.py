#!/usr/bin/env python3
"""本地 ASR 转写脚本(mlx-whisper,M 芯片快)—— bridge 的 `--asr local` 后端调它。

读一个 16-bit PCM WAV(任意采样率/声道),用 mlx-whisper 转写,把文本打到 stdout。
故意**不依赖 ffmpeg**:用标准库 wave + numpy 自己读样本,避免 mlx-whisper 默认的 ffmpeg 解码。

用法:  python3 scripts/asr_local.py <wav> [hf_model]
依赖:  pip install mlx-whisper   (numpy 随之装上)
"""
import sys
import wave

import numpy as np
import mlx_whisper


def load_wav_mono16k(path: str) -> np.ndarray:
    with wave.open(path, "rb") as w:
        ch, sr, n = w.getnchannels(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    a = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
    if ch > 1:  # 下混成单声道
        a = a.reshape(-1, ch).mean(axis=1)
    if sr != 16000:  # whisper 要 16kHz —— 简易重采样
        idx = (np.arange(int(len(a) * 16000 / sr)) * sr / 16000).astype(int)
        a = a[idx[idx < len(a)]]
    return a


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: asr_local.py <wav> [hf_model]", file=sys.stderr)
        return 2
    wav = sys.argv[1]
    model = sys.argv[2] if len(sys.argv) > 2 else "mlx-community/whisper-tiny"
    audio = load_wav_mono16k(wav)
    text = mlx_whisper.transcribe(audio, path_or_hf_repo=model)["text"].strip()
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
