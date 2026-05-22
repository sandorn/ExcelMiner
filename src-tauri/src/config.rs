use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// 全局应用配置（存储在 %APPDATA%/ExcelMiner/config.toml）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub defaults: DefaultConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_language")]
    pub language: String,

    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default)]
    pub recent_projects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultConfig {
    #[serde(default)]
    pub default_data_folder: String,

    #[serde(default)]
    pub default_output_folder: String,

    #[serde(default = "default_api_url")]
    pub api_url: String,

    #[serde(default = "default_model")]
    pub model: String,

    #[serde(default)]
    pub system_prompt_path: String,
}

fn default_language() -> String {
    "zh-CN".into()
}
fn default_theme() -> String {
    "light".into()
}
fn default_api_url() -> String {
    "https://api.deepseek.com/v1/chat/completions".into()
}
fn default_model() -> String {
    "deepseek-chat".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                language: default_language(),
                theme: default_theme(),
                recent_projects: vec![],
            },
            defaults: DefaultConfig {
                default_data_folder: String::new(),
                default_output_folder: String::new(),
                api_url: default_api_url(),
                model: default_model(),
                system_prompt_path: String::new(),
            },
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            theme: default_theme(),
            recent_projects: vec![],
        }
    }
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            default_data_folder: String::new(),
            default_output_folder: String::new(),
            api_url: default_api_url(),
            model: default_model(),
            system_prompt_path: String::new(),
        }
    }
}

impl AppConfig {
    /// 获取配置文件路径
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ExcelMiner")
            .join("config.toml")
    }

    /// 加载全局配置
    pub fn load() -> AppResult<Self> {
        let path = Self::config_path();
        if !path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    /// 保存全局配置
    pub fn save(&self) -> AppResult<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| AppError::Config(e.to_string()))?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}
