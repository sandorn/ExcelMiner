//! AI 分析命令

use tauri::{Emitter, Window};

use crate::error::{AppError};
use crate::models::analysis::{AnalysisResult, ProgressUpdate, ProgressStatus};
use crate::models::project::{BusinessType, Project};
use crate::services::ai_analyzer::AIAnalyzer;

/// 执行 AI 业态分析
#[tauri::command]
pub async fn execute_analysis(
    project: Project,
    business_types: Vec<String>,
    window: Window,
) -> Result<Vec<AnalysisResult>, AppError> {
    if project.ai_config.api_key.is_empty() {
        return Err(AppError::Config("请先配置 DeepSeek API Key".into()));
    }

    let analyzer = AIAnalyzer::new(project.ai_config.clone())?;
    let mut all_results = Vec::new();

    for bt_name in &business_types {
        let bt = parse_business_type(bt_name)?;

        // 筛选该业态的公司
        let companies: Vec<_> = project
            .companies
            .iter()
            .filter(|c| c.business_type == bt)
            .collect();

        if companies.is_empty() {
            continue;
        }

        // 为每个公司构建数据文本
        let companies_data: Vec<(String, String)> = companies
            .iter()
            .map(|c| {
                (
                    c.name.clone(),
                    format!(
                        "公司: {}\n业态: {}\n（请等待 Phase 3 完整数据汇总后，此处将包含实际经营数据）",
                        c.name, bt
                    ),
                )
            })
            .collect();

        let total = companies_data.len();
        // 分批分析
        let results = analyzer
            .analyze_batch(bt.clone(), &companies_data, |update| {
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

    Ok(all_results)
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
