// SNN Tick Compute Shader
// 每线程处理一个神经元
// global_id = creature_slot * MAX_NODES + node_local
//
// 同步批处理模型（C 方案）：
//   - spike_counts: GPU 端原子累加，整批 tick 内累计发放次数，末尾一次读回
//   - first_outputs: tick 0 时保存直读输出 tanh 值，末尾一次读回
//   - params.tick_index: 当前是批内第几个 tick（0-based），由 CPU 在每次 dispatch 前写入

const MAX_NODES: u32 = 64u;
const MAX_CONNS: u32 = 128u;
const MAX_CREATURES: u32 = 512u;

// 节点数据（双缓冲）
struct GpuNode {
    membrane: f32,
    decay: f32,
    threshold: f32,
    // flags: bit0=fired, bit1=input_node, bit2=output_node, bit3=direct_read
    // bits[8..15]=refractory_count, bits[16..23]=refractory_period
    flags: u32,
}

// 连接数据
struct GpuConnection {
    from_node: u32,
    to_node: u32,
    weight: f32,
    _pad: u32,
}

// 每个生物的元数据
struct GpuCreatureMeta {
    node_count: u32,
    conn_count: u32,
    input_count: u32,
    output_count: u32,
}

// CPU 在每次 dispatch 前通过 queue.write_buffer 更新
struct TickParams {
    tick_index: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> nodes_prev: array<GpuNode>;
@group(0) @binding(1) var<storage, read_write> nodes_next: array<GpuNode>;
@group(0) @binding(2) var<storage, read> connections: array<GpuConnection>;
@group(0) @binding(3) var<storage, read> creature_meta: array<GpuCreatureMeta>;
@group(0) @binding(4) var<storage, read_write> spike_counts: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> first_outputs: array<f32>;
@group(0) @binding(6) var<uniform> params: TickParams;

fn is_fired(flags: u32) -> bool {
    return (flags & 1u) != 0u;
}

fn is_input(flags: u32) -> bool {
    return (flags & 2u) != 0u;
}

fn is_output(flags: u32) -> bool {
    return (flags & 4u) != 0u;
}

fn is_direct_read(flags: u32) -> bool {
    return (flags & 8u) != 0u;
}

fn get_refractory_count(flags: u32) -> u32 {
    return (flags >> 8u) & 0xFFu;
}

fn get_refractory_period(flags: u32) -> u32 {
    return (flags >> 16u) & 0xFFu;
}

fn set_refractory_count(flags: u32, count: u32) -> u32 {
    return (flags & 0xFFFF00FFu) | ((count & 0xFFu) << 8u);
}

fn set_fired(flags: u32, fired: bool) -> u32 {
    if fired {
        return flags | 1u;
    } else {
        return flags & 0xFFFFFFFEu;
    }
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let thread_id = global_id.x;
    let creature_slot = thread_id / MAX_NODES;
    let node_local = thread_id % MAX_NODES;

    if creature_slot >= MAX_CREATURES {
        return;
    }

    let creature_meta_data = creature_meta[creature_slot];
    if node_local >= creature_meta_data.node_count {
        return;
    }

    let node_global = creature_slot * MAX_NODES + node_local;
    let prev = nodes_prev[node_global];
    var next_node = prev;

    // 输入节点：已由 CPU 侧设置好 membrane 和 fired，直接复制
    if is_input(prev.flags) {
        nodes_next[node_global] = next_node;
        return;
    }

    let ref_count = get_refractory_count(prev.flags);
    let ref_period = get_refractory_period(prev.flags);

    // 不应期
    if ref_count > 0u {
        next_node.flags = set_fired(next_node.flags, false);
        next_node.flags = set_refractory_count(next_node.flags, ref_count - 1u);
        nodes_next[node_global] = next_node;
        return;
    }

    // 膜电位衰减
    var membrane = prev.membrane * prev.decay;

    // 累加前驱信号：直读节点用 membrane*weight，脉冲节点用 weight
    let conn_base = creature_slot * MAX_CONNS;
    for (var c = 0u; c < creature_meta_data.conn_count; c++) {
        let conn = connections[conn_base + c];
        if conn.to_node == node_local {
            let from_global = creature_slot * MAX_NODES + conn.from_node;
            let from_node = nodes_prev[from_global];
            if is_fired(from_node.flags) {
                if from_node.threshold == 0.0 {
                    membrane += from_node.membrane * conn.weight;
                } else {
                    membrane += conn.weight;
                }
            }
        }
    }

    next_node.membrane = membrane;

    // 阈值判定
    if prev.threshold == 0.0 {
        // 直读模式
        next_node.flags = set_fired(next_node.flags, true);
    } else if abs(membrane) >= prev.threshold {
        // 超阈发放
        next_node.flags = set_fired(next_node.flags, true);
        next_node.membrane = 0.0;
        next_node.flags = set_refractory_count(next_node.flags, ref_period);
    } else {
        next_node.flags = set_fired(next_node.flags, false);
    }

    nodes_next[node_global] = next_node;

    // 输出节点：写入 spike_counts / first_outputs
    if is_output(prev.flags) {
        let output_idx = node_local - creature_meta_data.input_count;
        if output_idx < 7u {
            let out_global = creature_slot * 7u + output_idx;

            if is_direct_read(next_node.flags) {
                // 直读输出：tick 0 时保存 tanh 值（CPU 末尾读回）
                if params.tick_index == 0u {
                    let x = next_node.membrane;
                    let x2 = x * x;
                    first_outputs[out_global] = clamp(
                        x * (27.0 + x2) / (27.0 + 9.0 * x2),
                        -1.0,
                        1.0,
                    );
                }
            } else {
                // 脉冲输出：发放则原子累加
                if is_fired(next_node.flags) {
                    atomicAdd(&spike_counts[out_global], 1u);
                }
            }
        }
    }
}
