use egui::Ui;
use std::time::Instant;

use super::canvas::species_to_color;
use super::force_graph;
use super::Selection;
use crate::config::Config;
use crate::snapshot::list_archives;
use crate::store::Store;
use crate::world::{DeathAgeStats, DominantCandidate, SimSnapshot, GRID_WORLD_SIZE};

/// 面板操作结果
#[derive(Default)]
pub struct PanelAction {
    pub spawn: Option<Option<String>>,
    pub delete_selected: bool,
    pub save_selected: Option<String>,
    pub delete_template: Option<String>,
    pub clear_dominant: bool,
    pub save_snapshot: bool,
    /// 生成地形
    pub generate_terrain: bool,
    /// 保存指定族的代表基因（clan_hash）
    pub save_clan: Option<u64>,
    /// 重置世界（清空所有生物/粒子/痕迹，从头演化）
    pub reset_world: bool,
}

/// 统计面板
pub struct StatsPanel {
    update_interval: f64,
    last_update: Instant,
    cached_stats: CachedStats,
    pub selected_template: usize,
    save_dialog_open: bool,
    save_name: String,
    pub settings_open: bool,
    pub energy_settings_open: bool,
    pub templates_open: bool,
    /// 能量历史 (world_time, total_energy, creature_energy, theoretical_energy, creature_count)
    pub energy_history: Vec<(f64, f64, f64, f64, usize)>,
    /// 上次记录能量历史时的 world_time（防止暂停时填充相同时间戳）
    pub last_energy_time: f64,
    /// 重置确认弹框
    reset_confirm_open: bool,
    /// 查看目标偏好的来源（模板名或生物ID）
    target_pref_view: Option<TargetPrefSource>,
    /// 目标偏好窗口对应的基因组（克隆一份，避免生命周期问题）
    target_pref_genome: Option<crate::neural::Genome>,
    /// 力导图弹框：查看来源
    force_graph_view: Option<TargetPrefSource>,
    /// 力导图弹框：对应的基因组
    force_graph_genome: Option<crate::neural::Genome>,
    /// 力导图状态（跨帧维护力模拟）
    pub force_graph_state: super::force_graph::ForceGraphState,
}

/// 目标偏好查看窗口的数据来源
#[derive(Clone)]
enum TargetPrefSource {
    Template(String),
    Creature(u64),
}

#[derive(Default, Clone)]
pub struct CachedStats {
    pub time: f64,
    pub fps: f64,
    pub creature_count: usize,
    pub energy_particle_count: usize,
    pub trail_count: usize,
    pub total_energy: f64,
    pub creature_energy: f64,
    pub theoretical_energy: f64,
    pub volcano_countdown: f64,
    pub max_generation: usize,
    pub avg_energy: f64,
    // 行为触发统计（5事件：移动/吸收/咬/无性繁殖/有性繁殖）
    pub action_counts: [usize; 5],
    // 奖励触发统计（3通道：能量/痕迹/集体）
    pub reward_counts: [usize; 3],
    pub death_age_stats: DeathAgeStats,
    pub clan_count: usize,
    pub top_clans: Vec<(u64, usize)>,
    pub dominant_candidate: Option<DominantCandidate>,
    pub avg_compute_ns: f64,
    pub avg_nodes: f64,
    pub avg_connections: f64,
    pub max_nodes: usize,
    pub max_connections: usize,
    pub add_node_triggers: u64,
    pub add_conn_triggers: u64,
    /// 平均发育期（maturation_time）
    pub avg_maturation_time: f64,
}

/// 将秒数格式化为 d h m s
pub fn format_dhms(seconds: f64) -> String {
    let total = seconds as u64;
    let d = total / 86400;
    let h = (total % 86400) / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if d > 0 {
        format!("{}d{}h{}m{}s", d, h, m, s)
    } else if h > 0 {
        format!("{}h{}m{}s", h, m, s)
    } else if m > 0 {
        format!("{}m{}s", m, s)
    } else {
        format!("{}s", s)
    }
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            update_interval: 0.5,
            last_update: Instant::now(),
            cached_stats: CachedStats::default(),
            selected_template: 0,
            save_dialog_open: false,
            save_name: String::new(),
            settings_open: false,
            energy_settings_open: false,
            templates_open: false,
            energy_history: Vec::new(),
            last_energy_time: -1.0,
            reset_confirm_open: false,
            target_pref_view: None,
            target_pref_genome: None,
            force_graph_view: None,
            force_graph_genome: None,
            force_graph_state: super::force_graph::ForceGraphState::new(),
        }
    }

    pub fn update(&mut self, snapshot: &SimSnapshot, fps: f64, now: Instant) {
        self.cached_stats.fps = fps;
        self.cached_stats.volcano_countdown = snapshot.volcano_countdown;

        if now.duration_since(self.last_update).as_secs_f64() >= self.update_interval {
            self.last_update = now;
            let stats = &snapshot.world_stats;
            self.cached_stats = CachedStats {
                time: stats.time,
                fps,
                creature_count: stats.creature_count,
                energy_particle_count: stats.energy_particle_count,
                trail_count: stats.trail_count,
                total_energy: stats.total_energy,
                creature_energy: stats.creature_energy,
                theoretical_energy: stats.theoretical_energy,
                volcano_countdown: snapshot.volcano_countdown,
                max_generation: stats.max_generation,
                avg_energy: stats.avg_energy,
                action_counts: stats.action_counts,
                reward_counts: stats.reward_counts,
                death_age_stats: stats.death_age_stats.clone(),
                clan_count: stats.clan_count,
                top_clans: stats.top_clans.clone(),
                dominant_candidate: stats.dominant_candidate.clone(),
                avg_compute_ns: snapshot.perf_stats.avg_compute_ns,
                avg_nodes: 0.0,
                avg_connections: 0.0,
                max_nodes: 0,
                max_connections: 0,
                add_node_triggers: crate::neural::genome::ADD_NODE_TRIGGERS
                    .load(std::sync::atomic::Ordering::Relaxed),
                add_conn_triggers: crate::neural::genome::ADD_CONN_TRIGGERS
                    .load(std::sync::atomic::Ordering::Relaxed),
                avg_maturation_time: 0.0, // 下方覆盖真实值
            };
            // 计算存活生物的脑结构平均/最大
            let alive: Vec<_> = snapshot.creatures.iter().filter(|c| c.alive).collect();
            let n = alive.len();
            if n > 0 {
                let total_nodes: usize = alive.iter().map(|c| c.genome.nodes.len()).sum();
                let total_conns: usize = alive
                    .iter()
                    .map(|c| c.genome.connections.iter().filter(|cn| cn.enabled).count())
                    .sum();
                self.cached_stats.avg_nodes = total_nodes as f64 / n as f64;
                self.cached_stats.avg_connections = total_conns as f64 / n as f64;
                self.cached_stats.max_nodes = alive
                    .iter()
                    .map(|c| c.genome.nodes.len())
                    .max()
                    .unwrap_or(0);
                self.cached_stats.max_connections = alive
                    .iter()
                    .map(|c| c.genome.connections.iter().filter(|cn| cn.enabled).count())
                    .max()
                    .unwrap_or(0);
                self.cached_stats.avg_maturation_time =
                    alive.iter().map(|c| c.genome.maturation_time).sum::<f64>() / n as f64;
            } else {
                self.cached_stats.avg_maturation_time = 0.0;
            }
            // 记录能量历史（跳过暂停时相同时间戳，防止 X 轴塌缩）
            if (stats.time - self.last_energy_time).abs() > 0.001 {
                self.last_energy_time = stats.time;
                self.energy_history.push((
                    stats.time,
                    stats.total_energy,
                    stats.creature_energy,
                    stats.theoretical_energy,
                    stats.creature_count,
                ));
            }
            // 按时间裁剪：只保留最近30分钟
            let cutoff = stats.time - 1800.0;
            if let Some(pos) = self
                .energy_history
                .iter()
                .position(|&(t, _, _, _, _)| t >= cutoff)
            {
                if pos > 0 {
                    self.energy_history.drain(..pos);
                }
            }
        }
    }

    pub fn stats(&self) -> &CachedStats {
        &self.cached_stats
    }

    pub fn reset_template_selection(&mut self) {
        self.selected_template = 0;
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        fps: f64,
        actual_speed: f64,
        scale: f32,
        speed: &mut f64,
        paused: &mut bool,
        render_enabled: &mut bool,
        config: &mut Config,
        _store: &Store,
    ) -> PanelAction {
        let mut action = PanelAction::default();

        ui.horizontal(|ui| {
            ui.heading("Cell World");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 注意：right_to_left 布局下，先添加的按钮在最右边
                if ui
                    .button("💾")
                    .on_hover_text("保存世界快照")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    action.save_snapshot = true;
                }
                if ui
                    .button("⛰")
                    .on_hover_text("生成地形（一次性，不可撤销；已生成将被覆盖）")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    action.generate_terrain = true;
                }
                if ui
                    .button("🔄")
                    .on_hover_text("重置世界（清空所有生物/粒子/痕迹，从头演化；配置不变）")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    // 有存档则直接让 app 弹出存档列表；无存档才弹确认框
                    if list_archives().is_empty() {
                        self.reset_confirm_open = true;
                    } else {
                        action.reset_world = true;
                    }
                }
            });
        });

        // 重置确认弹框
        if self.reset_confirm_open {
            egui::Window::new("确认重置")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label("确定要清空所有生物、粒子和痕迹，从头开始演化吗？");
                    ui.label("配置不变，此操作不可撤销。");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("确认重置").clicked() {
                            action.reset_world = true;
                            self.reset_confirm_open = false;
                        }
                        if ui.button("取消").clicked() {
                            self.reset_confirm_open = false;
                        }
                    });
                });
        }

        // 目标偏好弹框（独立显示，不依赖任何 tab）
        self.render_target_pref_window(ui);

        // 力导图弹框（独立显示）
        self.render_force_graph_window(ui, config);

        ui.separator();

        // FPS、缩放和时间
        ui.horizontal(|ui| {
            // 同步批处理模型下，世界实际倍速就是唯一指标
            let speed_label = format!("FPS: {:.0} | 速度: {:.1}x", fps, actual_speed);
            ui.label(speed_label).on_hover_text("世界实际达成的倍速");
            ui.separator();
            ui.label(format!("×{:.2}", scale));
            ui.separator();
            ui.label(format!("时间: {}", format_dhms(self.cached_stats.time)));
        });

        // 速度控制
        ui.horizontal(|ui| {
            if ui
                .button("⏪")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                let new_speed = *speed - 0.5;
                if new_speed > 0.0 {
                    *speed = new_speed.max(0.1);
                }
            }
            ui.add(egui::Slider::new(speed, 0.5..=10.0).step_by(0.5));
            if ui
                .button("⏩")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *speed = (*speed + 0.5).min(10.0);
            }
            if ui
                .button(if *paused { "▶" } else { "⏸" })
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *paused = !*paused;
            }
            ui.checkbox(render_enabled, "渲染");
        });

        ui.separator();

        // 统计
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("生物:{}", self.cached_stats.creature_count));
            ui.label("│");
            ui.label(format!("粒子:{}", self.cached_stats.energy_particle_count));
            ui.label("│");
            ui.label(format!("痕迹:{}", self.cached_stats.trail_count));
            ui.label("│");
            ui.label(format!("总能:{:.0}", self.cached_stats.total_energy));
            ui.label("│");
            ui.label(format!("均能:{:.0}", self.cached_stats.avg_energy));
            ui.label("│");
            ui.label(format!("代:{}", self.cached_stats.max_generation));
            ui.label("│");
            ui.label(format!("种族:{}", self.cached_stats.clan_count));
            if self.cached_stats.avg_compute_ns > 0.0 {
                ui.label("│");
                ui.label(format!("算力:{:.0}ns", self.cached_stats.avg_compute_ns));
            }
            ui.label("│");
        });

        // 行为统计（5事件）
        let acts = &self.cached_stats.action_counts;
        let format_count = |c: usize| -> String {
            if c >= 1_000_000 {
                format!("{:.1}M", c as f64 / 1_000_000.0)
            } else if c >= 1_000 {
                format!("{:.1}K", c as f64 / 1_000.0)
            } else {
                format!("{}", c)
            }
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "行为: 移动:{}│吸收:{}│咬:{}│近1万次繁殖(无性{}/有性{})",
                format_count(acts[0]),
                format_count(acts[1]),
                format_count(acts[2]),
                format_count(acts[3]),
                format_count(acts[4])
            ));
        });

        // 奖励触发统计（3通道）
        let rw = &self.cached_stats.reward_counts;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "奖励: 能量:{}│痕迹:{}│集体:{}",
                format_count(rw[0]),
                format_count(rw[1]),
                format_count(rw[2])
            ));
        });

        // 死亡年龄统计
        let death = &self.cached_stats.death_age_stats;
        if death.count > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "寿命: 育均:{:.0}s│均:{:.1}│中:{:.1}│最长:{:.1}│最短:{:.1}│死亡:{}",
                    self.cached_stats.avg_maturation_time,
                    death.avg,
                    death.median,
                    death.max,
                    death.min,
                    death.total_deaths
                ));
            });
        }

        // 脑结构平均/最大
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "脑: 均节点:{:.1}  最大节点:{}  均连接:{:.0}  最大连接:{}",
                self.cached_stats.avg_nodes,
                self.cached_stats.max_nodes,
                self.cached_stats.avg_connections,
                self.cached_stats.max_connections,
            ));
        });
        // 结构变异触发计数（含未存活子代）
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "结构变异触发: add_node={}  add_conn={}",
                self.cached_stats.add_node_triggers, self.cached_stats.add_conn_triggers,
            ));
        });

        // 底部功能切换按钮
        ui.separator();
        ui.horizontal(|ui| {
            let make_tab = |label: &str, active: bool| -> egui::Button {
                if active {
                    egui::Button::new(egui::RichText::new(label).strong())
                        .fill(egui::Color32::from_gray(60))
                } else {
                    egui::Button::new(label)
                }
            };
            if ui
                .add(make_tab("📋 基因库", self.templates_open))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.templates_open = !self.templates_open;
                if self.templates_open {
                    self.settings_open = false;
                    self.energy_settings_open = false;
                }
            }
            if ui
                .add(make_tab("🌋 能量", self.energy_settings_open))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.energy_settings_open = !self.energy_settings_open;
                if self.energy_settings_open {
                    self.settings_open = false;
                    self.templates_open = false;
                }
            }
            if ui
                .add(make_tab("⚙ 配置", self.settings_open))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.settings_open = !self.settings_open;
                if self.settings_open {
                    self.energy_settings_open = false;
                    self.templates_open = false;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("+随机")
                    .on_hover_text("投放5个随机生物")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    action.spawn = Some(None);
                }
            });
        });

        action
    }

    /// 渲染基因库内容面板
    pub fn render_gene_library(&mut self, ui: &mut Ui, store: &Store) -> PanelAction {
        let mut action = PanelAction::default();

        let templates = store.templates();
        let auto_count = templates
            .iter()
            .filter(|t| t.auto_recorded == Some(true))
            .count();
        let manual_count = templates.len() - auto_count;

        ui.horizontal(|ui| {
            ui.label(format!("自动:{} 手动:{}", auto_count, manual_count));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if auto_count > 0 {
                    if ui
                        .small_button("清空自动")
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        action.clear_dominant = true;
                    }
                }
            });
        });

        if templates.is_empty() {
            ui.label("暂无保存的基因模板");
        } else {
            for template in templates {
                let is_auto = template.auto_recorded == Some(true);
                let tag = if is_auto { "⚡" } else { "📌" };

                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(4.0, 3.0))
                    .stroke(egui::Stroke::new(0.5, egui::Color32::from_gray(60)))
                    .rounding(3.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} {}", tag, template.name));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("🗑").on_hover_text("删除").clicked() {
                                        action.delete_template = Some(template.name.clone());
                                    }
                                    if ui
                                        .small_button("投放")
                                        .on_hover_text("投放5个该模板生物")
                                        .clicked()
                                    {
                                        action.spawn = Some(Some(template.name.clone()));
                                    }
                                    if ui
                                        .small_button("📊")
                                        .on_hover_text("查看目标偏好")
                                        .clicked()
                                    {
                                        self.target_pref_view =
                                            Some(TargetPrefSource::Template(template.name.clone()));
                                        self.target_pref_genome = Some(template.genome.clone());
                                    }
                                    if ui
                                        .small_button("🔗")
                                        .on_hover_text("查看脑拓扑力导图")
                                        .clicked()
                                    {
                                        self.force_graph_view =
                                            Some(TargetPrefSource::Template(template.name.clone()));
                                        self.force_graph_genome = Some(template.genome.clone());
                                    }
                                },
                            );
                        });

                        // 信息行
                        let mut info_parts: Vec<String> = Vec::new();
                        info_parts.push(format!("能量:{:.0}", template.initial_energy));
                        if let Some(score) = template.score {
                            info_parts.push(format!("评分:{:.1}", score));
                        }
                        if let Some(ratio) = template.population_ratio {
                            info_parts.push(format!("占比:{:.0}%", ratio * 100.0));
                        }
                        if let Some(age) = template.avg_age {
                            info_parts.push(format!("均龄:{:.0}s", age));
                        }
                        let gen = template.generation.or(template.max_generation);
                        if let Some(gen) = gen {
                            info_parts.push(format!("代:{}", gen));
                        }
                        if let Some(avg_e) = template.avg_energy {
                            info_parts.push(format!("均能:{:.0}", avg_e));
                        }

                        ui.label(
                            egui::RichText::new(info_parts.join("  "))
                                .small()
                                .color(egui::Color32::from_gray(160)),
                        );

                        // 基因结构信息
                        let conn_count = template.genome.connections.len();
                        let node_count = template.genome.nodes.len();
                        let hidden = node_count.saturating_sub(
                            crate::neural::Genome::INPUT_SIZE + crate::neural::Genome::OUTPUT_SIZE,
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "节点:{} (隐:{})  连接:{}",
                                node_count, hidden, conn_count
                            ))
                            .small()
                            .color(egui::Color32::from_gray(120)),
                        );
                    });
                ui.add_space(2.0);
            }
        }

        // 种族快速保存区
        if !self.cached_stats.top_clans.is_empty() {
            ui.separator();
            ui.label("当前种族");
            for (i, &(clan_hash, count)) in self.cached_stats.top_clans.iter().enumerate() {
                ui.horizontal(|ui| {
                    let color = species_to_color(clan_hash);
                    ui.colored_label(color, format!("{}. {} 个体", i + 1, count));
                    if ui
                        .small_button("💾")
                        .on_hover_text("保存该族代表基因")
                        .clicked()
                    {
                        action.save_clan = Some(clan_hash);
                    }
                });
            }
        }

        action
    }

    /// 渲染目标偏好查看窗口（独立弹框，不依赖任何 tab）
    pub fn render_target_pref_window(&mut self, ui: &mut Ui) {
        if self.target_pref_view.is_none() {
            return;
        }
        let source = self.target_pref_view.clone();
        let genome = self.target_pref_genome.clone();
        let label = match &source {
            Some(TargetPrefSource::Template(name)) => format!("模板: {}", name),
            Some(TargetPrefSource::Creature(id)) => format!("生物ID: {}", id),
            None => String::new(),
        };
        let fixed_w = 520.0;
        egui::Window::new(format!("目标偏好: {}", label))
            .resizable(false)
            .title_bar(false)
            .default_width(fixed_w)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_width(fixed_w);
                // 自定义标题栏
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(label).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("x").size(12.0).color(egui::Color32::from_gray(200))).frame(false).fill(egui::Color32::from_gray(50)).small()).clicked() {
                            self.target_pref_view = None;
                            self.target_pref_genome = None;
                        }
                    });
                });
                ui.separator();
                let genome = match genome.as_ref() {
                    Some(g) => g,
                    None => {
                        ui.label("无基因组数据");
                        return;
                    }
                };
                let conn_probs = &genome.conn_probs;

                if conn_probs.is_empty() {
                    ui.label("无分区连接概率数据");
                    return;
                }

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new("方向: [0]同区Proc [1]同区Out [2]跨区前馈同侧 [3]跨区前馈对侧 [4]跨区反馈")
                                .small()
                                .color(egui::Color32::from_gray(150)),
                        ).wrap());
                        ui.add_space(4.0);

                        let mut blocks: Vec<i8> = conn_probs.keys().copied().collect();
                        blocks.sort();

                        // 感官区 block → 器官名称
                        let organ_name = |blk: i8| -> &'static str {
                            match blk {
                                0 => "体感",
                                -1 => "左眼",
                                1 => "右眼",
                                -2 => "左感2",
                                2 => "右感2",
                                -3 => "左感3",
                                3 => "右感3",
                                -4 => "左感4",
                                4 => "右感4",
                                -5 => "左感5",
                                5 => "右感5",
                                -6 => "左感6",
                                6 => "右感6",
                                -7 => "左感7",
                                7 => "右感7",
                                _ => "",
                            }
                        };

                        // 辅助：渲染单个 block 内容（器官名同行，PQ/偏好换行显示）
                        let render_block = |ui: &mut egui::Ui, blk: i8, probs: Option<&crate::neural::ConnProbsGene>| {
                            let organ = organ_name(blk);
                            if let Some(p) = probs {
                                // Block 编号 + 器官名 同行
                                if organ.is_empty() {
                                    ui.label(egui::RichText::new(format!(
                                        "{:>3}",
                                        blk,
                                    )).small().color(egui::Color32::from_gray(160)));
                                } else {
                                    ui.label(egui::RichText::new(format!(
                                        "{:>3} {}\n",
                                        blk, organ,
                                    )).small().color(egui::Color32::from_gray(160)));
                                }
                                // PQ 一行
                                ui.label(egui::RichText::new(format!(
                                    "    P:[{:.2} {:.2} {:.2} {:.2} {:.2}]  Q:[{:.2} {:.2} {:.2} {:.2} {:.2}]",
                                    p.proc[0], p.proc[1], p.proc[2], p.proc[3], p.proc[4],
                                    p.out[0], p.out[1], p.out[2], p.out[3], p.out[4],
                                )).small().color(egui::Color32::from_gray(150)));
                                // target_pref 目标偏好（文本换行）
                                if !p.target_pref.is_empty() {
                                    let mut prefs: Vec<(&i8, &f32)> = p.target_pref.iter().collect();
                                    prefs.sort_by_key(|pr| pr.0);
                                    let line: String = prefs.iter().map(|(k, v)| {
                                        let tag = if **v > 1.0 { "+" } else if **v < 1.0 { "-" } else { "~" };
                                        format!("{}{}:{:.2}", tag, *k, v)
                                    }).collect::<Vec<_>>().join(" ");
                                    ui.add(egui::Label::new(
                                        egui::RichText::new(line).small().color(egui::Color32::from_gray(200))
                                    ).wrap());
                                }
                            } else {
                                if organ.is_empty() {
                                    ui.label(egui::RichText::new(format!("{:>3}  ---", blk)).small().color(egui::Color32::from_gray(100)));
                                } else {
                                    ui.label(egui::RichText::new(format!("{:>3} {}  ---", blk, organ)).small().color(egui::Color32::from_gray(100)));
                                }
                            }
                        };

                        // 辅助：渲染一个配对行（负值左 | 正值右，等宽等高）
                        let render_pair = |ui: &mut egui::Ui, neg_blk: i8, pos_blk: i8| {
                            let neg_probs = conn_probs.get(&neg_blk);
                            let pos_probs = conn_probs.get(&pos_blk);
                            egui::Frame::none()
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(80)))
                                .rounding(3.0)
                                .inner_margin(4.0)
                                .show(ui, |ui| {
                                    ui.columns(2, |cols| {
                                        // 左列：负值 block
                                        render_block(&mut cols[0], neg_blk, neg_probs);
                                        // 右列：正值 block
                                        render_block(&mut cols[1], pos_blk, pos_probs);
                                    });
                                });
                            ui.add_space(2.0);
                        };

                        // 感官区: Block -7 ~ 7（0在最上方，然后按绝对值排列配对）
                        ui.label(egui::RichText::new("── 感官区 Block -7 ~ 7 ──").small().color(egui::Color32::from_gray(130)));
                        // Block 0 体感区（单独一行，排最前）
                        egui::Frame::none()
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(80)))
                            .rounding(3.0)
                            .inner_margin(4.0)
                            .show(ui, |ui| {
                                render_block(ui, 0, conn_probs.get(&0));
                            });
                        ui.add_space(2.0);
                        for abs in 1..=7 {
                            render_pair(ui, -(abs as i8), abs as i8);
                        }

                        // 联合区: Block -24~-8 / 8~24
                        ui.label(egui::RichText::new("── 联合区 Block -24 ~ -8 / 8 ~ 24 ──").small().color(egui::Color32::from_gray(130)));
                        for abs in 8..=24 {
                            render_pair(ui, -(abs as i8), abs as i8);
                        }
                    });
            });
    }

    /// 渲染力导图弹框（分组力导图可视化）
    /// `config` 用于读取力导图锚定强度配置（每次打开弹框时读取）
    pub fn render_force_graph_window(&mut self, ui: &mut Ui, config: &Config) {
        if self.force_graph_view.is_none() {
            return;
        }
        // 每次打开弹框从 config 读取锚定强度（允许运行时调节）
        self.force_graph_state.h_anchor = config.force_graph_h_anchor;
        self.force_graph_state.v_anchor = config.force_graph_v_anchor;
        self.force_graph_state.max_vel = config.force_graph_max_vel;

        let source = self.force_graph_view.clone();
        let genome = self.force_graph_genome.clone();
        let label = match &source {
            Some(TargetPrefSource::Template(name)) => format!("模板: {}", name),
            Some(TargetPrefSource::Creature(id)) => format!("生物ID: {}", id),
            None => String::new(),
        };

        let genome = match genome.as_ref() {
            Some(g) => g,
            None => return,
        };

        let close =
            force_graph::render_force_graph_window(ui, genome, &mut self.force_graph_state, &label);

        if close {
            self.force_graph_view = None;
            self.force_graph_genome = None;
            self.force_graph_state.reset();
        }
    }

    pub fn render_selection(
        &mut self,
        ui: &mut Ui,
        selection: &Selection,
        snapshot: &SimSnapshot,
        lpp: f64,
    ) -> PanelAction {
        let mut action = PanelAction::default();

        match selection {
            Selection::None => {}
            Selection::Creature(id) => {
                if let Some(creature) = snapshot.creatures.iter().find(|c| c.id == *id && c.alive) {
                    ui.separator();
                    ui.label("选中生物");

                    ui.horizontal(|ui| {
                        if ui
                            .button("🗑 删除")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            action.delete_selected = true;
                        }
                        if ui
                            .button("💾 保存")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.save_dialog_open = true;
                            let version = env!("CARGO_PKG_VERSION");
                            let now = chrono::Local::now();
                            self.save_name =
                                format!("生物_v{}_{}", version, now.format("%m%d_%H%M"));
                        }
                    });

                    if self.save_dialog_open {
                        ui.horizontal(|ui| {
                            ui.label("名称:");
                            ui.text_edit_singleline(&mut self.save_name);
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .button("确认保存")
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                action.save_selected = Some(self.save_name.clone());
                                self.save_dialog_open = false;
                            }
                            if ui
                                .button("取消")
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.save_dialog_open = false;
                            }
                        });
                    }

                    ui.separator();

                    // ── 基本状态 ──
                    let body_radius = (creature.energy * 1.28_f64).cbrt();
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "能量:{:.0}  体型:{:.1}  代:{}",
                            creature.energy, body_radius, creature.generation
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "年龄:{:.1}s  速度:{:.1}  朝向:{:.0}°",
                            creature.age,
                            creature.current_speed,
                            creature.heading.to_degrees()
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("位置: ({:.0}, {:.0})", creature.x, creature.y));
                    });
                    // 发育进度
                    let mat = creature.genome.maturation_time;
                    let dev_progress = if mat > 0.0 { creature.age / mat } else { 1.0 };
                    let dev_label = if dev_progress < 1.0 {
                        "发育中"
                    } else {
                        "成熟"
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "发育:{:.0}/{:.0}s ({:.0}% {})",
                            creature.age.min(mat),
                            mat,
                            (dev_progress * 100.0).min(999.0),
                            dev_label
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "跟随度:{:.2}  发光:{:.1}",
                            creature.follow_level, creature.light_intensity
                        ));
                    });

                    // ── 运动决策 (block 25) ──
                    ui.separator();
                    let o = &creature.last_outputs;
                    ui.label("运动 (block 25)");
                    ui.horizontal(|ui| {
                        let mouth_str = if o[2] < -0.1 { "咬" } else { "闭" };
                        ui.label(format!(
                            "转向:{:+.2}  速度:{:.2}  嘴:{:.2}({})",
                            o[0], o[1], o[2], mouth_str
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "痕迹:{:.2}  嘴冷却:{:.1}s",
                            o[6].max(0.0),
                            creature.mouth_cooldown_timer.max(0.0),
                        ));
                    });

                    // ── 繁殖决策 (block -25) ──
                    ui.label("繁殖 (block -25)");
                    let threshold = 20.0 + (o[4] * 0.5 + 0.5).clamp(0.0, 1.0) * 180.0;
                    let child_ratio = 0.1 + (o[5] * 0.5 + 0.5).clamp(0.0, 1.0) * 0.4;
                    let ready = if o[3] > 0.2 && creature.energy >= threshold {
                        "Ready"
                    } else {
                        ""
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "意愿:{:.2}  阈值:{:.0}  比例:{:.0}% {}",
                            o[3],
                            threshold,
                            child_ratio * 100.0,
                            ready
                        ));
                    });

                    // ── 发光 (block 26) ──
                    ui.label(format!("发光 (block 26): {:.1}", creature.light_intensity));

                    // ── 感知概览 ──
                    ui.separator();
                    ui.label("感知");
                    let p = &creature.perception_cache;
                    ui.horizontal(|ui| {
                        // 左眼最近目标
                        let type_l = match p[3] as i32 {
                            0 => "-",
                            _ if p[3] < 0.5 => "粒",
                            _ if p[3] < 0.8 => "痕",
                            _ => "生",
                        };
                        let type_r = match p[11] as i32 {
                            0 => "-",
                            _ if p[11] < 0.5 => "粒",
                            _ if p[11] < 0.8 => "痕",
                            _ => "生",
                        };
                        ui.label(format!(
                            "左眼[{}]近:{:.2}  右眼[{}]近:{:.2}",
                            type_l, p[1], type_r, p[9]
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "能量密度 L:{:.3} R:{:.3}  自身:{:.2}",
                            p[5], p[13], p[16]
                        ));
                    });
                    if p[19] > 0.0 {
                        ui.horizontal(|ui| {
                            ui.label(format!("发光感知: 强度={:.1}", p[19]));
                        });
                    }

                    // ── 神经网络结构 ──
                    ui.separator();
                    let conn_count = creature.genome.connections.len();
                    let enabled_count = creature
                        .genome
                        .connections
                        .iter()
                        .filter(|c| c.enabled)
                        .count();
                    let node_count = creature.genome.nodes.len();
                    let hidden = node_count.saturating_sub(
                        crate::neural::Genome::INPUT_SIZE + crate::neural::Genome::OUTPUT_SIZE,
                    );
                    ui.label(format!(
                        "节点:{} (隐:{})  连接:{}/{}",
                        node_count, hidden, enabled_count, conn_count
                    ));

                    // 目标偏好基因（target_pref）
                    let total_pref_entries: usize = creature
                        .genome
                        .conn_probs
                        .values()
                        .map(|p| p.target_pref.len())
                        .sum();
                    if ui
                        .button(format!("查看偏好 ({})", total_pref_entries))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.target_pref_view = Some(TargetPrefSource::Creature(creature.id));
                        self.target_pref_genome = Some(creature.genome.clone());
                    }
                    if ui
                        .button("脑拓扑力导图 🔗")
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.force_graph_view = Some(TargetPrefSource::Creature(creature.id));
                        self.force_graph_genome = Some(creature.genome.clone());
                    }
                }
            }
            Selection::Energy(id) => {
                if let Some(particle) = snapshot
                    .energy_particles
                    .iter()
                    .find(|e| e.id == *id && e.alive)
                {
                    ui.separator();
                    ui.label("选中能量粒子");
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("位置:");
                        ui.label(format!("({:.1}, {:.1})", particle.x, particle.y));
                    });
                    ui.horizontal(|ui| {
                        let ratio = if particle.initial_energy > 0.0 {
                            particle.energy / particle.initial_energy
                        } else {
                            0.0
                        };
                        ui.label(format!(
                            "能量:{:.1}/{:.1} ({:.0}%)",
                            particle.energy,
                            particle.initial_energy,
                            ratio * 100.0
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("存在时间:{:.1}s", particle.age));
                    });
                    if particle.lava {
                        ui.horizontal(|ui| {
                            ui.label(format!("熔岩粒子  代数:{}", particle.chain_depth));
                        });
                    }

                    // 地形高度 + 等效液面
                    let cx = (particle.x / GRID_WORLD_SIZE).floor() as i32;
                    let cy = (particle.y / GRID_WORLD_SIZE).floor() as i32;
                    let th = snapshot.terrain.height_at(particle.x, particle.y);
                    if let Some(th) = th {
                        let count = snapshot
                            .energy_particles
                            .iter()
                            .filter(|p| {
                                p.alive
                                    && (p.x / GRID_WORLD_SIZE).floor() as i32 == cx
                                    && (p.y / GRID_WORLD_SIZE).floor() as i32 == cy
                            })
                            .count();
                        let eff = th as f64 + lpp * count as f64;
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "地形高:{}  区块粒子:{}  等效液面:{:.1}",
                                th, count, eff
                            ));
                        });
                    }
                }
            }
        }

        action
    }
}

impl Default for StatsPanel {
    fn default() -> Self {
        Self::new()
    }
}
