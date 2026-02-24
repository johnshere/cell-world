use egui::Ui;
use rustc_hash::FxHashMap;
use std::time::Instant;

use crate::config::Config;
use crate::store::Store;
use crate::world::World;
use super::Selection;

/// 面板操作结果
#[derive(Default)]
pub struct PanelAction {
    /// 添加生物（None=不添加，Some(None)=随机，Some(Some(name))=使用模板）
    pub spawn: Option<Option<String>>,
    /// 删除选中的生物
    pub delete_selected: bool,
    /// 保存选中的生物（带名称）
    pub save_selected: Option<String>,
}

/// 统计面板
pub struct StatsPanel {
    /// 更新间隔（秒）
    update_interval: f64,
    /// 上次更新时间（真实时间）
    last_update: Instant,
    /// 缓存的统计数据
    cached_stats: CachedStats,
    /// 当前选择的模板索引（0=随机）
    selected_template: usize,
    /// 保存对话框状态
    save_dialog_open: bool,
    /// 保存名称输入
    save_name: String,
}

/// 排名数据（用于显示）
#[derive(Clone, Default)]
pub struct RankedEntry {
    pub id: usize,
    pub count: usize,
    pub family_id: usize,   // 关联的家族ID
    pub species_id: usize,  // 关联的种族ID
}

#[derive(Default, Clone)]
pub struct CachedStats {
    pub time: f64,
    pub fps: f64,
    pub creature_count: usize,
    pub energy_particle_count: usize,
    pub total_energy: f64,           // 总能量（生物+粒子）
    /// 当前能量投放强度（波动值，1.0 = 100%）
    pub energy_intensity: f64,
    pub alive_families: usize,
    pub extinct_families: usize,
    pub max_generation: usize,
    pub avg_energy: f64,
    // 功能解锁统计（每个功能解锁的生物数）
    pub function_unlocks: [usize; 8],
    // 行为触发统计（累计触发次数）
    pub action_counts: [usize; 8],
    // 种群统计
    pub species_count: usize,
    pub top_families: Vec<RankedEntry>,
    pub top_species: Vec<RankedEntry>,
    // 生物ID -> 种群ID 映射
    pub creature_species_map: FxHashMap<u64, usize>,
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            update_interval: 0.5, // 500ms
            last_update: Instant::now(),
            cached_stats: CachedStats::default(),
            selected_template: 0,
            save_dialog_open: false,
            save_name: String::new(),
        }
    }

    /// 更新缓存的统计数据
    pub fn update(&mut self, world: &World, config: &Config, species_threshold: f64, fps: f64, now: Instant) {
        // fps 每帧都更新
        self.cached_stats.fps = fps;
        // 能量强度每帧更新（显示波动效果）
        self.cached_stats.energy_intensity = world.calculate_energy_intensity(config);

        // 使用真实时间进行缓存检查，避免速度倍率影响
        if now.duration_since(self.last_update).as_secs_f64() >= self.update_interval {
            self.last_update = now;
            let stats = world.stats(species_threshold);
            self.cached_stats = CachedStats {
                time: stats.time,
                fps,
                creature_count: stats.creature_count,
                energy_particle_count: stats.energy_particle_count,
                total_energy: stats.total_energy,
                energy_intensity: self.cached_stats.energy_intensity,
                alive_families: stats.alive_families,
                extinct_families: stats.extinct_families,
                max_generation: stats.max_generation,
                avg_energy: stats.avg_energy,
                function_unlocks: stats.function_unlocks,
                action_counts: stats.action_counts,
                species_count: stats.species_count,
                top_families: stats.top_families.iter()
                    .map(|e| RankedEntry {
                        id: e.id,
                        count: e.count,
                        family_id: e.family_id,
                        species_id: e.species_id,
                    })
                    .collect(),
                top_species: stats.top_species.iter()
                    .map(|e| RankedEntry {
                        id: e.id,
                        count: e.count,
                        family_id: e.family_id,
                        species_id: e.species_id,
                    })
                    .collect(),
                creature_species_map: stats.creature_species_map.clone(),
            };
        }
    }

    /// 获取缓存的统计数据
    pub fn stats(&self) -> &CachedStats {
        &self.cached_stats
    }

    /// 渲染面板，返回面板操作
    pub fn render(&mut self, ui: &mut Ui, fps: f64, speed: &mut f64, paused: &mut bool, store: &Store) -> PanelAction {
        let mut action = PanelAction::default();

        ui.heading("Cell World");
        ui.separator();

        // FPS 和 时间 一行
        ui.horizontal(|ui| {
            ui.label(format!("FPS: {:.0}", fps));
            ui.separator();
            ui.label(format!("时间: {:.0}s", self.cached_stats.time));
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

        // 统计 + 下拉选 + 添加按钮
        ui.horizontal(|ui| {
            ui.label("统计");
            ui.separator();

            // 下拉选择模板
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
                    action.spawn = Some(None); // 随机
                } else if let Some(name) = template_names.get(self.selected_template - 1) {
                    action.spawn = Some(Some(name.to_string())); // 使用模板
                }
            }
        });

        // 统计（一行显示）
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("生物:{}", self.cached_stats.creature_count));
            ui.label("│");
            ui.label(format!("粒子:{}", self.cached_stats.energy_particle_count));
            ui.label("│");
            ui.label(format!("总能:{:.0}", self.cached_stats.total_energy));
            ui.label("│");
            ui.label(format!("均能:{:.0}", self.cached_stats.avg_energy));
            ui.label("│");
            ui.label(format!("家族:{}", self.cached_stats.alive_families));
            ui.label("│");
            ui.label(format!("灭绝:{}", self.cached_stats.extinct_families));
            ui.label("│");
            ui.label(format!("代:{}", self.cached_stats.max_generation));
            ui.label("│");
            ui.label(format!("种群:{}", self.cached_stats.species_count));
            ui.label("│");
            // 能量强度（波动值），用不同颜色表示高低
            let intensity = self.cached_stats.energy_intensity;
            let color = if intensity > 1.2 {
                egui::Color32::from_rgb(100, 255, 100)  // 高强度：绿色
            } else if intensity < 0.8 {
                egui::Color32::from_rgb(255, 150, 100)  // 低强度：橙色
            } else {
                egui::Color32::from_rgb(200, 200, 200)  // 正常：灰色
            };
            ui.colored_label(color, format!("☀{:.0}%", intensity * 100.0));
        });

        // 功能解锁统计（每个功能解锁的生物数）
        let func_names = ["方向", "速度", "吸收", "释放", "繁殖", "转移", "扫描R", "扫描V"];
        ui.horizontal_wrapped(|ui| {
            ui.label("功能:");
            for (i, &count) in self.cached_stats.function_unlocks.iter().enumerate() {
                if i > 0 {
                    ui.label("│");
                }
                ui.label(format!("{}:{}", func_names[i], count));
            }
        });

        // 行为触发次数统计
        let action_names = ["移动", "速度", "吸收", "释放", "繁殖", "转移", "扫描R", "扫描V"];
        ui.horizontal_wrapped(|ui| {
            ui.label("行为:");
            for (i, &count) in self.cached_stats.action_counts.iter().enumerate() {
                if i > 0 {
                    ui.label("│");
                }
                // 用 K/M 简化大数字显示
                let display = if count >= 1_000_000 {
                    format!("{}:{:.1}M", action_names[i], count as f64 / 1_000_000.0)
                } else if count >= 1_000 {
                    format!("{}:{:.1}K", action_names[i], count as f64 / 1_000.0)
                } else {
                    format!("{}:{}", action_names[i], count)
                };
                ui.label(display);
            }
        });

        // 排行榜
        ui.separator();
        ui.horizontal(|ui| {
            // 家族前三（金/银/铜描边）
            ui.vertical(|ui| {
                ui.label("家族前三:");
                for (i, entry) in self.cached_stats.top_families.iter().enumerate() {
                    ui.label(format!("{}. {}[#{}]", i + 1, entry.count, entry.family_id));
                }
            });
            ui.separator();
            // 种群前三
            ui.vertical(|ui| {
                ui.label("种群前三:");
                for (i, entry) in self.cached_stats.top_species.iter().enumerate() {
                    ui.label(format!("{}. {}[${}]", i + 1, entry.count, entry.species_id));
                }
            });
        });

        action
    }

    /// 渲染选中信息，返回操作
    pub fn render_selection(&mut self, ui: &mut Ui, selection: &Selection, world: &World) -> PanelAction {
        let creature_species_map = &self.cached_stats.creature_species_map;
        let mut action = PanelAction::default();

        match selection {
            Selection::None => {}
            Selection::Creature(id) => {
                if let Some(creature) = world.creatures.iter().find(|c| c.id == *id && c.alive) {
                        ui.separator();
                        ui.label("选中生物");

                        // 删除和保存按钮
                        ui.horizontal(|ui| {
                            if ui.button("🗑 删除").clicked() {
                                action.delete_selected = true;
                            }
                            if ui.button("💾 保存").clicked() {
                                self.save_dialog_open = true;
                                self.save_name = format!("生物_{:08X}", creature.genome_hash);
                            }
                        });

                        // 保存对话框
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
                            ui.label("家族ID:");
                            ui.label(format!("{}", creature.family_id));
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

                        ui.horizontal(|ui| {
                            ui.label("输出维度:");
                            ui.label(format!("{}", creature.genome.output_map.len()));
                        });

                        // 功能映射
                        let func_names = ["移动X", "移动Y", "吸收", "释放", "繁殖", "转移"];
                        ui.horizontal_wrapped(|ui| {
                            ui.label("功能:");
                            for &func_id in &creature.genome.output_map {
                                if func_id < func_names.len() {
                                    ui.label(func_names[func_id]);
                                }
                            }
                        });
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
                        ui.label("存在时间:");
                        ui.label(format!("{:.1}s", particle.age));
                    });

                    ui.horizontal(|ui| {
                        ui.label("剩余时间:");
                        ui.label(format!("{:.1}s", particle.lifetime - particle.age));
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
