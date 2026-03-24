use egui::Ui;
use std::time::Instant;

use super::canvas::species_to_color;
use super::Selection;
use crate::config::Config;
use crate::store::Store;
use crate::world::{DeathAgeStats, DominantCandidate, World};

/// 面板操作结果
#[derive(Default)]
pub struct PanelAction {
    pub spawn: Option<Option<String>>,
    pub delete_selected: bool,
    pub save_selected: Option<String>,
    pub delete_template: Option<String>,
    pub clear_dominant: bool,
    pub save_snapshot: bool,
    /// 保存指定族的代表基因（clan_hash）
    pub save_clan: Option<u64>,
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
    /// 能量历史 (world_time, total_energy, creature_energy, particle_initial_energy)
    pub energy_history: Vec<(f64, f64, f64, f64)>,
}

/// 排名数据
#[derive(Clone, Default)]
pub struct RankedEntry {
    pub id: usize,
    pub count: usize,
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
    // 行为触发统计（4事件：移动/吸收/咬/繁殖）
    pub action_counts: [usize; 4],
    pub death_age_stats: DeathAgeStats,
    pub clan_count: usize,
    pub top_clans: Vec<(u64, usize)>,
    pub dominant_candidate: Option<DominantCandidate>,
    pub avg_compute_ns: f64,
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
            energy_history: Vec::new(),
        }
    }

    pub fn update(
        &mut self,
        world: &World,
        config: &Config,
        species_threshold: f64,
        fps: f64,
        now: Instant,
    ) {
        self.cached_stats.fps = fps;
        self.cached_stats.volcano_countdown = world.volcano_countdown(config);

        if now.duration_since(self.last_update).as_secs_f64() >= self.update_interval {
            self.last_update = now;
            let stats = world.stats(species_threshold, config);
            self.cached_stats = CachedStats {
                time: stats.time,
                fps,
                creature_count: stats.creature_count,
                energy_particle_count: stats.energy_particle_count,
                trail_count: stats.trail_count,
                total_energy: stats.total_energy,
                creature_energy: stats.creature_energy,
                theoretical_energy: stats.theoretical_energy,
                volcano_countdown: self.cached_stats.volcano_countdown,
                max_generation: stats.max_generation,
                avg_energy: stats.avg_energy,
                action_counts: stats.action_counts,
                death_age_stats: stats.death_age_stats.clone(),
                clan_count: stats.clan_count,
                top_clans: stats.top_clans.clone(),
                dominant_candidate: stats.dominant_candidate.clone(),
                avg_compute_ns: world.perf_stats.avg_compute_ns,
            };
            // 记录能量历史（总能量 + 生命能量 + 理论投放能量）
            self.energy_history.push((
                stats.time,
                stats.total_energy,
                stats.creature_energy,
                stats.theoretical_energy,
            ));
            // 按时间裁剪：只保留最近30分钟
            let cutoff = stats.time - 1800.0;
            if let Some(pos) = self
                .energy_history
                .iter()
                .position(|&(t, _, _, _)| t >= cutoff)
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
        speed: &mut f64,
        paused: &mut bool,
        store: &Store,
        _config: &mut Config,
    ) -> PanelAction {
        let mut action = PanelAction::default();

        ui.horizontal(|ui| {
            ui.heading("Cell World");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("💾")
                    .on_hover_text("保存世界快照")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    action.save_snapshot = true;
                }
                if ui
                    .button("⚙")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    if self.settings_open {
                        self.settings_open = false;
                    } else {
                        self.settings_open = true;
                        self.energy_settings_open = false;
                    }
                }
                if ui
                    .button("🌋")
                    .on_hover_text("能量源周期")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    if self.energy_settings_open {
                        self.energy_settings_open = false;
                    } else {
                        self.energy_settings_open = true;
                        self.settings_open = false;
                    }
                }
            });
        });
        ui.separator();

        // FPS 和 时间
        ui.horizontal(|ui| {
            ui.label(format!("FPS: {:.0}", fps));
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
        });

        ui.separator();

        // 模板选择 + 添加按钮
        ui.horizontal(|ui| {
            ui.label("统计");
            ui.separator();

            let template_names = store.names();
            let options: Vec<&str> = std::iter::once("随机")
                .chain(template_names.iter().copied())
                .collect();

            egui::ComboBox::from_id_salt("template_select")
                .selected_text(*options.get(self.selected_template).unwrap_or(&"随机"))
                .show_ui(ui, |ui| {
                    for (i, name) in options.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_template, i, *name);
                    }
                });

            if ui
                .button("+")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                if self.selected_template == 0 {
                    action.spawn = Some(None);
                } else if let Some(name) = template_names.get(self.selected_template - 1) {
                    action.spawn = Some(Some(name.to_string()));
                }
            }

            if self.selected_template > 0 {
                if ui
                    .button("-")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    if let Some(name) = template_names.get(self.selected_template - 1) {
                        action.delete_template = Some(name.to_string());
                    }
                }
            }

            if ui
                .button("清空")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                action.clear_dominant = true;
            }
        });

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
            // 火山倒计时
            let countdown = self.cached_stats.volcano_countdown;
            let color = if countdown < 5.0 {
                egui::Color32::from_rgb(255, 80, 30)
            } else if countdown < 15.0 {
                egui::Color32::from_rgb(255, 200, 50)
            } else {
                egui::Color32::from_rgb(200, 200, 200)
            };
            ui.colored_label(color, format!("🌋{:.0}s", countdown));
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
                "行为: 移动:{}│吸收:{}│咬:{}│繁殖:{}",
                format_count(acts[0]),
                format_count(acts[1]),
                format_count(acts[2]),
                format_count(acts[3])
            ));
        });

        // 死亡年龄统计
        let death = &self.cached_stats.death_age_stats;
        if death.count > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "寿命: 均:{:.1}│中:{:.1}│最长:{:.1}│最短:{:.1}│死亡:{}",
                    death.avg, death.median, death.max, death.min, death.count
                ));
            });
        }

        // 种族排行
        ui.separator();
        ui.label(format!("种族 ({})", self.cached_stats.clan_count));
        for (i, &(clan_hash, count)) in self.cached_stats.top_clans.iter().enumerate() {
            ui.horizontal(|ui| {
                let color = species_to_color(clan_hash);
                ui.colored_label(color, format!("{}. {} 个体", i + 1, count));
                if ui.small_button("💾").on_hover_text("保存该族代表基因").clicked() {
                    action.save_clan = Some(clan_hash);
                }
            });
        }

        action
    }

    pub fn render_selection(
        &mut self,
        ui: &mut Ui,
        selection: &Selection,
        world: &World,
    ) -> PanelAction {
        let mut action = PanelAction::default();

        match selection {
            Selection::None => {}
            Selection::Creature(id) => {
                if let Some(creature) = world.creatures.iter().find(|c| c.id == *id && c.alive) {
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
                            self.save_name = format!("生物_{:08X}", creature.genome_hash);
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

                    // 基本状态
                    let body_radius = (creature.energy * 1.28_f64).cbrt();
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "能量:{:.0}  体型:{:.1}  代:{}", creature.energy, body_radius, creature.generation
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "年龄:{:.1}s  速度:{:.1}  朝向:{:.0}°",
                            creature.age, creature.current_speed, creature.heading.to_degrees()
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("位置: ({:.0}, {:.0})", creature.x, creature.y));
                    });

                    // 冷却
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "嘴:{:.1}s  繁殖:{:.1}s  痕迹:{:.1}s",
                            creature.mouth_cooldown_timer.max(0.0),
                            creature.reproduce_cooldown_timer.max(0.0),
                            creature.trail_emit_timer.max(0.0),
                        ));
                    });

                    // 神经网络输出（实时决策）
                    ui.separator();
                    let o = &creature.last_outputs;
                    ui.label("决策输出");
                    ui.horizontal(|ui| {
                        ui.label(format!("转向:{:+.2}  速度:{:.2}", o[0], o[1]));
                    });
                    ui.horizontal(|ui| {
                        let mouth_str = if o[2] < -0.1 { "咬" } else { "闭" };
                        ui.label(format!(
                            "嘴:{:.2}({})  繁殖意愿:{:.2}", o[2], mouth_str, o[3]
                        ));
                    });
                    ui.horizontal(|ui| {
                        // sigmoid映射：繁殖阈值 20~200，子代比例 0.1~0.5
                        let threshold = 20.0 + 180.0 / (1.0 + (-o[4]).exp());
                        let child_ratio = 0.1 + 0.4 / (1.0 + (-o[5]).exp());
                        ui.label(format!(
                            "繁殖阈值:{:.0}  子代比例:{:.0}%  痕迹:{:.2}",
                            threshold, child_ratio * 100.0, o[6].max(0.0)
                        ));
                    });
                }
            }
            Selection::Energy(id) => {
                if let Some(particle) = world
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
                        ui.label("能量值:");
                        ui.label(format!("{:.1}", particle.energy));
                    });
                    ui.horizontal(|ui| {
                        ui.label("初始能量:");
                        ui.label(format!("{:.1}", particle.initial_energy));
                    });
                    ui.horizontal(|ui| {
                        ui.label("存在时间:");
                        ui.label(format!("{:.1}s", particle.age));
                    });
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
