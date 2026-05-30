//! 数据导入命令

use tauri::{Emitter, State, Window};
use tokio::task::JoinSet;

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
    // ── Phase 1: 启动所有引擎（同时并行执行）─────────────────────────────
    let total = engines.len();
    let mut spawn_errors: Vec<String> = Vec::new();
    let mut set = JoinSet::new();

    for engine_key in &engines {
        let engine = match get_engine(engine_key) {
            Ok(e) => e,
            Err(e) => {
                spawn_errors.push(format!("{}: {}", engine_key, e));
                continue;
            }
        };
        let project = project.clone();
        let engine_key_label = engine_key.clone();

        set.spawn(async move {
            let engine_name = engine.name().to_string();
            let result = engine.execute(&project);
            (engine_name, engine_key_label, result)
        });
    }

    for err in &spawn_errors {
        tracing::error!("[汇总] 引擎创建失败: {}", err);
    }

    // ── Phase 2: 收集结果（每个引擎完成时推送进度）─────────────────────────
    let spawned = total.saturating_sub(spawn_errors.len());
    let _ = window.emit("aggregation-progress", serde_json::json!({
        "step": format!("汇总进行中: 0/{} 引擎完成", spawned),
        "progress": 0.0,
        "status": "running",
        "engine": null,
    }));

    let mut new_results: Vec<AggregationResult> = Vec::with_capacity(spawned);
    let mut completed = 0usize;

    while let Some(task_result) = set.join_next().await {
        completed += 1;
        match task_result {
            Ok((engine_name, _engine_key, Ok(result))) => {
                tracing::info!(
                    "[汇总] {} ({}/{}): 公司数={} 指标数={} 警告数={}",
                    engine_name, completed, spawned,
                    result.companies_processed,
                    result.indicators_collected,
                    result.warnings.len()
                );
                for w in &result.warnings {
                    tracing::warn!("[汇总] {} - {}", engine_name, w);
                }
                let _ = window.emit("aggregation-progress", serde_json::json!({
                    "step": format!("已完成: {} ({}/{})", engine_name, completed, spawned),
                    "progress": completed as f64 / spawned as f64,
                    "status": "running",
                    "engine": engine_name,
                }));
                new_results.push(result);
            }
            Ok((engine_name, _engine_key, Err(e))) => {
                tracing::error!("[汇总] {} 执行失败: {}", engine_name, e);
                let _ = window.emit("aggregation-progress", serde_json::json!({
                    "step": format!("失败: {}", engine_name),
                    "progress": completed as f64 / spawned as f64,
                    "status": "error",
                    "engine": engine_name,
                    "error": e.to_string(),
                }));
            }
            Err(join_err) => {
                tracing::error!("[汇总] 引擎任务异常退出: {}", join_err);
            }
        }
    }

    // ── Phase 3: 合并（保留未被本次运行覆盖的引擎结果）────────────────────
    let run_engine_names: Vec<String> = engines
        .iter()
        .filter_map(|e| get_engine(e).ok())
        .map(|eng| eng.name().to_string())
        .collect();

    let mut stored = state.aggregation_results.lock().await;
    stored.retain(|r| !run_engine_names.contains(&r.engine_name));
    stored.extend(new_results.clone());

    // 回写到输出文件，确保后续板块分析和公司分析能从汇总表读取最新数据
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
