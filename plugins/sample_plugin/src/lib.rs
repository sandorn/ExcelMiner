//! ExcelMiner 示例插件
//!
//! 演示如何实现 `EnginePlugin` trait 并通过 `create_engine` 导出。
//! 此插件不执行实际汇总，仅作为开发模板参考。

use excelminer_lib::error::AppResult;
use excelminer_lib::models::analysis::{AggregationResult, PreviewData};
use excelminer_lib::models::project::Project;
use excelminer_lib::services::engine_plugin::EnginePlugin;

/// 示例插件结构体
struct SamplePlugin;

impl EnginePlugin for SamplePlugin {
    fn plugin_id(&self) -> &str {
        "sample"
    }

    fn display_name(&self) -> &str {
        "示例数据汇总"
    }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        let _ = project; // 实际插件应从 project.data_folder 扫描文件
        Ok(PreviewData {
            engine_name: self.display_name().into(),
            files_found: vec![],
            sheets_detected: vec![],
            companies_detected: vec![],
            available_indicators: vec!["示例指标A".into(), "示例指标B".into()],
            warnings: vec!["示例插件：请在 data_folder 下放置数据源文件".into()],
        })
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        let _ = project;
        Ok(AggregationResult {
            engine_name: self.display_name().into(),
            companies_processed: 0,
            indicators_collected: 0,
            summary_data: String::new(),
            warnings: vec!["示例插件：请实现实际汇总逻辑".into()],
        })
    }
}

/// 插件入口符号（必须导出）
///
/// ExcelMiner 启动时通过 `libloading` 查找并调用此函数获取引擎实例。
#[no_mangle]
pub extern "C" fn create_engine() -> *mut dyn EnginePlugin {
    Box::into_raw(Box::new(SamplePlugin))
}
