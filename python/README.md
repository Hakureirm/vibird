# vibird (Python)

Python bindings for the [Vibird](https://github.com/Hakureirm/vibird) host bridge, built with
[maturin](https://www.maturin.rs/) from the Rust core. The eventual `pip install vibird` story
([ADR-0001](../docs/agent/adr/ADR-0001-positioning.md), the "zero-config soul"): install, then your AI agent
reads a bundled skill and sets the device up for you.

## Build (from this directory)

```bash
pip install maturin
maturin develop --release      # build + install into the current venv (dev)
# or
maturin build --release        # produce a wheel under target/wheels/
```

> The crate uses PyO3's `abi3` (stable ABI), so one wheel works across Python ≥ 3.9.

## Use

```python
import vibird
print(vibird.__version__, vibird.DEFAULT_PORT)
vibird.serve(port=8137, tmux="claude", asr="stub")   # blocks, running the bridge
#   asr="cloud" reads VIBIRD_ASR_ENDPOINT / _KEY / _MODEL / _LANG
```

## Status

Scaffold. The bridge (`serve`) is wired through PyO3. Still to bundle into the wheel: the `vibird` CLI binary
(maturin bin support) and the Claude Code skill (`claude-plugin/skills/vibird-setup/`) as package data, so a
fresh `pip install vibird` ships everything an agent needs to self-configure.

License: AGPL-3.0-or-later + commercial (see the repo `LICENSE`).
