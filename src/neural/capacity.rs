//! 神经网络容量上限告警
//!
//! `SpikingNetwork::from_genome` 和 `GpuExecutor::upload_genome` 都会静默截断
//! 超过 `MAX_NODES` / `MAX_CONNS` 的节点和连接。本模块提供高水位告警，
//! 仅当观测到的最大值刷新历史记录时打印一次，避免日志被相同基因组刷屏。

use std::sync::atomic::{AtomicUsize, Ordering};

/// 历史观测到的最大节点数（含被截断的部分）
static MAX_OBSERVED_NODES: AtomicUsize = AtomicUsize::new(0);
/// 历史观测到的最大启用连接数（含被截断的部分）
static MAX_OBSERVED_CONNS: AtomicUsize = AtomicUsize::new(0);

/// 节点数超限告警：仅当 `nodes_len` 刷新历史最大值时打印
pub fn warn_nodes_overflow(nodes_len: usize, cap: usize, source: &str) {
    if nodes_len <= cap {
        return;
    }
    let prev = MAX_OBSERVED_NODES.fetch_max(nodes_len, Ordering::Relaxed);
    if nodes_len > prev {
        eprintln!(
            "[{}] ⚠ 节点截断: genome 有 {} 个节点，MAX_NODES={}, {} 个被丢弃 \
             (考虑上调 spiking.rs/gpu.rs/snn_tick.wgsl 的 MAX_NODES)",
            source,
            nodes_len,
            cap,
            nodes_len - cap
        );
    }
}

/// 连接数超限告警：仅当 `enabled_conns` 刷新历史最大值时打印
pub fn warn_conns_overflow(enabled_conns: usize, cap: usize, source: &str) {
    if enabled_conns <= cap {
        return;
    }
    let prev = MAX_OBSERVED_CONNS.fetch_max(enabled_conns, Ordering::Relaxed);
    if enabled_conns > prev {
        eprintln!(
            "[{}] ⚠ 连接截断: 启用连接 {} 条，MAX_CONNS={}, {} 条被丢弃 \
             (考虑上调 spiking.rs/gpu.rs/snn_tick.wgsl 的 MAX_CONNS)",
            source,
            enabled_conns,
            cap,
            enabled_conns - cap
        );
    }
}
