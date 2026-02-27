use egui::Ui;
use rustc_hash::FxHashMap;
use std::time::Instant;

use crate::config::Config;
use crate::store::Store;
use crate::world::{World, DeathAgeStats, DominantCandidate};
use super::Selection;

/// 面板操作结果
#[derive(Default)]
pub struct PanelAction {
    pub spawn: Option<Option<String>>,
    pub delete_selected: bool,
    pub save_selected: Option<String>,
    pub delete_template: Option<String>,
}

/// 统计面板
pub struct StatsPanel {
    update_interval: f64,
    last_update: Instant,
    cached_stats: CachedStats,
    selected_template: usize,
    save_dialog_open: bool,
    save_name: String,
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
    pub total_energy: f64,
    pub volcano_countdown: f64,
    pub max_generation: usize,
    pub avg_energy: f64,
    // 行为触发统计（5事件：移动/吸收/咬/喂/繁殖）
    pub action_counts: [usize; 5],
    pub death_age_stats: DeathAgeStats,
    pub species_count: usize,
    pub top_species: Vec<RankedEntry>,
    pub creature_species_map: FxHashMap<u64, u64>,
    pub dominant_candidate: Option<DominantCandidate>,
}

/// 将秒数格式化为 d h m s
fn format_dhms(seconds: f64) -> String {
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
        }
    }

    pub fn update(&mut self, world: &World, config: &Config, species_threshold: f64, fps: f64, now: Instant) {
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
                total_energy: stats.total_energy,
                volcano_countdown: self.cached_stats.volcano_countdown,
                max_generation: stats.max_generation,
                avg_energy: stats.avg_energy,
                action_counts: stats.action_counts,
                death_age_stats: stats.death_age_stats.clone(),
                species_count: stats.species_count,
                top_species: stats.top_species.iter()
                    .map(|e| RankedEntry { id: e.id, count: e.count })
                    .collect(),
                creature_species_map: stats.creature_species_map.clone(),
                dominant_candidate: stats.dominant_candidate.clone(),
            };
        }
    }

    pub fn stats(&self) -> &CachedStats {
        &self.cached_stats
    }

    pub fn reset_template_selection(&mut self) {
        self.selected_template = 0;
    }

    pub fn render(&mut self, ui: &mut Ui, fps: f64, speed: &mut f64, paused: &mut bool, store: &Store) -> PanelAction {
        let mut action = PanelAction::default();

        ui.heading("Cell World");
        ui.separator();

        // FPS 和 时间
        ui.horizontal(|ui| {
            ui.label(format!("FPS: {:.0}", fps));
            ui.separator();
            ui.label(format!("时间: {}", format_dhms(self.cached_stats.time)));
        });

        // 速度控制
        ui.horizontal(|ui| {
            if ui.button("⏪").clicked() {
                *speed = (*speed - 0.2).max(0.1);
            }
            ui.add(egui::Slider::new(speed, 0.1..=10.0).logarithmic(true));
            if ui.button("⏩").clicked() {
                *speed = (*speed + 0.2).min(10.0);
            }
            if ui.button(if *paused { "▶" } else { "⏸" }).clicked() {
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

            if ui.button("+").clicked() {
                if self.selected_template == 0 {
                    action.spawn = Some(None);
                } else if let Some(name) = template_names.get(self.selected_template - 1) {
                    action.spawn = Some(Some(name.to_string()));
                }
            }

            if self.selected_template > 0 {
                if ui.button("-").clicked() {
                    if let Some(name) = template_names.get(self.selected_template - 1) {
                        action.delete_template = Some(name.to_string());
                    }
                }
            }
        });

        // 统计
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("生物:{}", self.cached_stats.creature_count));
            ui.label("│");
            ui.label(format!("粒子:{}", self.cached_stats.energy_particle_count));
            ui.label("│");
            ui.label(format!("总能:{:.0}", self.cached_stats.total_energy));
            ui.label("│");
            ui.label(format!("均能:{:.0}", self.cached_stats.avg_energy));
            ui.label("│");
            ui.label(format!("代:{}", self.cached_stats.max_generation));
            ui.label("│");
            ui.label(format!("种群:{}", self.cached_stats.species_count));
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
                "行为: 移动:{}│吸收:{}│咬:{}│喂:{}│繁殖:{}",
                format_count(acts[0]), format_count(acts[1]), format_count(acts[2]),
                format_count(acts[3]), format_count(acts[4])
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

        // 种群排行
        ui.separator();
        ui.label("种群前三:");
        for (i, entry) in self.cached_stats.top_species.iter().enumerate() {
            ui.label(format!("{}. {} 个体", i + 1, entry.count));
        }

        action
    }

    pub fn render_selection(&mut self, ui: &mut Ui, selection: &Selection, world: &World) -> PanelAction {
        let creature_species_map = &self.cached_stats.creature_species_map;
        let mut action = PanelAction::default();

        match selection {
            Selection::None => {}
            Selection::Creature(id) => {
                if let Some(creature) = world.creatures.iter().find(|c| c.id == *id && c.alive) {
                    ui.separator();
                    ui.label("选中生物");

                    ui.horizontal(|ui| {
                        if ui.button("🗑 删除").clicked() {
                            action.delete_selected = true;
                        }
                        if ui.button("💾 保存").clicked() {
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
                            if ui.button("确认保存").clicked() {
                                action.save_selected = Some(self.save_name.clone());
                                self.save_dialog_open = false;
                            }
                            if ui.button("取消").clicked() {
                                self.save_dialog_open = false;
                            }
                        });
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("位置:");
                        ui.label(format!("({:.1}, {:.1})", creature.x, creature.y));
                    });
                    ui.horizontal(|ui| {
                        ui.label("能量:");
                        ui.label(format!("{:.1}", creature.energy));
                    });
                    ui.horizontal(|ui| {
                        ui.label("年龄:");
                        ui.label(format!("{:.1}s", creature.age));
                    });
                    ui.horizontal(|ui| {
                        ui.label("朝向:");
                        ui.label(format!("{:.1}°", creature.heading.to_degrees()));
                    });
                    ui.horizontal(|ui| {
                        ui.label("种群ID:");
                        let species_id = creature_species_map.get(&creature.id).copied().unwrap_or(0);
                        ui.label(format!("{}", species_id));
                    });
                    ui.horizontal(|ui| {
                        ui.label("基因哈希:");
                        ui.label(format!("{:08X}", creature.genome_hash));
                    });

                    // 神经网络信息
                    ui.separator();
                    ui.label("神经网络");
                    ui.horizontal(|ui| {
                        ui.label("节点数:");
                        ui.label(format!("{}", creature.genome.nodes.len()));
                    });
                    ui.horizontal(|ui| {
                        ui.label("连接数:");
                        ui.label(format!("{}", creature.genome.connections.len()));
                    });
                    ui.label("输出: 转向/速度/嘴/繁殖");
                }
            }
            Selection::Energy(id) => {
                if let Some(particle) = world.energy_particles.iter().find(|e| e.id == *id && e.alive) {
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
