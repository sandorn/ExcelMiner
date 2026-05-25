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
use crate::services::report_writer::ReportWriter;

/// 预览导入（扫描文件发现数据）
#[tauri::command]
pub async fn preview_import(
    project: Project,
    engine: String,
) -> Result<PreviewData, AppError> {
    let engine = get_engine(&engine)?;
    engine.preview(&project)
}

/// 执行数据汇总（合并模式：仅替换同引擎的结果，保留其他引擎结果）
/// 汇总完成后自动回写到输出文件，确保后续 AI 分析能读取到最新数据
#[tauri::command]
pub async fn execute_aggregation(
    state: State<'_, AppState>,
    project: Project,
    engines: Vec<String>,
    window: Window,
) -> Result<Vec<AggregationResult>, AppError> {
    let mut new_results = Vec::new();

    for (i, engine_key) in engines.iter().enumerate() {
        let engine = get_engine(engine_key)?;

        let _ = window.emit("aggregation-progress", serde_json::json!({
            "step": format!("正在执行: {} ({}/{})", engine.name(), i + 1, engines.len()),
            "progress": i as f64 / engines.len() as f64,
            "status": "running",
            "engine": engine.name(),
        }));

        let result = engine.execute(&project)?;
        tracing::info!(
            "[汇总] {}: 公司数={} 指标数={} 警告数={}",
            engine.name(),
            result.companies_processed,
            result.indicators_collected,
            result.warnings.len()
        );
        for w in &result.warnings {
            tracing::warn!("[汇总] {} - {}", engine.name(), w);
        }
        new_results.push(result);
    }

    let _ = window.emit("aggregation-progress", serde_json::json!({
        "step": "汇总完成",
        "progress": 1.0,
        "status": "done",
        "engine": null,
    }));

    // 合并：保留未被本次运行覆盖的引擎结果
    let run_engine_names: Vec<String> = engines
        .iter()
        .filter_map(|e| get_engine(e).ok())
        .map(|eng| eng.name().to_string())
        .collect();

    let mut stored = state.aggregation_results.lock().await;
    stored.retain(|r| !run_engine_names.contains(&r.engine_name));
    stored.extend(new_results.clone());

    // ✅ 回写到输出文件，确保后续板块分析和公司分析能从汇总表读取最新数据
    let all_results = stored.clone();
    drop(stored);
    if !all_results.is_empty() {
        match ReportWriter::write_summary(
            &project.output_file,
            &all_results,
            &[], // 汇总阶段无 AI 结果
            &project.name,
            project.year,
            project.month,
        ) {
            Ok(()) => {
                tracing::info!("[汇总] 已回写到输出文件: {}", project.output_file.display());
            }
            Err(e) => {
                tracing::error!("[汇总] 回写输出文件失败: {}", e);
            }
        }
    }

    Ok(new_results)
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
