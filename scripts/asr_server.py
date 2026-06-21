#!/usr/bin/env python3
"""常驻 SenseVoice ASR 服务:模型**只加载一次**,之后每句仅推理(~0.5s)。

bridge 起一个长连子进程,把它喂 WAV 路径、读回转写,摊销掉 ~4.3s 的冷启开销
(import + 加载权重),解决 per-call 调用每句都重载模型的延迟。

行协议(都走 stdout 的「真」句柄;funasr 的加载噪声重定向到 stderr,保证 stdout 干净):
  - 启动、模型加载完后打印一行 ``READY``;
  - 之后每读到 stdin 一行 ``<wav 路径>``,回 stdout 一行 ``<转写纯文本>``(出错回空行)。

用法:python3 scripts/asr_server.py [language]   (language 默认 zh)
依赖:pip install funasr torchaudio;模型 iic/SenseVoiceSmall(ModelScope,~893MB)。
"""
import os
import sys
import wave

import numpy as np

os.environ.setdefault("MODELSCOPE_CACHE", os.path.expanduser("~/.cache/modelscope"))
MODEL_ID = "iic/SenseVoiceSmall"

# 把 funasr 的加载/进度噪声挡进 stderr;协议输出走保存下来的「真」stdout。
_real_stdout = sys.stdout
sys.stdout = sys.stderr


def emit(s: str) -> None:
    print(s, file=_real_stdout, flush=True)


def load_wav_mono16k(path: str) -> np.ndarray:
    """读 16-bit PCM WAV → 16kHz 单声道 float32([-1,1])。不依赖 ffmpeg。"""
    with wave.open(path, "rb") as w:
        ch, sr, n = w.getnchannels(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    a = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
    if ch > 1:
        a = a.reshape(-1, ch).mean(axis=1)
    if sr != 16000:
        idx = (np.arange(int(len(a) * 16000 / sr)) * sr / 16000).astype(int)
        a = a[idx[idx < len(a)]]
    return np.ascontiguousarray(a, dtype=np.float32)


def main() -> int:
    language = sys.argv[1] if len(sys.argv) > 1 else "zh"
    from funasr import AutoModel
    from funasr.utils.postprocess_utils import rich_transcription_postprocess

    model = AutoModel(model=MODEL_ID, hub="ms", device="cpu", disable_update=True)
    emit("READY")

    for line in sys.stdin:
        path = line.strip()
        if not path:
            continue
        try:
            audio = load_wav_mono16k(path)
            res = model.generate(
                input=audio,
                cache={},
                language=language,
                use_itn=True,
                batch_size_s=60,
            )
            text = rich_transcription_postprocess(res[0]["text"]).strip()
        except Exception as e:  # noqa: BLE001 —— 单句出错不该拖垮服务
            print(f"[asr_server] error: {e}", file=sys.stderr, flush=True)
            text = ""
        emit(text.replace("\n", " "))  # 转写保持单行,匹配行协议
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
