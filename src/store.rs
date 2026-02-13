use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::neural::Genome;

/// 保存的生物模板
#[derive(Clone, Serialize, Deserialize)]
pub struct CreatureTemplate {
    /// 自定义名称
    pub name: String,
    /// 基因组
    pub genome: Genome,
    /// 初始能量
    pub initial_energy: f64,
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
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(template) = serde_json::from_str::<CreatureTemplate>(&content) {
                            self.templates.push(template);
                        }
                    }
                }
            }
        }
        // 按名称排序
        self.templates.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// 保存模板
    pub fn save(&mut self, template: CreatureTemplate) -> Result<(), String> {
        let filename = format!("{}.json", sanitize_filename(&template.name));
        let path = self.dir.join(&filename);

        let content = serde_json::to_string_pretty(&template)
            .map_err(|e| format!("序列化失败: {}", e))?;

        fs::write(&path, content)
            .map_err(|e| format!("写入失败: {}", e))?;

        // 更新内存中的列表
        if let Some(existing) = self.templates.iter_mut().find(|t| t.name == template.name) {
            *existing = template;
        } else {
            self.templates.push(template);
            self.templates.sort_by(|a, b| a.name.cmp(&b.name));
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
