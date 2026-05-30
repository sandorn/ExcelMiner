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

    /// AI 相关可调参数（超时、重试、并发等）
    #[serde(default)]
    pub tuning: TuningConfig,
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

/// AI/网络调优参数，可通过环境变量覆盖
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningConfig {
    /// AI API 超时秒数
    #[serde(default = "default_timeout_secs")]
    pub api_timeout_secs: u64,
    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 初始退避毫秒
    #[serde(default = "default_backoff_ms")]
    pub retry_initial_backoff_ms: u64,
    /// 最大并发请求数
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,
}

fn default_language() -> String { "zh-CN".into() }
fn default_theme() -> String { "light".into() }
fn default_api_url() -> String { "https://api.deepseek.com/v1/chat/completions".into() }
fn default_model() -> String { "deepseek-v4-pro".into() }
fn default_timeout_secs() -> u64 { 60 }
fn default_max_retries() -> u32 { 2 }
fn default_backoff_ms() -> u64 { 2000 }
fn default_max_concurrent() -> usize { 3 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            defaults: DefaultConfig::default(),
            tuning: TuningConfig::default(),
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

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            api_timeout_secs: default_timeout_secs(),
            max_retries: default_max_retries(),
            retry_initial_backoff_ms: default_backoff_ms(),
            max_concurrent_requests: default_max_concurrent(),
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

    /// 加载全局配置（文件不存在时自动创建默认配置并保存）
    pub fn load() -> AppResult<Self> {
        let path = Self::config_path();
        let mut config = if !path.exists() {
            let c = Self::default();
            c.save()?;
            c
        } else {
            let content = std::fs::read_to_string(&path)?;
            toml::from_str(&content)?
        };

        // 环境变量覆盖（三层合并：defaults → 文件 → env）
        config.apply_env_overrides();
        Ok(config)
    }

    /// 用环境变量覆盖配置（优先级最高）
    /// 支持: EXCELMINER_API_URL, EXCELMINER_API_KEY, EXCELMINER_MODEL,
    ///       EXCELMINER_TIMEOUT, EXCELMINER_MAX_RETRIES, EXCELMINER_CONCURRENT
    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("EXCELMINER_API_URL") {
            if !v.is_empty() {
                tracing::info!("[Config] ENV覆盖 api_url");
                self.defaults.api_url = v;
            }
        }
        if let Ok(v) = std::env::var("EXCELMINER_MODEL") {
            if !v.is_empty() {
                tracing::info!("[Config] ENV覆盖 model={}", v);
                self.defaults.model = v;
            }
        }
        if let Ok(v) = std::env::var("EXCELMINER_TIMEOUT") {
            if let Ok(s) = v.parse::<u64>() {
                tracing::info!("[Config] ENV覆盖 api_timeout={}s", s);
                self.tuning.api_timeout_secs = s;
            }
        }
        if let Ok(v) = std::env::var("EXCELMINER_MAX_RETRIES") {
            if let Ok(n) = v.parse::<u32>() {
                tracing::info!("[Config] ENV覆盖 max_retries={}", n);
                self.tuning.max_retries = n;
            }
        }
        if let Ok(v) = std::env::var("EXCELMINER_CONCURRENT") {
            if let Ok(n) = v.parse::<usize>() {
                tracing::info!("[Config] ENV覆盖 max_concurrent={}", n);
                self.tuning.max_concurrent_requests = n;
            }
        }
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

    /// 从环境变量读取 API Key（优先级：ENV > 项目配置）
    /// 环境变量名: EXCELMINER_API_KEY
    pub fn api_key_from_env() -> Option<String> {
        std::env::var("EXCELMINER_API_KEY").ok().filter(|v| !v.is_empty())
    }
}
