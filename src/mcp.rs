//! MCP (Model Context Protocol) SSE 服务器
//!
//! 通过 SSE 传输向 Claude Code 暴露模拟运行时数据。
//! 协议: JSON-RPC 2.0 over MCP SSE transport。
//!
//! ## 架构
//! - SSE handler (GET /sse): 建立 SSE 连接，分配 session_id
//! - Messages handler (POST /messages): 接收 JSON-RPC 请求，通过 broadcast 回传响应
//! - 所有工具 handler 只读访问 Arc<RwLock<SimSnapshot>>
//!
//! ## 使用模式
//! 1. set_paused(true) → 冻结世界
//! 2. 多次查询 → 结果一致
//! 3. set_paused(false) → 恢复演化

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::{self, Stream, StreamExt};
use rand::Rng;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::config::Config;
use crate::world::sim_thread::{SimCommand, SimSnapshot};
use crate::world::{Creature, EnergyParticle, TrailPoint};

// ─── 共享状态 ──────────────────────────────────────────────────────────

pub struct AppState {
    pub snapshot: Arc<RwLock<SimSnapshot>>,
    pub config: Arc<RwLock<Config>>,
    pub cmd_tx: std::sync::mpsc::Sender<SimCommand>,
    sessions: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
    pub mcp_paused: AtomicBool,
}

// ─── 自动清理 SSE 流（断开时移除 session）─────────────────────────────

struct CleanupStream<S> {
    stream: S,
    sessions: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
    session_id: String,
}

impl<S: Stream<Item = Result<Event, Infallible>> + Unpin> Stream for CleanupStream<S> {
    type Item = Result<Event, Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

impl<S> Drop for CleanupStream<S> {
    fn drop(&mut self) {
        self.sessions.write().unwrap().remove(&self.session_id);
    }
}

// ─── SSE Handler ─────────────────────────────────────────────────────

async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();

    let (tx, rx) = broadcast::channel::<String>(64);
    state
        .sessions
        .write()
        .unwrap()
        .insert(session_id.clone(), tx);

    let sessions = state.sessions.clone();
    let sid = session_id.clone();
    let endpoint = format!("/messages?session_id={}", session_id);

    // 单 unfold：第一帧 = endpoint，后续 = broadcast 转发
    let stream = Box::pin(stream::unfold(
        (rx, false, sid, sessions, endpoint),
        |(mut rx, sent_endpoint, sid, sessions, endpoint)| async move {
            if !sent_endpoint {
                let event = Event::default().event("endpoint").data(endpoint.clone());
                Some((
                    Ok::<_, Infallible>(event),
                    (rx, true, sid, sessions, endpoint),
                ))
            } else {
                loop {
                    match rx.recv().await {
                        Ok(msg) => {
                            let event = Event::default().event("message").data(msg);
                            return Some((
                                Ok::<_, Infallible>(event),
                                (rx, true, sid, sessions, endpoint),
                            ));
                        }
                        Err(broadcast::error::RecvError::Closed) => return None,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            eprintln!("[mcp] SSE lagged, skipped {} messages", n);
                            continue;
                        }
                    }
                }
            }
        },
    ));

    let cleanup = CleanupStream {
        stream,
        sessions: state.sessions.clone(),
        session_id,
    };

    Sse::new(cleanup).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

// ─── Messages Handler (POST) ─────────────────────────────────────────

#[derive(serde::Deserialize)]
struct SessionQuery {
    session_id: String,
}

async fn messages_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    Json(body): Json<Value>,
) -> impl axum::response::IntoResponse {
    let tx = {
        let sessions = state.sessions.read().unwrap();
        sessions.get(&query.session_id).cloned()
    };

    let sender = match tx {
        Some(tx) => tx,
        None => return (axum::http::StatusCode::ACCEPTED, "{}"),
    };

    // 异步处理请求，不阻塞 POST handler
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let response = process_jsonrpc(&body, &state_clone);
        let json_str = serde_json::to_string(&response).unwrap_or_default();
        let _ = sender.send(json_str);
    });

    (axum::http::StatusCode::ACCEPTED, "{}")
}

// ─── JSON-RPC 分发器 ─────────────────────────────────────────────────

fn process_jsonrpc(request: &Value, state: &Arc<AppState>) -> Value {
    let id = request.get("id");
    let method = match request.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return jsonrpc_error(id, -32600, "Invalid Request: missing method"),
    };

    let params = request.get("params").unwrap_or(&json!(null));

    let result = match method {
        "initialize" => handle_initialize(),
        "notifications/initialized" => json!({}),
        "notifications/cancelled" => json!({}),
        "ping" => json!({}),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(params, state),
        "resources/list" => handle_resources_list(),
        "resources/read" => handle_resources_read(params, state),
        _ => return jsonrpc_error(id, -32601, &format!("Method not found: {}", method)),
    };

    let mut response = json!({
        "jsonrpc": "2.0",
        "result": result
    });
    if let Some(id_val) = id {
        response["id"] = id_val.clone();
    } else {
        response["id"] = json!(null);
    }
    response
}

fn jsonrpc_error(id: Option<&Value>, code: i64, message: &str) -> Value {
    let mut err = json!({
        "jsonrpc": "2.0",
        "error": { "code": code, "message": message }
    });
    if let Some(id_val) = id {
        err["id"] = id_val.clone();
    } else {
        err["id"] = json!(null);
    }
    err
}

// ─── MCP Initialize ─────────────────────────────────────────────────

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {},
            "resources": {}
        },
        "serverInfo": {
            "name": "cell-world-mcp",
            "version": "2.3.0"
        }
    })
}

// ─── 工具列表 ─────────────────────────────────────────────────────────

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "get_stats",
                "description": "种群统计概览：生物/粒子/痕迹数量、能量、世代、死亡年龄、优势种、族群",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_performance",
                "description": "性能分解：perceive/SNN/action 各阶段耗时",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_creatures",
                "description": "按空间/属性筛选生物，支持排序和分页",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x_min": { "type": "number" }, "x_max": { "type": "number" },
                        "y_min": { "type": "number" }, "y_max": { "type": "number" },
                        "center_x": { "type": "number" }, "center_y": { "type": "number" },
                        "radius": { "type": "number" },
                        "clan_hash": { "type": "number" },
                        "min_energy": { "type": "number" }, "max_energy": { "type": "number" },
                        "min_age": { "type": "number" }, "max_age": { "type": "number" },
                        "min_generation": { "type": "number" }, "max_generation": { "type": "number" },
                        "min_speed": { "type": "number" }, "max_speed": { "type": "number" },
                        "min_follow": { "type": "number" }, "max_follow": { "type": "number" },
                        "min_light": { "type": "number" }, "max_light": { "type": "number" },
                        "min_nodes": { "type": "number" }, "max_nodes": { "type": "number" },
                        "min_connections": { "type": "number" }, "max_connections": { "type": "number" },
                        "alive_only": { "type": "boolean" },
                        "sort_by": { "type": "string", "enum": ["id","energy","age","generation","speed"] },
                        "sort_desc": { "type": "boolean" },
                        "page": { "type": "number" }, "page_size": { "type": "number" }
                    }
                }
            },
            {
                "name": "get_creature",
                "description": "单个生物完整详情，含基因组",
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": { "type": "number" } },
                    "required": ["id"]
                }
            },
            {
                "name": "get_neighbors",
                "description": "查询某生物附近的其他生物",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "creature_id": { "type": "number" },
                        "radius": { "type": "number" },
                        "page": { "type": "number" }, "page_size": { "type": "number" }
                    },
                    "required": ["creature_id"]
                }
            },
            {
                "name": "get_energy_particles",
                "description": "查询能量粒子",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x_min": { "type": "number" }, "x_max": { "type": "number" },
                        "y_min": { "type": "number" }, "y_max": { "type": "number" },
                        "center_x": { "type": "number" }, "center_y": { "type": "number" },
                        "radius": { "type": "number" },
                        "lava_only": { "type": "boolean" },
                        "min_energy": { "type": "number" }, "max_energy": { "type": "number" },
                        "page": { "type": "number" }, "page_size": { "type": "number" }
                    }
                }
            },
            {
                "name": "get_trails",
                "description": "查询痕迹点",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x_min": { "type": "number" }, "x_max": { "type": "number" },
                        "y_min": { "type": "number" }, "y_max": { "type": "number" },
                        "center_x": { "type": "number" }, "center_y": { "type": "number" },
                        "radius": { "type": "number" },
                        "clan_hash": { "type": "number" },
                        "min_energy": { "type": "number" }, "max_energy": { "type": "number" },
                        "page": { "type": "number" }, "page_size": { "type": "number" }
                    }
                }
            },
            {
                "name": "get_clans",
                "description": "族群分布详情",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_dominant_species",
                "description": "优势种详情",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_config",
                "description": "当前全部配置",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_terrain_info",
                "description": "地形信息",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x": { "type": "number" }, "y": { "type": "number" }
                    }
                }
            },
            {
                "name": "set_paused",
                "description": "暂停/继续模拟。暂停后快照冻结，多次查询结果一致",
                "inputSchema": {
                    "type": "object",
                    "properties": { "paused": { "type": "boolean" } },
                    "required": ["paused"]
                }
            },
            {
                "name": "validate_brain_topology",
                "description": "校验生物脑拓扑约束（block!=0、跨半球同源、前馈方向）。无 id 则校验全部生物。返回违规列表",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "number", "description": "可选：单个生物 ID。省略则全部校验" }
                    }
                }
            }
        ]
    })
}

// ─── 工具调用分发 ──────────────────────────────────────────────────────

fn handle_tools_call(params: &Value, state: &Arc<AppState>) -> Value {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let empty = json!({});
    let args = params.get("arguments").unwrap_or(&empty);

    match name {
        "get_stats" => call_get_stats(state),
        "get_performance" => call_get_performance(state),
        "get_creatures" => call_get_creatures(state, args),
        "get_creature" => call_get_creature(state, args),
        "get_neighbors" => call_get_neighbors(state, args),
        "get_energy_particles" => call_get_energy_particles(state, args),
        "get_trails" => call_get_trails(state, args),
        "get_clans" => call_get_clans(state),
        "get_dominant_species" => call_get_dominant_species(state),
        "get_config" => call_get_config(state),
        "get_terrain_info" => call_get_terrain_info(state, args),
        "set_paused" => call_set_paused(state, args),
        "validate_brain_topology" => call_validate_brain_topology(state, args),
        _ => json!({
            "content": [{"type": "text", "text": format!("Unknown tool: {}", name)}],
            "isError": true
        }),
    }
}

// ─── 工具实现 ─────────────────────────────────────────────────────────

fn call_get_stats(state: &Arc<AppState>) -> Value {
    let snap = state.snapshot.read().unwrap();
    let ws = &snap.world_stats;

    let top_clans: Vec<Value> = ws
        .top_clans
        .iter()
        .take(5)
        .map(|(hash, count, avg_nodes)| {
            json!({
                "hash": hash,
                "count": count,
                "ratio": *count as f64 / ws.creature_count.max(1) as f64,
                "avg_nodes": avg_nodes
            })
        })
        .collect();

    let dominant = ws.dominant_candidate.as_ref().map(|d| {
        json!({
            "genome_hash": d.genome.hash(),
            "avg_energy": d.avg_energy,
            "avg_age": d.avg_age,
            "max_generation": d.max_generation,
            "population_ratio": d.population_ratio,
            "score": d.score
        })
    });

    let data = json!({
        "time": snap.time,
        "creature_count": ws.creature_count,
        "energy_particle_count": ws.energy_particle_count,
        "trail_count": ws.trail_count,
        "total_energy": ws.total_energy,
        "creature_energy": ws.creature_energy,
        "theoretical_energy": ws.theoretical_energy,
        "max_generation": ws.max_generation,
        "avg_energy": ws.avg_energy,
        "clan_count": ws.clan_count,
        "top_clans": top_clans,
        "dominant_species": dominant,
        "death_age_stats": {
            "count": ws.death_age_stats.count,
            "avg": ws.death_age_stats.avg,
            "median": ws.death_age_stats.median,
            "max": ws.death_age_stats.max,
            "min": ws.death_age_stats.min
        },
        "action_counts": ws.action_counts,
        "reward_counts": ws.reward_counts,
        "volcano_countdown": snap.volcano_countdown,
        "paused": state.mcp_paused.load(Ordering::Relaxed)
    });

    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&data).unwrap_or_default()
        }]
    })
}

fn call_get_performance(state: &Arc<AppState>) -> Value {
    let snap = state.snapshot.read().unwrap();
    let p = &snap.perf_stats;

    let data = json!({
        "perceive_ms": p.perceive_ms,
        "snn_ms": p.snn_ms,
        "actions_ms": p.actions_ms,
        "spatial_ms": p.spatial_ms,
        "total_ms": p.total_ms,
        "creature_count": p.creature_count,
        "avg_compute_ns": p.avg_compute_ns
    });

    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&data).unwrap_or_default()
        }]
    })
}

fn call_get_creatures(state: &Arc<AppState>, args: &Value) -> Value {
    let snap = state.snapshot.read().unwrap();
    let alive_only = args
        .get("alive_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let mut filtered: Vec<&Creature> = snap
        .creatures
        .iter()
        .filter(|c| !alive_only || c.alive)
        .filter(|c| filter_creature(c, args))
        .collect();

    // 空间过滤（圆形优先于矩形）
    if let Some((cx, cy, r)) = parse_circle(args) {
        filtered.retain(|c| dist_sq(c.x, c.y, cx, cy) <= r * r);
    } else if let Some((x1, x2, y1, y2)) = parse_rect(args) {
        filtered.retain(|c| c.x >= x1 && c.x <= x2 && c.y >= y1 && c.y <= y2);
    }

    // 排序
    let sort_by = args.get("sort_by").and_then(|v| v.as_str()).unwrap_or("id");
    let desc = args
        .get("sort_desc")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    sort_creatures(&mut filtered, sort_by, desc);

    // 分页
    let (page, page_size) = parse_page(args);
    let total = filtered.len();
    let pages = if page_size == 0 {
        1
    } else {
        (total as u32 + page_size - 1) / page_size
    };

    let start = page as usize * page_size as usize;
    let items: Vec<Value> = filtered
        .iter()
        .skip(start)
        .take(page_size as usize)
        .map(|c| creature_summary(c))
        .collect();

    let data = json!({
        "items": items, "total_count": total,
        "page": page, "page_size": page_size, "total_pages": pages
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

fn call_get_creature(state: &Arc<AppState>, args: &Value) -> Value {
    let id = match args.get("id").and_then(|v| v.as_u64()) {
        Some(id) => id,
        None => return err_text("Missing required parameter: id"),
    };

    let snap = state.snapshot.read().unwrap();
    let creature = match snap.creatures.iter().find(|c| c.id == id) {
        Some(c) => c,
        None => return err_text(&format!("Creature not found: {}", id)),
    };

    // 计算 vision_range 内活邻居数（用于孤独判定）
    let vision_range = state.config.read().unwrap().vision_range;
    let neighbor_count_in_vision = snap
        .creatures
        .iter()
        .filter(|c| c.id != creature.id && c.alive)
        .filter(|c| dist_sq(c.x, c.y, creature.x, creature.y) <= vision_range * vision_range)
        .count();
    let is_lonely = neighbor_count_in_vision == 0;

    let genome = &creature.genome;
    let nodes: Vec<Value> = genome
        .nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "neuron_type": format!("{:?}", n.node_type),
                "block_id": format!("{:?}", n.node_type),
                "layer": format!("{:?}", n.layer),
                "decay": n.decay,
                "threshold": n.threshold,
                "refractory_period": n.refractory_period
            })
        })
        .collect();

    let connections: Vec<Value> = genome
        .connections
        .iter()
        .map(|c| {
            json!({
                "in_node": c.in_node,
                "out_node": c.out_node,
                "weight": c.weight,
                "enabled": c.enabled
            })
        })
        .collect();

    let conn_projs: Value = {
        let mut map = serde_json::Map::new();
        for (blk, probs) in &genome.conn_probs {
            map.insert(
                blk.to_string(),
                json!({
                    "proc": probs.proc,
                    "out": probs.out,
                    "target_pref": probs.target_pref
                }),
            );
        }
        Value::Object(map)
    };

    let data = json!({
        "id": creature.id,
        "x": creature.x, "y": creature.y,
        "energy": creature.energy, "age": creature.age,
        "heading": creature.heading, "generation": creature.generation,
        "alive": creature.alive, "parent_id": creature.parent_id,
        "clan_hash": creature.clan_hash,
        "current_speed": creature.current_speed,
        "follow_level": creature.follow_level,
        "neighbor_count_in_vision": neighbor_count_in_vision,
        "is_lonely": is_lonely,
        "heading_persist": (creature.smoothed_dir_x * creature.smoothed_dir_x
            + creature.smoothed_dir_y * creature.smoothed_dir_y).sqrt(),
        "light_intensity": creature.light_intensity,
        "perception_cache": creature.perception_cache,
        "last_outputs": creature.last_outputs,
        "mouth_cooldown_timer": creature.mouth_cooldown_timer,
        "physio": {
            "pleasure_energy": creature.physio.pleasure_energy,
            "pleasure_trail": creature.physio.pleasure_trail,
            "pleasure_group": creature.physio.pleasure_group
        },
        "node_count": genome.nodes.len(),
        "connection_count": genome.connections.len(),
        "genome": {
            "hash": genome.hash(),
            "nodes": nodes,
            "connections": connections,
            "physio": {
                "pleasure_energy_sensitivity": genome.physio.pleasure_energy_sensitivity,
                "pleasure_trail_sensitivity": genome.physio.pleasure_trail_sensitivity,
                "pleasure_group_sensitivity": genome.physio.pleasure_group_sensitivity
            },
            "conn_probs": conn_projs,
            "maturation_time": genome.maturation_time
        }
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

fn call_get_neighbors(state: &Arc<AppState>, args: &Value) -> Value {
    let id = match args.get("creature_id").and_then(|v| v.as_u64()) {
        Some(id) => id,
        None => return err_text("Missing required parameter: creature_id"),
    };

    let snap = state.snapshot.read().unwrap();
    let target = match snap.creatures.iter().find(|c| c.id == id) {
        Some(c) => c,
        None => return err_text(&format!("Creature not found: {}", id)),
    };

    let radius = args.get("radius").and_then(|v| v.as_f64()).unwrap_or(100.0);
    let (page, page_size) = parse_page(args);

    let mut neighbors: Vec<&Creature> = snap
        .creatures
        .iter()
        .filter(|c| c.id != id && c.alive)
        .filter(|c| dist_sq(c.x, c.y, target.x, target.y) <= radius * radius)
        .collect();

    neighbors.sort_by(|a, b| {
        dist_sq(a.x, a.y, target.x, target.y)
            .partial_cmp(&dist_sq(b.x, b.y, target.x, target.y))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total = neighbors.len();
    let pages = if page_size == 0 {
        1
    } else {
        (total as u32 + page_size - 1) / page_size
    };
    let start = page as usize * page_size as usize;
    let items: Vec<Value> = neighbors
        .iter()
        .skip(start)
        .take(page_size as usize)
        .map(|c| creature_summary(c))
        .collect();

    let data = json!({
        "creature_id": id, "radius": radius,
        "items": items, "total_count": total,
        "page": page, "page_size": page_size, "total_pages": pages
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

fn call_get_energy_particles(state: &Arc<AppState>, args: &Value) -> Value {
    let snap = state.snapshot.read().unwrap();
    let lava_only = args
        .get("lava_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut filtered: Vec<&EnergyParticle> = snap
        .energy_particles
        .iter()
        .filter(|p| p.alive)
        .filter(|p| !lava_only || p.lava)
        .filter(|p| {
            if let Some(v) = args.get("min_energy").and_then(|v| v.as_f64()) {
                if p.energy < v {
                    return false;
                }
            }
            if let Some(v) = args.get("max_energy").and_then(|v| v.as_f64()) {
                if p.energy > v {
                    return false;
                }
            }
            true
        })
        .collect();

    apply_spatial_filter(&mut filtered, args, |p| (p.x, p.y));
    let (page, page_size) = parse_page(args);
    let total = filtered.len();
    let pages = if page_size == 0 {
        1
    } else {
        (total as u32 + page_size - 1) / page_size
    };
    let start = page as usize * page_size as usize;

    let items: Vec<Value> = filtered
        .iter()
        .skip(start)
        .take(page_size as usize)
        .map(|p| {
            json!({
                "id": p.id, "x": p.x, "y": p.y,
                "energy": p.energy, "initial_energy": p.initial_energy,
                "lava": p.lava, "chain_depth": p.chain_depth,
                "source": "Volcano"
            })
        })
        .collect();

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&json!({
            "items": items, "total_count": total,
            "page": page, "page_size": page_size, "total_pages": pages
        })).unwrap_or_default() }]
    })
}

fn call_get_trails(state: &Arc<AppState>, args: &Value) -> Value {
    let snap = state.snapshot.read().unwrap();

    let mut filtered: Vec<&TrailPoint> = snap
        .trail_points
        .iter()
        .filter(|t| t.alive)
        .filter(|t| {
            if let Some(v) = args.get("clan_hash").and_then(|v| v.as_u64()) {
                if t.clan_hash != v {
                    return false;
                }
            }
            if let Some(v) = args.get("min_energy").and_then(|v| v.as_f64()) {
                if t.energy < v {
                    return false;
                }
            }
            if let Some(v) = args.get("max_energy").and_then(|v| v.as_f64()) {
                if t.energy > v {
                    return false;
                }
            }
            true
        })
        .collect();

    apply_spatial_filter(&mut filtered, args, |t| (t.x, t.y));
    let (page, page_size) = parse_page(args);
    let total = filtered.len();
    let pages = if page_size == 0 {
        1
    } else {
        (total as u32 + page_size - 1) / page_size
    };
    let start = page as usize * page_size as usize;

    let items: Vec<Value> = filtered
        .iter()
        .skip(start)
        .take(page_size as usize)
        .map(|t| {
            json!({
                "x": t.x, "y": t.y,
                "energy": t.energy, "initial_energy": t.initial_energy,
                "clan_hash": t.clan_hash, "creator_id": t.creator_id, "age": t.age
            })
        })
        .collect();

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&json!({
            "items": items, "total_count": total,
            "page": page, "page_size": page_size, "total_pages": pages
        })).unwrap_or_default() }]
    })
}

fn call_get_clans(state: &Arc<AppState>) -> Value {
    let snap = state.snapshot.read().unwrap();
    let ws = &snap.world_stats;

    let mut clan_data: HashMap<u64, (usize, f64, f64, usize, u64)> = HashMap::new();
    for c in &snap.creatures {
        if !c.alive {
            continue;
        }
        let entry = clan_data
            .entry(c.clan_hash)
            .or_insert((0, 0.0, 0.0, 0, c.id));
        entry.0 += 1;
        entry.1 += c.energy;
        entry.2 += c.age;
        entry.3 = entry.3.max(c.generation);
    }

    let mut clans: Vec<Value> = clan_data
        .into_iter()
        .map(|(hash, (count, total_e, total_a, max_gen, rep_id))| {
            json!({
                "hash": hash,
                "count": count,
                "ratio": count as f64 / ws.creature_count.max(1) as f64,
                "avg_energy": total_e / count as f64,
                "avg_age": total_a / count as f64,
                "max_generation": max_gen,
                "representative_id": rep_id
            })
        })
        .collect();

    clans.sort_by(|a, b| {
        b["count"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["count"].as_u64().unwrap_or(0))
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&clans).unwrap_or_default() }]
    })
}

fn call_get_dominant_species(state: &Arc<AppState>) -> Value {
    let snap = state.snapshot.read().unwrap();
    let candidate = match &snap.world_stats.dominant_candidate {
        Some(d) => d,
        None => {
            return json!({
                "content": [{ "type": "text", "text": "null" }]
            });
        }
    };

    let nodes: Vec<Value> = candidate
        .genome
        .nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "neuron_type": format!("{:?}", n.node_type),
                "layer": format!("{:?}", n.layer),
                "decay": n.decay,
                "threshold": n.threshold,
                "refractory_period": n.refractory_period
            })
        })
        .collect();

    let connections: Vec<Value> = candidate
        .genome
        .connections
        .iter()
        .map(|c| {
            json!({
                "in_node": c.in_node, "out_node": c.out_node,
                "weight": c.weight, "enabled": c.enabled
            })
        })
        .collect();

    let data = json!({
        "genome_hash": candidate.genome.hash(),
        "avg_energy": candidate.avg_energy,
        "avg_age": candidate.avg_age,
        "max_generation": candidate.max_generation,
        "population_ratio": candidate.population_ratio,
        "score": candidate.score,
        "genome": {
            "hash": candidate.genome.hash(),
            "nodes": nodes,
            "connections": connections,
            "physio": {
                "pleasure_energy_sensitivity": candidate.genome.physio.pleasure_energy_sensitivity,
                "pleasure_trail_sensitivity": candidate.genome.physio.pleasure_trail_sensitivity,
                "pleasure_group_sensitivity": candidate.genome.physio.pleasure_group_sensitivity
            },
            "maturation_time": candidate.genome.maturation_time
        }
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

fn call_get_config(state: &Arc<AppState>) -> Value {
    let c = state.config.read().unwrap();
    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&*c).unwrap_or_default() }]
    })
}

fn call_get_terrain_info(state: &Arc<AppState>, args: &Value) -> Value {
    let snap = state.snapshot.read().unwrap();
    let t = &snap.terrain;

    // 有点坐标：查单点
    if let (Some(x), Some(y)) = (
        args.get("x").and_then(|v| v.as_f64()),
        args.get("y").and_then(|v| v.as_f64()),
    ) {
        let height = t.height_at(x, y);
        let data = json!({
            "is_generated": t.is_generated(),
            "x": x, "y": y,
            "height_at_point": height
        });
        return json!({
            "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
        });
    }

    // 无坐标：返回概览
    let chunk_count = t.chunks.len();
    let data = json!({
        "is_generated": t.is_generated(),
        "generated_radius": t.generated_radius,
        "chunk_count": chunk_count,
        "min_height": t.min_h,
        "max_height": t.max_h
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

fn call_set_paused(state: &Arc<AppState>, args: &Value) -> Value {
    let paused = match args.get("paused").and_then(|v| v.as_bool()) {
        Some(p) => p,
        None => return err_text("Missing required parameter: paused"),
    };

    let was_paused = state.mcp_paused.load(Ordering::Relaxed);
    state.mcp_paused.store(paused, Ordering::Relaxed);

    let cmd = if paused {
        SimCommand::Pause
    } else {
        SimCommand::Resume
    };
    let _ = state.cmd_tx.send(cmd);

    let time = state.snapshot.read().unwrap().time;

    let data = json!({ "ok": true, "was_paused": was_paused, "time": time });
    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

/// 校验单个生物 genome 拓扑约束，返回违规列表
fn check_topology_violations(creature: &Creature) -> Vec<Value> {
    use crate::neural::Genome;
    let mut violations = Vec::new();
    let genome = &creature.genome;

    // 1. 节点 block 不应为 0（block 0 已弃用）
    for node in &genome.nodes {
        let blk = Genome::node_block(node);
        if blk == 0 {
            violations.push(json!({
                "type": "block_zero_used",
                "node_id": node.id,
                "detail": format!("{:?}", node.node_type)
            }));
        }
    }

    // 2. 连接拓扑约束
    for conn in &genome.connections {
        if !conn.enabled {
            continue;
        }
        let from_node = genome.nodes.iter().find(|n| n.id == conn.in_node);
        let to_node = genome.nodes.iter().find(|n| n.id == conn.out_node);
        let (Some(fnode), Some(tnode)) = (from_node, to_node) else {
            violations.push(json!({
                "type": "dangling_connection",
                "in_node": conn.in_node,
                "out_node": conn.out_node
            }));
            continue;
        };
        let f = Genome::node_block(fnode);
        let t = Genome::node_block(tnode);

        // 跨半球必须同源（|from| == |to|）
        let cross_hemi = (f > 0) != (t > 0);
        if cross_hemi && f.unsigned_abs() != t.unsigned_abs() {
            violations.push(json!({
                "type": "cross_hemisphere_not_homotopic",
                "in_node": conn.in_node,
                "out_node": conn.out_node,
                "from_block": f,
                "to_block": t
            }));
        }
    }

    violations
}

fn call_validate_brain_topology(state: &Arc<AppState>, args: &Value) -> Value {
    let snap = state.snapshot.read().unwrap();
    let id_filter = args.get("id").and_then(|v| v.as_u64());

    let mut report = Vec::new();
    let mut total_checked = 0usize;
    let mut total_violations = 0usize;

    for creature in snap.creatures.iter() {
        if let Some(id) = id_filter {
            if creature.id != id {
                continue;
            }
        }
        total_checked += 1;
        let v = check_topology_violations(creature);
        if !v.is_empty() {
            total_violations += v.len();
            report.push(json!({
                "creature_id": creature.id,
                "violation_count": v.len(),
                "violations": v
            }));
        }
    }

    let data = json!({
        "checked": total_checked,
        "creatures_with_violations": report.len(),
        "total_violations": total_violations,
        "report": report
    });

    json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&data).unwrap_or_default() }]
    })
}

// ─── 资源 ─────────────────────────────────────────────────────────────

fn handle_resources_list() -> Value {
    json!({
        "resources": [
            { "uri": "cellworld://stats", "name": "种群统计", "description": "种群统计概览" },
            { "uri": "cellworld://clans", "name": "族群分布", "description": "族群详情" },
            { "uri": "cellworld://config", "name": "配置参数", "description": "当前配置" },
            { "uri": "cellworld://performance", "name": "性能", "description": "性能分解" },
            { "uri": "cellworld://dominant", "name": "优势种", "description": "优势种详情" },
            { "uri": "cellworld://terrain", "name": "地形", "description": "地形信息" }
        ]
    })
}

fn handle_resources_read(params: &Value, state: &Arc<AppState>) -> Value {
    let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or("");

    let (result, mime) = match uri {
        "cellworld://stats" => (call_get_stats(state), "application/json"),
        "cellworld://clans" => (call_get_clans(state), "application/json"),
        "cellworld://config" => (call_get_config(state), "application/json"),
        "cellworld://performance" => (call_get_performance(state), "application/json"),
        "cellworld://dominant" => (call_get_dominant_species(state), "application/json"),
        "cellworld://terrain" => (call_get_terrain_info(state, &json!({})), "application/json"),
        _ => {
            return json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": format!("Resource not found: {}", uri)
                }],
                "isError": true
            });
        }
    };

    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    json!({
        "contents": [{
            "uri": uri,
            "mimeType": mime,
            "text": text
        }]
    })
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────

fn creature_summary(c: &Creature) -> Value {
    json!({
        "id": c.id,
        "x": c.x, "y": c.y,
        "energy": c.energy, "age": c.age,
        "heading": c.heading, "generation": c.generation,
        "clan_hash": c.clan_hash,
        "current_speed": c.current_speed,
        "follow_level": c.follow_level,
        "light_intensity": c.light_intensity,
        "node_count": c.genome.nodes.len(),
        "connection_count": c.genome.connections.len(),
        "last_outputs": c.last_outputs
    })
}

fn filter_creature(c: &Creature, args: &Value) -> bool {
    macro_rules! check {
        ($key:ident, $field:expr, $cmp:ident) => {
            if let Some(v) = args.get(stringify!($key)).and_then(|v| v.as_f64()) {
                let f = $field as f64;
                if !(f.$cmp(&v)) {
                    return false;
                }
            }
        };
        (int $key:ident, $field:expr, $cmp:ident) => {
            if let Some(v) = args.get(stringify!($key)).and_then(|v| v.as_u64()) {
                let f = $field as u64;
                if !(f.$cmp(&v)) {
                    return false;
                }
            }
        };
    }

    if let Some(v) = args.get("clan_hash").and_then(|v| v.as_u64()) {
        if c.clan_hash != v {
            return false;
        }
    }

    check!(min_energy, c.energy, ge);
    check!(max_energy, c.energy, le);
    check!(min_age, c.age, ge);
    check!(max_age, c.age, le);
    check!(min_speed, c.current_speed, ge);
    check!(max_speed, c.current_speed, le);
    check!(min_follow, c.follow_level, ge);
    check!(max_follow, c.follow_level, le);
    check!(min_light, c.light_intensity, ge);
    check!(max_light, c.light_intensity, le);
    check!(int min_generation, c.generation, ge);
    check!(int max_generation, c.generation, le);
    check!(int min_nodes, c.genome.nodes.len(), ge);
    check!(int max_nodes, c.genome.nodes.len(), le);
    check!(int min_connections, c.genome.connections.len(), ge);
    check!(int max_connections, c.genome.connections.len(), le);

    true
}

fn parse_circle(args: &Value) -> Option<(f64, f64, f64)> {
    let cx = args.get("center_x")?.as_f64()?;
    let cy = args.get("center_y")?.as_f64()?;
    let r = args.get("radius")?.as_f64()?;
    Some((cx, cy, r))
}

fn parse_rect(args: &Value) -> Option<(f64, f64, f64, f64)> {
    Some((
        args.get("x_min")?.as_f64()?,
        args.get("x_max")?.as_f64()?,
        args.get("y_min")?.as_f64()?,
        args.get("y_max")?.as_f64()?,
    ))
}

fn parse_page(args: &Value) -> (u32, u32) {
    let page = args.get("page").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let size = args
        .get("page_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .min(200) as u32;
    (page, size)
}

fn dist_sq(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

fn apply_spatial_filter<T>(items: &mut Vec<&T>, args: &Value, pos: fn(&T) -> (f64, f64)) {
    if let Some((cx, cy, r)) = parse_circle(args) {
        items.retain(|item| {
            let (x, y) = pos(item);
            dist_sq(x, y, cx, cy) <= r * r
        });
    } else if let Some((x1, x2, y1, y2)) = parse_rect(args) {
        items.retain(|item| {
            let (x, y) = pos(item);
            x >= x1 && x <= x2 && y >= y1 && y <= y2
        });
    }
}

fn sort_creatures(list: &mut Vec<&Creature>, field: &str, desc: bool) {
    list.sort_by(|a, b| {
        let ord = match field {
            "energy" => a.energy.partial_cmp(&b.energy),
            "age" => a.age.partial_cmp(&b.age),
            "generation" => Some(a.generation.cmp(&b.generation)),
            "speed" => a.current_speed.partial_cmp(&b.current_speed),
            _ => Some(a.id.cmp(&b.id)),
        }
        .unwrap_or(std::cmp::Ordering::Equal);
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
}

fn err_text(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

// ─── 启动入口 ─────────────────────────────────────────────────────────

/// 在独立线程中启动 MCP SSE 服务器，与 GUI 共享 sim 数据
/// 由 CellWorldApp::new() 调用，不阻塞 GUI
pub fn start_mcp_server(
    port: u16,
    snapshot: Arc<RwLock<SimSnapshot>>,
    config: Arc<RwLock<Config>>,
    cmd_tx: std::sync::mpsc::Sender<SimCommand>,
) {
    std::thread::Builder::new()
        .name("mcp-server".to_string())
        .spawn(move || {
            eprintln!("[mcp] Starting MCP SSE server on port {}", port);

            let state = Arc::new(AppState {
                snapshot,
                config,
                cmd_tx,
                sessions: Arc::new(RwLock::new(HashMap::new())),
                mcp_paused: AtomicBool::new(false),
            });

            let app = Router::new()
                .route("/sse", get(sse_handler))
                .route("/messages", post(messages_handler))
                .with_state(state);

            let addr = SocketAddr::from(([127, 0, 0, 1], port));

            let rt = tokio::runtime::Runtime::new().expect("Failed to start tokio runtime");
            rt.block_on(async {
                eprintln!("[mcp] SSE server listening on http://{}", addr);
                let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
                if let Err(e) = axum::serve(listener, app).await {
                    eprintln!("[mcp] Server error: {}", e);
                }
            });
        })
        .expect("Failed to spawn MCP server thread");
}
