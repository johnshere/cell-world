use std::fs;
use std::path::PathBuf;

use chrono::NaiveDateTime;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::neural::Genome;

/// 从文件名中解析时间戳
/// 匹配格式: v版本_月日_时分 (如 v2.3.0_0422_1721)
/// 返回当年的时间（用于排序，不用于显示）
fn parse_file_time(name: &str) -> Option<NaiveDateTime> {
    // 匹配模式: _月日_时分 或 -月日_时分
    let re = Regex::new(r"_(\d{2})(\d{2})_(\d{2})(\d{2})\.json$").ok()?;
    let caps = re.captures(name)?;

    let month: u32 = caps.get(1)?.as_str().parse().ok()?;
    let day: u32 = caps.get(2)?.as_str().parse().ok()?;
    let hour: u32 = caps.get(3)?.as_str().parse().ok()?;
    let minute: u32 = caps.get(4)?.as_str().parse().ok()?;

    // 使用当前年份（用于排序）
    let year = chrono::Local::now().format("%Y").to_string();
    let date_str = format!("{}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, minute);

    NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d %H:%M").ok()
}

/// 保存的生物模板
#[derive(Clone, Serialize, Deserialize)]
pub struct CreatureTemplate {
    /// 自定义名称
    pub name: String,
    /// 基因组
    pub genome: Genome,
    /// 初始能量
    pub initial_energy: f64,
    /// 记录时的版本号
    #[serde(default)]
    pub version: Option<String>,
    /// 评分
    #[serde(default)]
    pub score: Option<f64>,
    /// 种群占比
    #[serde(default)]
    pub population_ratio: Option<f64>,
    /// 平均能量
    #[serde(default)]
    pub avg_energy: Option<f64>,
    /// 平均年龄
    #[serde(default)]
    pub avg_age: Option<f64>,
    /// 最大世代
    #[serde(default)]
    pub max_generation: Option<usize>,
    /// 保存时的代数（None=旧格式无记录，投放时从0开始）
    #[serde(default)]
    pub generation: Option<usize>,
    /// 记录时的世界时间
    #[serde(default)]
    pub recorded_at: Option<f64>,
    /// true=自动检测记录, None/false=手动保存
    #[serde(default)]
    pub auto_recorded: Option<bool>,
    /// 文件名中的时间（从文件名解析，不持久化）
    #[serde(skip)]
    pub file_time: Option<NaiveDateTime>,
}

/// 生物模板存储
pub struct Store {
    /// 存储目录
    dir: PathBuf,
    /// 已加载的模板
    templates: Vec<CreatureTemplate>,
}

impl Store {
    /// 创建存储，自动加载已有模板
    pub fn new() -> Self {
        let dir = PathBuf::from("store");
        if !dir.exists() {
            fs::create_dir_all(&dir).ok();
        }

        let mut store = Self {
            dir,
            templates: Vec::new(),
        };
        store.load_all();
        store
    }

    /// 加载所有模板
    pub fn load_all(&mut self) {
        self.templates.clear();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let filename = match path.file_name().and_then(|n| n.to_str()) {
                    Some(f) => f.to_string(),
                    None => continue,
                };
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut template) = serde_json::from_str::<CreatureTemplate>(&content)
                        {
                            template.genome.ensure_sorted_cache();
                            // 从文件名解析时间
                            template.file_time = parse_file_time(&filename);
                            self.templates.push(template);
                        }
                    }
                }
            }
        }
        // 按文件名时间倒序（最新在前），无时间戳的排最后
        self.templates.sort_by(|a, b| {
            let ta = a.file_time;
            let tb = b.file_time;
            match (ta, tb) {
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.recorded_at.unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&a.recorded_at.unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal),
                (Some(a_time), Some(b_time)) => b_time.cmp(&a_time),
            }
        });
    }

    /// 保存模板
    pub fn save(&mut self, mut template: CreatureTemplate) -> Result<(), String> {
        let filename = format!("{}.json", sanitize_filename(&template.name));
        let path = self.dir.join(&filename);

        // 从文件名解析时间
        template.file_time = parse_file_time(&filename);

        let content =
            serde_json::to_string_pretty(&template).map_err(|e| format!("序列化失败: {}", e))?;

        fs::write(&path, content).map_err(|e| format!("写入失败: {}", e))?;

        // 更新内存中的列表
        if let Some(existing) = self.templates.iter_mut().find(|t| t.name == template.name) {
            *existing = template;
        } else {
            self.templates.push(template);
            self.templates.sort_by(|a, b| {
                let ta = a.file_time;
                let tb = b.file_time;
                match (ta, tb) {
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => b.recorded_at.unwrap_or(f64::NEG_INFINITY)
                        .partial_cmp(&a.recorded_at.unwrap_or(f64::NEG_INFINITY))
                        .unwrap_or(std::cmp::Ordering::Equal),
                    (Some(a_time), Some(b_time)) => b_time.cmp(&a_time),
                }
            });
        }

        Ok(())
    }

    /// 获取所有模板
    pub fn templates(&self) -> &[CreatureTemplate] {
        &self.templates
    }

    /// 按名称获取模板
    pub fn get(&self, name: &str) -> Option<&CreatureTemplate> {
        self.templates.iter().find(|t| t.name == name)
    }

    /// 获取模板名称列表
    pub fn names(&self) -> Vec<&str> {
        self.templates.iter().map(|t| t.name.as_str()).collect()
    }

    /// 删除模板
    pub fn delete(&mut self, name: &str) {
        self.templates.retain(|t| t.name != name);
        let filename = format!("{}.json", sanitize_filename(name));
        let path = self.dir.join(&filename);
        let _ = fs::remove_file(path);
    }

    /// 清空优势种数据
    pub fn clear_dominant(&mut self) {
        // 删除文件
        for template in &self.templates {
            if template.auto_recorded == Some(true) {
                let filename = format!("{}.json", sanitize_filename(&template.name));
                let path = self.dir.join(&filename);
                let _ = fs::remove_file(path);
            }
        }
        // 从内存中移除
        self.templates.retain(|t| t.auto_recorded != Some(true));
    }

    /// 获取优势种列表
    pub fn dominant_species(&self) -> Vec<crate::world::DominantCandidate> {
        self.templates
            .iter()
            .filter(|t| t.auto_recorded == Some(true))
            .map(|t| crate::world::DominantCandidate {
                genome: t.genome.clone(),
                genome_hash: 0, // 或许需要计算，但暂时0
                score: t.score.unwrap_or(0.0),
                population_ratio: t.population_ratio.unwrap_or(0.0),
                avg_energy: t.avg_energy.unwrap_or(0.0),
                avg_age: t.avg_age.unwrap_or(0.0),
                max_generation: t.max_generation.unwrap_or(0),
            })
            .collect()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// 清理文件名，移除非法字符
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}
