use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 业态类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum BusinessType {
    Insurance,
    Hotel,
    Commercial,
}

impl std::fmt::Display for BusinessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusinessType::Insurance => write!(f, "保险"),
            BusinessType::Hotel => write!(f, "酒店"),
            BusinessType::Commercial => write!(f, "商写"),
        }
    }
}

/// 公司实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Company {
    /// 公司名称
    pub name: String,
    /// 所属业态
    pub business_type: BusinessType,
    /// 区域列表（酒店业态用，如 ["餐饮", "客房", "会议"]）
    #[serde(default)]
    pub regions: Vec<String>,
}

/// 项目实体（对应一个月份的工作目录）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub year: u32,
    pub month: u32,
    /// 子公司数据根目录
    pub data_folder: PathBuf,
    /// 汇总输出文件路径
    pub output_file: PathBuf,
    /// 子公司列表
    pub companies: Vec<Company>,
    /// YTD 累计月份数
    #[serde(default = "default_ytd_months")]
    pub ytd_months: u32,
    /// AI 配置
    #[serde(default)]
    pub ai_config: AIConfig,
}

fn default_ytd_months() -> u32 {
    1
}

/// AI API 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// 系统提示词文件路径
    #[serde(default)]
    pub system_prompt_path: PathBuf,
    /// 每批分析公司数
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 质量评分阈值 (0-10)
    #[serde(default = "default_quality_threshold")]
    pub quality_threshold: u32,
}

fn default_api_url() -> String {
    "https://api.deepseek.com/v1/chat/completions".into()
}
fn default_model() -> String {
    "deepseek-chat".into()
}
fn default_temperature() -> f64 {
    0.3
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_batch_size() -> usize {
    3
}
fn default_max_retries() -> u32 {
    3
}
fn default_quality_threshold() -> u32 {
    8
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            api_key: String::new(),
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            system_prompt_path: PathBuf::new(),
            batch_size: default_batch_size(),
            max_retries: default_max_retries(),
            quality_threshold: default_quality_threshold(),
        }
    }
}

/// 项目配置文件（可序列化为 .toml）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectConfigInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfigInner {
    pub name: String,
    pub year: u32,
    pub month: u32,
    pub data_folder: String,
    pub output_file: String,
    pub ytd_months: u32,
    pub companies: Vec<CompanyConfig>,
    #[serde(default)]
    pub ai: AIConfigToml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyConfig {
    pub name: String,
    pub business_type: String,
    #[serde(default)]
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfigToml {
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub system_prompt_path: String,
    pub batch_size: Option<usize>,
    pub max_retries: Option<u32>,
    pub quality_threshold: Option<u32>,
}

impl Default for AIConfigToml {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            api_key: String::new(),
            model: default_model(),
            temperature: Some(default_temperature()),
            max_tokens: Some(default_max_tokens()),
            system_prompt_path: String::new(),
            batch_size: Some(default_batch_size()),
            max_retries: Some(default_max_retries()),
            quality_threshold: Some(default_quality_threshold()),
        }
    }
}

impl Project {
    /// 从 ProjectConfig 转换
    pub fn from_config(config: ProjectConfig) -> Self {
        let inner = config.project;
        let ai = inner.ai;
        Self {
            name: inner.name,
            year: inner.year,
            month: inner.month,
            data_folder: PathBuf::from(inner.data_folder),
            output_file: PathBuf::from(inner.output_file),
            companies: inner
                .companies
                .into_iter()
                .map(|c| Company {
                    name: c.name,
                    business_type: match c.business_type.as_str() {
                        "Insurance" | "insurance" => BusinessType::Insurance,
                        "Hotel" | "hotel" => BusinessType::Hotel,
                        "Commercial" | "commercial" => BusinessType::Commercial,
                        _ => BusinessType::Commercial,
                    },
                    regions: c.regions,
                })
                .collect(),
            ytd_months: inner.ytd_months,
            ai_config: AIConfig {
                api_url: ai.api_url,
                api_key: ai.api_key,
                model: ai.model,
                temperature: ai.temperature.unwrap_or(default_temperature()),
                max_tokens: ai.max_tokens.unwrap_or(default_max_tokens()),
                system_prompt_path: PathBuf::from(ai.system_prompt_path),
                batch_size: ai.batch_size.unwrap_or(default_batch_size()),
                max_retries: ai.max_retries.unwrap_or(default_max_retries()),
                quality_threshold: ai.quality_threshold.unwrap_or(default_quality_threshold()),
            },
        }
    }

    /// 转为 ProjectConfig
    pub fn to_config(&self) -> ProjectConfig {
        ProjectConfig {
            project: ProjectConfigInner {
                name: self.name.clone(),
                year: self.year,
                month: self.month,
                data_folder: self.data_folder.to_string_lossy().to_string(),
                output_file: self.output_file.to_string_lossy().to_string(),
                ytd_months: self.ytd_months,
                companies: self
                    .companies
                    .iter()
                    .map(|c| CompanyConfig {
                        name: c.name.clone(),
                        business_type: format!("{:?}", c.business_type),
                        regions: c.regions.clone(),
                    })
                    .collect(),
                ai: AIConfigToml {
                    api_url: self.ai_config.api_url.clone(),
                    api_key: self.ai_config.api_key.clone(),
                    model: self.ai_config.model.clone(),
                    temperature: Some(self.ai_config.temperature),
                    max_tokens: Some(self.ai_config.max_tokens),
                    system_prompt_path: self
                        .ai_config
                        .system_prompt_path
                        .to_string_lossy()
                        .to_string(),
                    batch_size: Some(self.ai_config.batch_size),
                    max_retries: Some(self.ai_config.max_retries),
                    quality_threshold: Some(self.ai_config.quality_threshold),
                },
            },
        }
    }
}
