#!/usr/bin/env python3
"""本地 ASR 转写脚本(SenseVoiceSmall,中文为主,非自回归,M 芯片快)。

bridge 的 `--asr local` 后端调它,替代原来的 mlx-whisper 脚本。
读一个 16-bit PCM WAV(任意采样率/声道),用 FunASR 跑 SenseVoiceSmall 转写,把纯文本打到 stdout。

故意**不依赖 ffmpeg**:用标准库 wave + numpy 自己读样本成 16kHz 单声道 float32,
直接把 numpy 数组喂给 funasr(funasr 内部对 ndarray 只做 torch.from_numpy,
audio_fs==fs==16000 时不重采样、不解码,因此既不碰 ffmpeg 也不碰 torchaudio 解码路径)。

用法:  python3 scripts/asr_sensevoice.py <wav> [language]
        language 默认 "zh"(中文为主);可传 "auto"/"en"/"yue"/"ja"/"ko"。
依赖:  pip install funasr torchaudio   (torch 已在 venv;numpy 随之装上)
模型:  iic/SenseVoiceSmall,首次运行从 ModelScope 下载(~936MB),缓存到 ~/.cache/modelscope。

注意:本脚本每次调用都会加载模型(冷启 ~数秒)。若 bridge 高频按句调用,
请改用同目录的常驻服务方案(见文末 NOTE),把 AutoModel 保持常驻。
"""
import os
import sys
import wave

import numpy as np

# 离线/国内优化:强制走 ModelScope,跳过启动时的版本检查网络请求。
# (AutoModel 也接收 hub="ms" / disable_update=True,这里用环境变量做双保险。)
os.environ.setdefault("MODELSCOPE_CACHE", os.path.expanduser("~/.cache/modelscope"))

MODEL_ID = "iic/SenseVoiceSmall"


def load_wav_mono16k(path: str) -> np.ndarray:
    """读 16-bit PCM WAV → 16kHz 单声道 float32(范围 [-1,1])。不依赖 ffmpeg。"""
    with wave.open(path, "rb") as w:
        ch, sr, n = w.getnchannels(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    a = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
    if ch > 1:  # 下混成单声道
        a = a.reshape(-1, ch).mean(axis=1)
    if sr != 16000:  # SenseVoice 要 16kHz —— 简易线性抽取重采样
        idx = (np.arange(int(len(a) * 16000 / sr)) * sr / 16000).astype(int)
        a = a[idx[idx < len(a)]]
    return np.ascontiguousarray(a, dtype=np.float32)


def build_model(device: str = "cpu"):
    """构造 AutoModel。device 默认 "cpu"。

    实测(M 芯片,3s 短句):CPU 单句推理 ~0.48s,MPS ~2.08s —— **CPU 反而更快**
    (SenseVoice 是小型非自回归模型,MPS 的 kernel 启动/算子回落开销盖过收益)。
    所以这里默认 cpu;mps 仅作可选项保留(funasr 在 mps 不可用时会自动回落 cpu)。

    短句(2-5s)场景**不挂 VAD**:省掉 fsmn-vad 的加载与切分开销,延迟更低。
    trust_remote_code 不传(funasr 1.3.x 已内置 SenseVoice,走集成实现,
    免去下载/执行 model.py 的 gotcha)。
    """
    from funasr import AutoModel
    return AutoModel(
        model=MODEL_ID,
        hub="ms",              # 从 ModelScope 拉模型(国内快);默认就是 ms,显式写更稳
        device=device,         # "cpu"(最稳)或 "mps"
        disable_update=True,   # 跳过每次启动的版本检查网络请求
    )


def transcribe(model, audio: np.ndarray, language: str = "zh") -> str:
    """跑一次转写,返回去标签后的纯文本。input 直接传 numpy float32 数组。"""
    from funasr.utils.postprocess_utils import rich_transcription_postprocess
    res = model.generate(
        input=audio,           # numpy float32 16kHz 单声道,无需文件、无需 ffmpeg
        cache={},
        language=language,     # 中文为主用 "zh";混合不确定用 "auto"
        use_itn=True,          # 反文本归一化:数字/标点更规整
        batch_size_s=60,
    )
    # res[0]["text"] 形如 "<|zh|><|NEUTRAL|><|Speech|><|woitn|>实际文本"
    return rich_transcription_postprocess(res[0]["text"]).strip()


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: asr_sensevoice.py <wav> [language]", file=sys.stderr)
        return 2
    wav = sys.argv[1]
    language = sys.argv[2] if len(sys.argv) > 2 else "zh"
    audio = load_wav_mono16k(wav)
    model = build_model(device="cpu")  # CPU 在 Mac 上最稳;要试 MPS 改成 "mps"
    text = transcribe(model, audio, language=language)
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

# NOTE 常驻方案(高频按句调用时强烈推荐):
#   实测(M 芯片,模型已缓存):import torch+funasr ~2.7s + 加载权重 ~1.7s ≈ 4.3s 固定冷启,
#   而单句推理仅 ~0.48s(非自回归)。即固定开销 ≈ 推理的 9 倍。若 bridge 每句起一个进程,
#   这个冷启会彻底主导延迟。解决办法:起一个常驻 Python 小服务,AutoModel 只构造一次:
#     - 最简:一个读 stdin(每行一个 wav 路径)、把转写结果写 stdout 的循环;
#       bridge 用一个长连子进程喂路径、读结果(行协议),模型常驻内存。
#     - 或者 HTTP/Unix socket:fastapi/flask 起 /asr 接口,接收 wav 路径或 PCM 字节,
#       复用同一个 model 对象。
#   关键点都是「AutoModel 构造一次,generate 多次」,把冷启摊销掉。
