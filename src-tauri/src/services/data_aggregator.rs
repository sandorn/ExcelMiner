//! 数据汇总引擎集合

pub mod commercial;
pub mod financial;
pub mod hotel;
pub mod insurance;

use crate::error::AppResult;
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;

/// 汇总引擎标识
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineType {
    Insurance,
    Hotel,
    Commercial,
    Financial,
}

impl EngineType {
    pub fn name(&self) -> &str {
        match self {
            EngineType::Insurance => "保险数据汇总",
            EngineType::Hotel => "酒店数据汇总",
            EngineType::Commercial => "商写数据汇总",
            EngineType::Financial => "经营报表汇总",
        }
    }

    pub fn all() -> Vec<EngineType> {
        vec![
            EngineType::Insurance,
            EngineType::Hotel,
            EngineType::Commercial,
            EngineType::Financial,
        ]
    }
}

/// 汇总引擎 trait
pub trait AggregationEngine: Send + Sync {
    fn engine_type(&self) -> EngineType;
    fn name(&self) -> &str;

    /// 预览：读取文件发现数据
    fn preview(&self, project: &Project) -> AppResult<PreviewData>;

    /// 执行汇总
    fn execute(
        &self,
        project: &Project,
    ) -> AppResult<AggregationResult>;
}
