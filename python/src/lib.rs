//! Vibird 的 Python 绑定:`pip install vibird` 后可在 Python 里启动桥接。
//!
//! 理想用法(ADR-0001 的"零配置灵魂"):`pip install vibird`,然后 Agent 读内置 skill 自己配设备。
//! 这里先暴露最小面:`vibird.serve(...)`(阻塞跑桥接)+ `__version__`。

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// 启动桥接(阻塞):WebSocket 服务端 + ASR + 注入 + 状态控制面。
///
/// - `port`:WS 端口(控制面 = port+1)。
/// - `tmux`:把语音转写注入到的 tmux 目标;`None` 则只记录。
/// - `asr`:`"cloud"` 读 `VIBIRD_ASR_*` 环境变量,其它走 stub。
#[pyfunction]
#[pyo3(signature = (port=8137, tmux=None, asr=None))]
fn serve(py: Python<'_>, port: u16, tmux: Option<String>, asr: Option<String>) -> PyResult<()> {
    let asr_backend = match asr.as_deref() {
        Some("cloud") => vibird_bridge::Asr::cloud_from_env()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?,
        _ => vibird_bridge::Asr::stub(),
    };
    let inject = match tmux {
        Some(t) => vibird_bridge::Inject::tmux(t),
        None => vibird_bridge::Inject::default(),
    };
    let config = vibird_bridge::Config {
        asr: asr_backend,
        inject,
    };
    // 释放 GIL 跑阻塞的桥接(永不返回,直到出错 / 被中断)。
    py.allow_threads(|| {
        let rt =
            tokio::runtime::Runtime::new().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        rt.block_on(vibird_bridge::serve(port, config))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    })
}

/// Python 模块 `vibird`。
#[pymodule]
fn vibird(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("DEFAULT_PORT", vibird_bridge::DEFAULT_PORT)?;
    m.add_function(wrap_pyfunction!(serve, m)?)?;
    Ok(())
}
