use egui::Ui;

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
    /// 更新间隔
    update_interval: f64,
    /// 上次更新时间
    last_update: f64,
    /// 缓存的统计数据
    cached_stats: CachedStats,
    /// 当前选择的模板索引（0=随机）
    selected_template: usize,
    /// 保存对话框状态
    save_dialog_open: bool,
    /// 保存名称输入
    save_name: String,
}

#[derive(Default, Clone)]
pub struct CachedStats {
    pub time: f64,
    pub creature_count: usize,
    pub energy_particle_count: usize,
    pub alive_families: usize,
    pub extinct_families: usize,
    pub largest_family: usize,
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            update_interval: 0.5, // 500ms
            last_update: 0.0,
            cached_stats: CachedStats::default(),
            selected_template: 0,
            save_dialog_open: false,
            save_name: String::new(),
        }
    }

    /// 更新缓存的统计数据
    pub fn update(&mut self, world: &World) {
        let current_time = world.time;
        if current_time - self.last_update >= self.update_interval {
            self.last_update = current_time;
            let stats = world.stats();
            self.cached_stats = CachedStats {
                time: stats.time,
                creature_count: stats.creature_count,
                energy_particle_count: stats.energy_particle_count,
                alive_families: stats.alive_families,
                extinct_families: stats.extinct_families,
                largest_family: stats.largest_family,
            };
        }
    }

    /// 获取缓存的统计数据
    pub fn stats(&self) -> &CachedStats {
        &self.cached_stats
    }

    /// 渲染面板，返回面板操作
    pub fn render(&mut self, ui: &mut Ui, fps: f64, store: &Store) -> PanelAction {
        let mut action = PanelAction::default();

        ui.heading("Cell World");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("FPS:");
            ui.label(format!("{:.1}", fps));
        });

        ui.horizontal(|ui| {
            ui.label("时间:");
            ui.label(format!("{:.1}s", self.cached_stats.time));
        });

        ui.separator();
        ui.label("种群统计");

        // 生物数量 + 添加按钮 + 下拉选择
        ui.horizontal(|ui| {
            ui.label("生物数量:");
            ui.label(format!("{}", self.cached_stats.creature_count));
        });

        ui.horizontal(|ui| {
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

        ui.horizontal(|ui| {
            ui.label("能量粒子:");
            ui.label(format!("{}", self.cached_stats.energy_particle_count));
        });

        ui.horizontal(|ui| {
            ui.label("存活家族:");
            ui.label(format!("{}", self.cached_stats.alive_families));
        });

        ui.horizontal(|ui| {
            ui.label("灭绝家族:");
            ui.label(format!("{}", self.cached_stats.extinct_families));
        });

        ui.horizontal(|ui| {
            ui.label("最大家族:");
            ui.label(format!("{}", self.cached_stats.largest_family));
        });

        action
    }

    /// 渲染选中信息，返回操作
    pub fn render_selection(&mut self, ui: &mut Ui, selection: &Selection, world: &World) -> PanelAction {
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
