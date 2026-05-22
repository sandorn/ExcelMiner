//! AI 分析命令

use tauri::{Emitter, State, Window};

use crate::commands::project_cmd::AppState;
use crate::error::{AppError};
use crate::models::analysis::{AnalysisResult, AggregationResult, ProgressUpdate, ProgressStatus};
use crate::models::project::{BusinessType, Project};
use crate::services::ai_analyzer::AIAnalyzer;

/// 执行 AI 业态分析
#[tauri::command]
pub async fn execute_analysis(
    state: State<'_, AppState>,
    project: Project,
    business_types: Vec<String>,
    custom_prompt: Option<String>,
    window: Window,
) -> Result<Vec<AnalysisResult>, AppError> {
    if project.ai_config.api_key.is_empty() {
        return Err(AppError::Config("请先配置 DeepSeek API Key".into()));
    }

    let mut ai_config = project.ai_config.clone();
    if custom_prompt.is_some() {
        ai_config.system_prompt_path = std::path::PathBuf::new();
    }

    // 从状态中读取汇总数据，构建公司数据文本
    let agg_results = state.aggregation_results.lock().await;
    let agg_data_map = build_company_data_map(&agg_results);

    let analyzer = AIAnalyzer::new(ai_config)?;
    let mut all_results = Vec::new();

    for bt_name in &business_types {
        let bt = parse_business_type(bt_name)?;

        let companies: Vec<_> = project
            .companies
            .iter()
            .filter(|c| c.business_type == bt)
            .collect();

        if companies.is_empty() {
            continue;
        }

        // 为每个公司构建数据文本，优先从汇总结果取实际数据
        let companies_data: Vec<(String, String)> = companies
            .iter()
            .map(|c| {
                let data_text = agg_data_map
                    .get(&c.name)
                    .cloned()
                    .unwrap_or_else(|| {
                        format!(
                            "公司: {}\n业态: {}\n（未找到汇总数据，请先执行步骤二）",
                            c.name, bt
                        )
                    });
                (c.name.clone(), data_text)
            })
            .collect();

        let results = analyzer
            .analyze_batch(bt.clone(), &companies_data, custom_prompt.as_deref(), |update| {
                let _ = window.emit("analysis-progress", &update);
            })
            .await?;

        all_results.extend(results);
    }

    let _ = window.emit("analysis-progress", ProgressUpdate {
        step: "全部分析完成".into(),
        progress: 1.0,
        status: ProgressStatus::Done,
        company: None,
    });

    // 存储到 AppState 供导出步骤使用
    *state.analysis_results.lock().await = all_results.clone();

    Ok(all_results)
}

/// 从汇总结果中提取按公司分组的数据文本
fn build_company_data_map(
    agg_results: &[AggregationResult],
) -> std::collections::HashMap<String, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for result in agg_results {
        if let Ok(companies) =
            serde_json::from_str::<Vec<serde_json::Value>>(&result.summary_data)
        {
            for co in &companies {
                if let Some(name) = co.get("company").and_then(|v| v.as_str()) {
                    let entry = map.entry(name.to_string()).or_default();
                    entry.push_str(&format!("\n## {}\n", result.engine_name));
                    if let Some(obj) = co.as_object() {
                        for (k, v) in obj {
                            if k == "company" { continue; }
                            entry.push_str(&format!("{}: {}\n", k, v));
                        }
                    }
                }
            }
        }
    }

    map
}

/// 测试 API 连接
#[tauri::command]
pub async fn test_api_connection(
    api_url: String,
    api_key: String,
    model: String,
) -> Result<String, AppError> {
    let config = crate::models::project::AIConfig {
        api_url,
        api_key,
        model,
        ..Default::default()
    };
    let analyzer = AIAnalyzer::new(config)?;
    let (response, _) = analyzer
        .call("你是一个测试助手。", "请回复'连接成功'")
        .await?;
    Ok(response)
}

fn parse_business_type(name: &str) -> Result<BusinessType, AppError> {
    match name {
        "insurance" | "Insurance" | "保险" => Ok(BusinessType::Insurance),
        "hotel" | "Hotel" | "酒店" => Ok(BusinessType::Hotel),
        "commercial" | "Commercial" | "商写" => Ok(BusinessType::Commercial),
        _ => Err(AppError::Other(format!("未知业态: {}", name))),
    }
}
