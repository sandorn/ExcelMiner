//! 数据导入命令

use tauri::{Emitter, State, Window};

use crate::commands::project_cmd::AppState;
use crate::error::AppError;
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::Project;
use crate::services::data_aggregator::{
    AggregationEngine,
    insurance::InsuranceAggregator,
    hotel::HotelAggregator,
    commercial::CommercialAggregator,
    financial::FinancialAggregator,
};

/// 预览导入（扫描文件发现数据）
#[tauri::command]
pub async fn preview_import(
    project: Project,
    engine: String,
) -> Result<PreviewData, AppError> {
    let engine = get_engine(&engine)?;
    engine.preview(&project)
}

/// 执行数据汇总
#[tauri::command]
pub async fn execute_aggregation(
    state: State<'_, AppState>,
    project: Project,
    engines: Vec<String>,
    window: Window,
) -> Result<Vec<AggregationResult>, AppError> {
    let mut results = Vec::new();

    for (i, engine_name) in engines.iter().enumerate() {
        let engine = get_engine(engine_name)?;

        let _ = window.emit("aggregation-progress", serde_json::json!({
            "step": format!("正在执行: {} ({}/{})", engine.name(), i + 1, engines.len()),
            "progress": i as f64 / engines.len() as f64,
            "status": "running",
            "engine": engine.name(),
        }));

        let result = engine.execute(&project)?;
        results.push(result);
    }

    let _ = window.emit("aggregation-progress", serde_json::json!({
        "step": "汇总完成",
        "progress": 1.0,
        "status": "done",
        "engine": null,
    }));

    // 存储到 AppState 供后续步骤使用
    *state.aggregation_results.lock().await = results.clone();

    Ok(results)
}

fn get_engine(name: &str) -> Result<Box<dyn AggregationEngine>, AppError> {
    match name {
        "insurance" | "保险" => Ok(Box::new(InsuranceAggregator)),
        "hotel" | "酒店" => Ok(Box::new(HotelAggregator)),
        "commercial" | "商写" => Ok(Box::new(CommercialAggregator)),
        "financial" | "经营报表" => Ok(Box::new(FinancialAggregator)),
        _ => Err(AppError::Other(format!("未知汇总引擎: {}", name))),
    }
}
