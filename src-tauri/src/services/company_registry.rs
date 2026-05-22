//! 子公司注册表 — 从 resources/companies.toml 加载公司配置
//!
//! 替代各聚合器中硬编码的 COMPANIES 数组

use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

/// 公司注册表（从 companies.toml 加载）
#[derive(Debug, Clone, Deserialize)]
pub struct CompanyRegistry {
    #[serde(default)]
    pub insurance: Vec<CompanyEntry>,
    #[serde(default)]
    pub commercial: Vec<CompanyEntry>,
    #[serde(default)]
    pub hotel: Vec<HotelEntry>,
}

/// 通用公司条目
#[derive(Debug, Clone, Deserialize)]
pub struct CompanyEntry {
    pub name: String,
}

/// 酒店业态公司条目（含达成列偏移标志）
#[derive(Debug, Clone, Deserialize)]
pub struct HotelEntry {
    pub name: String,
    /// true=伯豪瑞廷(E列起始), false=重庆瑞尔(D列起始)
    #[serde(default)]
    pub is_bhrt: bool,
}

impl CompanyRegistry {
    /// 加载公司配置，按优先级尝试多个路径
    pub fn load() -> AppResult<Self> {
        for candidate in &Self::candidate_paths() {
            if candidate.exists() {
                let content = std::fs::read_to_string(candidate)
                    .map_err(|e| AppError::Config(format!("读取公司配置失败: {}", e)))?;
                let registry: Self = toml::from_str(&content)
                    .map_err(|e| AppError::Config(format!("解析公司配置失败: {}", e)))?;
                tracing::info!("从 {} 加载了公司配置", candidate.display());
                return Ok(registry);
            }
        }
        tracing::warn!("未找到 companies.toml 文件，使用内嵌默认配置");
        Ok(Self::default())
    }

    /// 搜索路径优先级:
    /// 1. CWD/resources/companies.toml (dev: 项目根目录)
    /// 2. exe_dir/resources/companies.toml (生产: 安装目录)
    /// 3. ../resources/companies.toml (dev alternative)
    fn candidate_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join("resources").join("companies.toml"));
            paths.push(cwd.join("..").join("resources").join("companies.toml"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join("resources").join("companies.toml"));
            }
        }

        paths
    }

    /// 根据文件名查找公司（用于汇总引擎定位源文件）
    pub fn find_company(&self, name: &str) -> Option<&CompanyEntry> {
        self.insurance
            .iter()
            .chain(self.commercial.iter())
            .find(|c| c.name == name)
    }

    /// 查找酒店公司
    pub fn find_hotel(&self, name: &str) -> Option<&HotelEntry> {
        self.hotel.iter().find(|c| c.name == name)
    }
}

impl Default for CompanyRegistry {
    fn default() -> Self {
        Self {
            insurance: vec![
                CompanyEntry { name: "盛唐融信".into() },
                CompanyEntry { name: "君康经纪".into() },
            ],
            commercial: vec![
                CompanyEntry { name: "北京中言".into() },
                CompanyEntry { name: "大连凯丹".into() },
                CompanyEntry { name: "福建钱隆".into() },
                CompanyEntry { name: "春夏秋冬".into() },
                CompanyEntry { name: "重庆宜新".into() },
            ],
            hotel: vec![
                HotelEntry { name: "伯豪瑞廷".into(), is_bhrt: true },
                HotelEntry { name: "重庆瑞尔".into(), is_bhrt: false },
            ],
        }
    }
}

/// 懒加载全局单例（首次调用时从文件加载，后续复用缓存）
pub fn company_registry() -> &'static CompanyRegistry {
    use std::sync::OnceLock;
    static REGISTRY: OnceLock<CompanyRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| CompanyRegistry::load().unwrap_or_default())
}
