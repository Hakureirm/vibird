#!/usr/bin/env bash
# 起 Vibird bridge(本地 SenseVoice ASR)。在**你自己的终端**里跑,这样 keystroke 注入能拿到
# 「辅助功能」权限(我后台跑的进程没权限)。
#
# 用法:
#   ./scripts/run-bridge.sh                 # keystroke 模式:转写粘进**前台窗口**(说话时让目标窗口在前台)
#   ./scripts/run-bridge.sh tmux <目标>     # 注入到 tmux pane。会话必须建在固定 socket 上:
#                                           #   tmux -S /tmp/vibird-tmux.sock new -s cc   (里面跑 claude),再传 cc
#
# 首次 keystroke 模式 macOS 会弹「辅助功能」授权 → 允许你的终端(Terminal/iTerm)即可。
# 设备经 mDNS 自动发现本机 bridge,无需填 IP。
set -e
HERE="$(cd "$(dirname "$0")/.." && pwd)"
export VIBIRD_ASR_PY="${VIBIRD_ASR_PY:-/Users/hakureirm/.venvs/base/bin/python}"
export VIBIRD_ASR_SCRIPT="$HERE/scripts/asr_server.py"
export VIBIRD_ASR_LANG="${VIBIRD_ASR_LANG:-zh}"
export RUST_LOG="${RUST_LOG:-info}"
unset all_proxy http_proxy https_proxy ALL_PROXY HTTP_PROXY HTTPS_PROXY

BIN="$HERE/host/target/debug/vibird"
[ -x "$BIN" ] || { echo "先构建:cd host && cargo build"; exit 1; }

if [ "$1" = "tmux" ]; then
  exec "$BIN" serve --asr local --tmux "$2"
else
  echo "keystroke 模式:说话时让要接收文字的窗口(如 Claude Code)在前台。"
  exec "$BIN" serve --asr local --keystroke
fi
