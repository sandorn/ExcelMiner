//! AI 分析命令

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tauri::{Emitter, State, Window};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use calamine::Reader;
use crate::commands::project_cmd::AppState;

use crate::error::{AppError};
use crate::models::analysis::{AnalysisResult, AggregationResult, ProgressUpdate, ProgressStatus};
use crate::models::project::{BusinessType, Project};
use crate::services::ai_analyzer::AIAnalyzer;

/// 阶段一：板块业态分析（跳过质量检查，仅检查内容长度≥50字）
#[tauri::command]
pub async fn execute_segment_analysis(
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

    // ✅ 提前获取汇总数据快照并在使用后立即释放锁，避免后续回写时死锁
    let agg_results = {
        let locked = state.aggregation_results.lock().await;
        locked.clone()
    }; // MutexGuard 在此释放

    let analyzer = AIAnalyzer::new(ai_config)?;
    let mut all_results = Vec::new();
    let total_types = business_types.len();

    for (type_idx, bt_name) in business_types.iter().enumerate() {
        let bt = parse_business_type(bt_name)?;

        let companies: Vec<_> = project
            .companies
            .iter()
            .filter(|c| c.business_type == bt)
            .collect();

        if companies.is_empty() {
            tracing::info!(
                "[板块分析] {}业态无匹配公司（project.companies={}），跳过",
                bt, project.companies.len()
            );
            continue;
        }

        let segment_name = format!("{}板块", bt);
        tracing::info!(
            "[板块分析] 开始分析 {} (公司数:{}, 第{}/{})",
            segment_name, companies.len(), type_idx + 1, total_types
        );

        // ✅ 与AardMiner/VBA原型一致：从汇总表行业Sheet直接读取单元格数据
        let user_data = read_segment_data(&project.output_file, &bt, project.month);

        let user_prompt = if let Some(data) = user_data {
            format!(
                "请对以下{}子公司的经营数据进行分析，并按系统提示词指定格式输出。\n数据表格如下：\n{}",
                bt, data
            )
        } else {
            // 回退：使用内存汇总数据
            tracing::warn!("[板块分析] {} 汇总表无数据，回退到内存汇总", segment_name);
            let engine_name = engine_name_for_business_type(&bt);
            let agg_data_map = build_company_data_map_filtered(&agg_results, Some(engine_name));
            let combined_data: Vec<String> = companies
                .iter()
                .map(|c| {
                    let data_text = agg_data_map
                        .get(&c.name)
                        .cloned()
                        .unwrap_or_else(|| format!("公司: {}\n（未找到汇总数据）", c.name));
                    format!("【{}】\n{}", c.name, data_text)
                })
                .collect();
            format!(
                "请分析以下{}子公司的经营数据：\n\n{}",
                bt,
                combined_data.join("\n\n---\n\n")
            )
        };

        let system_prompt = if let Some(ref p) = custom_prompt {
            if !p.trim().is_empty() {
                p.clone()
            } else {
                analyzer.load_system_prompt(Some(&bt))?
            }
        } else {
            analyzer.load_system_prompt(Some(&bt))?
        };

        let _ = window.emit("analysis-progress", ProgressUpdate {
            step: format!("板块分析: {} (第{}/{}) — 正在调用AI...", segment_name, type_idx + 1, total_types),
            progress: (type_idx as f64 + 1.0) / (total_types as f64),
            status: ProgressStatus::Running,
            company: Some(segment_name.clone()),
        });

        tracing::info!(
            "[板块分析] {} {} → 发送到 DeepSeek (system_prompt={}字, user_prompt={}字)",
            segment_name, bt, system_prompt.chars().count(), user_prompt.chars().count()
        );
        let call_start = std::time::Instant::now();
        let result = analyzer
            .analyze_segment(
                &system_prompt,
                &user_prompt,
                &segment_name,
                &bt.to_string(),
            )
            .await;
        let call_ms = call_start.elapsed().as_millis();
        tracing::info!(
            "[板块分析] {} {} → API返回 耗时={}ms success={} content_len={} err={:?}",
            segment_name, bt, call_ms, result.success, result.content.len(), result.error_message
        );

        if !result.success {
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("{} 分析失败: {}", segment_name, result.error_message.as_deref().unwrap_or("未知错误")),
                progress: (type_idx as f64) / (total_types as f64),
                status: ProgressStatus::Error,
                company: Some(segment_name),
            });
        } else {
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("{} 分析完成 ({:.0}字, {}ms)", segment_name, result.content.len(), call_ms),
                progress: (type_idx as f64 + 1.0) / (total_types as f64),
                status: ProgressStatus::Running,
                company: Some(segment_name.clone()),
            });
        }

        all_results.push(result);
    }

    // 汇总分析结果
    let success_count = all_results.iter().filter(|r| r.success).count();
    let fail_count = all_results.len() - success_count;
    tracing::info!(
        "[板块分析] 完成: {}成功 {}失败",
        success_count, fail_count
    );
    for r in &all_results {
        tracing::info!(
            "[板块分析] {}: success={} len={} err={:?}",
            r.company_name, r.success, r.content.len(), r.error_message
        );
    }

    let _ = window.emit("analysis-progress", ProgressUpdate {
        step: "板块分析完成".into(),
        progress: 1.0,
        status: ProgressStatus::Done,
        company: None,
    });

    // 存储到 AppState（只保留 segment 类结果）
    {
        let mut stored = state.analysis_results.lock().await;
        stored.retain(|r: &AnalysisResult| r.analysis_category != "segment");
        stored.extend(all_results.clone());
        tracing::info!("[板块分析] AppState 中 AI 结果共 {} 条 (segment: {})",
            stored.len(),
            stored.iter().filter(|r| r.analysis_category == "segment").count()
        );
    }

    // 自动回写到模板：板块分析结果 → L14/M14
    {
        let agg = state.aggregation_results.lock().await;
        let ai = state.analysis_results.lock().await;
        tracing::info!(
            "[板块分析] 回写模板: agg={}条 ai={}条 → {}",
            agg.len(), ai.len(), project.output_file.display()
        );
        if let Err(e) = crate::services::report_writer::ReportWriter::write_summary(
            &project.output_file, &agg, &ai,
            &project.name, project.year, project.month,
        ) {
            tracing::error!("[板块分析] 回写模板失败: {}", e);
        } else {
            tracing::info!("[板块分析] ✅ 回写模板成功");
        }
    }

    Ok(all_results)
}

/// 阶段二：子公司经营指标分析（带质量检查）
#[tauri::command]
pub async fn execute_company_analysis(
    state: State<'_, AppState>,
    project: Project,
    window: Window,
) -> Result<Vec<AnalysisResult>, AppError> {
    if project.ai_config.api_key.is_empty() {
        return Err(AppError::Config("请先配置 DeepSeek API Key".into()));
    }

    // ✅ 提前获取汇总数据快照并在使用后立即释放锁，避免后续回写时死锁
    let fallback_data_map = {
        let agg_results = state.aggregation_results.lock().await;
        build_company_data_map_filtered(&agg_results, Some("经营报表汇总"))
    }; // MutexGuard 在此释放

    // 从汇总表模板读取各公司的 B 列指标名
    let indicator_names = read_indicator_names_from_template(&project.output_file)
        .unwrap_or_default();

    let analyzer = Arc::new(AIAnalyzer::new(project.ai_config.clone())?);
    let financial_prompt = Arc::new(analyzer.load_system_prompt(None)?);
    let total = project.companies.len();
    let progress_pct = project.month as f64 / 12.0 * 100.0;

    let semaphore = Arc::new(Semaphore::new(3));
    let completed = Arc::new(AtomicUsize::new(0));
    let mut set = JoinSet::new();

    let output_file = project.output_file.clone();
    let ytd_months = project.ytd_months;

    for company in &project.companies {
        let analyzer = analyzer.clone();
        let prompt = financial_prompt.clone();
        let permit = semaphore.clone();
        let completed = completed.clone();
        let window = window.clone();
        let company_name = company.name.clone();
        // 优先从汇总表 C1:R5 读取，但仅当当前月份有数据时才使用，否则回退到内存数据
        let data_text = read_company_data_from_summary(&output_file, &company_name, ytd_months)
            .filter(|text| has_month_data(text, project.month))
            .or_else(|| {
                let fallback = fallback_data_map.get(&company_name).cloned().unwrap_or_default();
                if fallback.is_empty() { None } else { Some(fallback) }
            })
            .unwrap_or_default();
        let names = indicator_names.get(&company_name).cloned();
        let data_text = apply_indicator_names(&data_text, names.as_ref());
        let year = project.year;
        let month = project.month;
        let progress_pct = progress_pct;

        set.spawn(async move {
            let _permit = permit.acquire_owned().await;
            let user_prompt = format!(
                "公司名称：{}\n年份：{}\n当前月份：{}月\n序时进度：{:.2}%\n数据单位：万元\n请按系统提示词要求输出指定格式。\n\n{}",
                company_name, year, month, progress_pct, data_text
            );

            let result = analyzer
                .analyze_single(
                    &prompt,
                    &user_prompt,
                    &company_name,
                    "经营指标",
                    None,
                    "company",
                )
                .await;

            let n = completed.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("经营指标分析: {} ({}/{})", company_name, n, total),
                progress: n as f64 / total as f64,
                status: if result.success { ProgressStatus::Running } else { ProgressStatus::Error },
                company: Some(company_name.clone()),
            });

            if !result.success {
                let _ = window.emit("analysis-progress", ProgressUpdate {
                    step: format!("{} 分析失败: {}", company_name, result.error_message.as_deref().unwrap_or("未知错误")),
                    progress: n as f64 / total as f64,
                    status: ProgressStatus::Error,
                    company: Some(company_name),
                });
            }

            result
        });
    }

    let mut all_results = Vec::new();
    tracing::info!(
        "[经营分析] 开始: {}家公司, 并发数=3, YTD月份={}, 序时进度={:.2}%",
        total, ytd_months, progress_pct
    );
    while let Some(task_result) = set.join_next().await {
        match task_result {
            Ok(r) => {
                let done = all_results.len() + 1;
                let success = r.success;
                let company = r.company_name.clone();
                let score = r.quality_score;
                let content_len = r.content.len();
                let token_est = r.token_usage.as_ref().map(|t| t.total_tokens as u64 * 10).unwrap_or(0);
                all_results.push(r);
                tracing::info!(
                    "[经营分析] [{}/{}] {} {} (est={}ms, score={}, len={}字)",
                    done, total,
                    if success { "✅" } else { "❌" },
                    company, token_est, score, content_len
                );
            }
            Err(e) => {
                tracing::error!("[经营分析] JoinSet 任务异常: {}", e);
            }
        }
    }

    // 汇总分析结果
    let success_count = all_results.iter().filter(|r| r.success).count();
    let fail_count = all_results.len() - success_count;
    tracing::info!(
        "[经营分析] 完成: {}成功 {}失败",
        success_count, fail_count
    );
    for r in &all_results {
        tracing::info!(
            "[经营分析] {}: success={} len={} score={} err={:?}",
            r.company_name, r.success, r.content.len(), r.quality_score, r.error_message
        );
    }

    let _ = window.emit("analysis-progress", ProgressUpdate {
        step: "经营指标分析完成".into(),
        progress: 1.0,
        status: ProgressStatus::Done,
        company: None,
    });

    // 存储到 AppState（合并已有 segment 结果 + 新的 company 结果）
    {
        let mut stored = state.analysis_results.lock().await;
        stored.retain(|r: &AnalysisResult| r.analysis_category != "company");
        stored.extend(all_results.clone());
        tracing::info!("[经营分析] AppState 中 AI 结果共 {} 条 (company: {})",
            stored.len(),
            stored.iter().filter(|r| r.analysis_category == "company").count()
        );
    }

    // 自动回写到模板：公司分析结果 → 各公司 Sheet C61
    {
        let agg = state.aggregation_results.lock().await;
        let ai = state.analysis_results.lock().await;
        tracing::info!(
            "[公司分析] 回写模板: agg={}条 ai={}条 → {}",
            agg.len(), ai.len(), project.output_file.display()
        );
        if let Err(e) = crate::services::report_writer::ReportWriter::write_summary(
            &project.output_file, &agg, &ai,
            &project.name, project.year, project.month,
        ) {
            tracing::error!("[公司分析] 回写模板失败: {}", e);
        } else {
            tracing::info!("[公司分析] ✅ 回写模板成功");
        }
    }

    Ok(all_results)
}

/// 执行 AI 业态分析（两阶段：板块分析 + 子公司经营指标分析）
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

    // ✅ 提前获取汇总数据快照并在使用后立即释放锁，避免后续回写时死锁
    let agg_results = {
        let locked = state.aggregation_results.lock().await;
        locked.clone()
    }; // MutexGuard 在此释放
    // 收集所有涉及的公司（去重）
    let mut all_companies: Vec<&crate::models::project::Company> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let analyzer = AIAnalyzer::new(ai_config)?;
    let mut all_results = Vec::new();
    let total_types = business_types.len();
    let total_companies_count: usize = business_types.iter().map(|bt| {
        let bt_enum = parse_business_type(bt).unwrap();
        project.companies.iter().filter(|c| c.business_type == bt_enum).count()
    }).sum();
    // 总步骤 = 板块分析数 + 公司分析数
    let total_steps = total_types + total_companies_count;
    let mut step_idx = 0usize;

    // ====================
    // 阶段一：板块级分析（每个业态用专属提示词，仅用本业态汇总数据）
    // ====================
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

        // 记录公司（用于阶段二）
        for c in &companies {
            if seen.insert(c.name.clone()) {
                all_companies.push(c);
            }
        }

        let segment_name = format!("{}板块", bt);

        // ✅ 与AardMiner/VBA原型一致：从汇总表行业Sheet直接读取单元格数据
        let user_data = read_segment_data(&project.output_file, &bt, project.month);

        let user_prompt = if let Some(data) = user_data {
            format!(
                "请对以下{}子公司的经营数据进行分析，并按系统提示词指定格式输出。\n数据表格如下：\n{}",
                bt, data
            )
        } else {
            // 回退：使用内存汇总数据
            tracing::warn!("[板块分析] {} 汇总表无数据，回退到内存汇总", segment_name);
            let engine_name = engine_name_for_business_type(&bt);
            let agg_data_map = build_company_data_map_filtered(&agg_results, Some(engine_name));
            let combined_data: Vec<String> = companies
                .iter()
                .map(|c| {
                    let data_text = agg_data_map
                        .get(&c.name)
                        .cloned()
                        .unwrap_or_else(|| {
                            format!("公司: {}\n（未找到汇总数据）", c.name)
                        });
                    format!("【{}】\n{}", c.name, data_text)
                })
                .collect();
            format!(
                "请分析以下{}子公司的经营数据：\n\n{}",
                bt,
                combined_data.join("\n\n---\n\n")
            )
        };

        let system_prompt = if let Some(ref p) = custom_prompt {
            if !p.trim().is_empty() {
                p.clone()
            } else {
                analyzer.load_system_prompt(Some(&bt))?
            }
        } else {
            analyzer.load_system_prompt(Some(&bt))?
        };

        step_idx += 1;
        let _ = window.emit("analysis-progress", ProgressUpdate {
            step: format!("板块分析: {} (第{}/{})", segment_name, step_idx, total_steps),
            progress: step_idx as f64 / total_steps as f64,
            status: ProgressStatus::Running,
            company: Some(segment_name.clone()),
        });

        let result = analyzer
            .analyze_segment(
                &system_prompt,
                &user_prompt,
                &segment_name,
                &bt.to_string(),
            )
            .await;

        if !result.success {
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("{} 分析失败: {}", segment_name, result.error_message.as_deref().unwrap_or("未知错误")),
                progress: step_idx as f64 / total_steps as f64,
                status: ProgressStatus::Error,
                company: Some(segment_name),
            });
        }

        all_results.push(result);
    }

    // ====================
    // 阶段二：子公司经营指标分析（每家公司独立用财务分析师提示词）
    // ====================
    let financial_prompt = Arc::new(analyzer.load_system_prompt(None)?);
    // ✅ 优先从汇总表读取，回退到内存汇总数据
    let fallback_data_map = build_company_data_map_filtered(&agg_results, Some("经营报表汇总"));
    let indicator_names = read_indicator_names_from_template(&project.output_file)
        .unwrap_or_default();
    let progress_pct = project.month as f64 / 12.0 * 100.0;

    let output_file = project.output_file.clone();
    let ytd_months = project.ytd_months;

    let step_idx_base = step_idx; // 阶段一结束后的进度基数
    let analyzer = Arc::new(analyzer);
    let semaphore = Arc::new(Semaphore::new(3));
    let completed = Arc::new(AtomicUsize::new(0));
    let company_total = all_companies.len();
    let mut set = JoinSet::new();

    for company in &all_companies {
        let analyzer = analyzer.clone();
        let prompt = financial_prompt.clone();
        let permit = semaphore.clone();
        let completed = completed.clone();
        let window = window.clone();
        let company_name = company.name.clone();
        // 优先从汇总表 C1:R5 读取，但仅当当前月份有数据时才使用，否则回退到内存数据
        let data_text = read_company_data_from_summary(&output_file, &company_name, ytd_months)
            .filter(|text| has_month_data(text, project.month))
            .or_else(|| {
                let fallback = fallback_data_map.get(&company_name).cloned().unwrap_or_default();
                if fallback.is_empty() { None } else { Some(fallback) }
            })
            .unwrap_or_default();
        let names = indicator_names.get(&company_name).cloned();
        let data_text = apply_indicator_names(&data_text, names.as_ref());
        let year = project.year;
        let month = project.month;

        set.spawn(async move {
            let _permit = permit.acquire_owned().await;
            let user_prompt = format!(
                "公司名称：{}\n年份：{}\n当前月份：{}月\n序时进度：{:.2}%\n数据单位：万元\n请按系统提示词要求输出指定格式。\n\n{}",
                company_name, year, month, progress_pct, data_text
            );

            let result = analyzer
                .analyze_single(
                    &prompt,
                    &user_prompt,
                    &company_name,
                    "经营指标",
                    None,
                    "company",
                )
                .await;

            let n = completed.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("经营指标分析: {} ({}/{})", company_name, n, company_total),
                progress: (step_idx_base + n) as f64 / total_steps as f64,
                status: if result.success { ProgressStatus::Running } else { ProgressStatus::Error },
                company: Some(company_name.clone()),
            });

            if !result.success {
                let _ = window.emit("analysis-progress", ProgressUpdate {
                    step: format!("{} 分析失败: {}", company_name, result.error_message.as_deref().unwrap_or("未知错误")),
                    progress: (step_idx_base + n) as f64 / total_steps as f64,
                    status: ProgressStatus::Error,
                    company: Some(company_name),
                });
            }

            result
        });
    }

    while let Some(task_result) = set.join_next().await {
        match task_result {
            Ok(r) => all_results.push(r),
            Err(e) => {
                tracing::error!("[经营分析] JoinSet 任务异常: {}", e);
            }
        }
    }

    // 汇总分析结果
    let success_count = all_results.iter().filter(|r| r.success).count();
    let fail_count = all_results.len() - success_count;
    tracing::info!(
        "[分析] 全部完成: segment={} company={} | {}成功 {}失败",
        all_results.iter().filter(|r| r.analysis_category == "segment").count(),
        all_results.iter().filter(|r| r.analysis_category == "company").count(),
        success_count, fail_count
    );
    for r in &all_results {
        tracing::info!(
            "[分析] cat={} {}: success={} len={} score={} err={:?}",
            r.analysis_category, r.company_name,
            r.success, r.content.len(), r.quality_score, r.error_message
        );
    }

    let _ = window.emit("analysis-progress", ProgressUpdate {
        step: "全部分析完成".into(),
        progress: 1.0,
        status: ProgressStatus::Done,
        company: None,
    });

    // 存储到 AppState 供导出步骤使用
    *state.analysis_results.lock().await = all_results.clone();

    // 自动回写到模板：板块分析 → L14/M14 + 公司分析 → C61
    {
        let agg = state.aggregation_results.lock().await;
        let ai = state.analysis_results.lock().await;
        tracing::info!(
            "[分析] 回写模板: agg={}条 ai={}条 → {}",
            agg.len(), ai.len(), project.output_file.display()
        );
        if let Err(e) = crate::services::report_writer::ReportWriter::write_summary(
            &project.output_file, &agg, &ai,
            &project.name, project.year, project.month,
        ) {
            tracing::error!("[分析] 回写模板失败: {}", e);
        } else {
            tracing::info!("[分析] ✅ 回写模板成功");
        }
    }

    Ok(all_results)
}

/// 从汇总结果中提取按公司分组的数据文本（按引擎名称过滤）
/// engine_name_filter: 仅提取匹配引擎的数据，None 表示不过滤
fn build_company_data_map_filtered(
    agg_results: &[AggregationResult],
    engine_name_filter: Option<&str>,
) -> std::collections::HashMap<String, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for result in agg_results {
        // 按引擎名称过滤
        if let Some(filter) = engine_name_filter {
            if result.engine_name != filter {
                continue;
            }
        }

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
                            // 展平嵌套对象为可读的 key: value 格式，避免 JSON 大块输出
                            flatten_value_to_prompt(entry, k, v, "");
                        }
                    }
                }
            }
        }
    }

    map
}

/// 递归展平 serde_json::Value 为可读的 prompt 文本行
fn flatten_value_to_prompt(out: &mut String, key: &str, v: &serde_json::Value, indent: &str) {
    match v {
        serde_json::Value::Object(map) => {
            for (sub_key, sub_val) in map {
                let full_key = format!("{}.{}", key, sub_key);
                flatten_value_to_prompt(out, &full_key, sub_val, indent);
            }
        }
        serde_json::Value::Array(arr) => {
            // 检测是否为 [{label, target?, ytd?, values}, ...] 结构（经营报表指标行）
            if arr.first().and_then(|x| x.get("label")).is_some()
                && arr.first().and_then(|x| x.get("values")).is_some()
            {
                // 只取 C1:R5 对应的 4 个核心指标（营业收入/EBITDA/经营活动净现金流/经营支出）
                let core: std::collections::HashSet<&str> = [
                    "营业收入", "EBITDA", "经营活动净现金流", "经营支出",
                ].into();
                // 先输出表头
                out.push_str("指标\t年度目标\t实际达成\t达成率\t1月\t2月\t3月\t4月\t5月\t6月\t7月\t8月\t9月\t10月\t11月\t12月\n");
                for item in arr {
                    let label = item["label"].as_str().unwrap_or("").trim().to_string();
                    if label.is_empty() || !core.contains(label.as_str()) { continue; }
                    let target = item["target"].as_f64().map(|t| format!("{:.2}", t)).unwrap_or_else(|| "-".into());
                    let ytd = item["ytd"].as_f64().map(|y| format!("{:.2}", y)).unwrap_or_else(|| "-".into());
                    let rate = item["rate"].as_f64().map(|r| format!("{:.2}%", r)).unwrap_or_else(|| "-".into());
                    let months: Vec<String> = item["values"].as_array()
                        .map(|v| v.iter().map(|x| {
                            if let Some(n) = x.as_f64() {
                                if n.is_finite() && n != 0.0 { format!("{:.2}", n) } else { "-".into() }
                            } else { "-".into() }
                        }).collect())
                        .unwrap_or_default();
                    out.push_str(&format!("{}\t{}\t{}\t{}\t{}\n", label, target, ytd, rate, months.join("\t")));
                }
                return;
            }
            // 普通数组
            if arr.len() <= 12 {
                let vals: Vec<String> = arr.iter().map(|x| {
                    if let Some(n) = x.as_f64() {
                        if n.is_finite() { format!("{:.2}", n) } else { "#N/A".into() }
                    } else { format!("{}", x) }
                }).collect();
                out.push_str(&format!("{}{}: [{}]\n", indent, key, vals.join(", ")));
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                out.push_str(&format!("{}{}: {:.2}\n", indent, key, f));
            } else {
                out.push_str(&format!("{}{}: {}\n", indent, key, n));
            }
        }
        serde_json::Value::String(s) => {
            out.push_str(&format!("{}{}: {}\n", indent, key, s));
        }
        _ => {
            out.push_str(&format!("{}{}: {}\n", indent, key, v));
        }
    }
}

/// 从汇总表（output_file）指定公司 Sheet 的 C1:R5 汇总区域读取财务数据
/// C1:R5 布局: C=指标名, D=年度目标, E=实际达成(YTD), F=达成率, G:R=1~12月
fn read_company_data_from_summary(
    output_path: &std::path::Path,
    company_name: &str,
    _ytd_months: u32,
) -> Option<String> {
    if !output_path.exists() { return None; }

    let mut workbook: calamine::Xlsx<_> = calamine::open_workbook(output_path).ok()?;
    let range = workbook.worksheet_range(company_name).ok()?;
    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    if rows.len() < 5 { return None; }

    // C=col2, D=col3, E=col4, F=col5, G:R=col6~17 (0-based)
    let mut out = String::from("指标\t年度目标\t实际达成\t年度目标达成率\t1月\t2月\t3月\t4月\t5月\t6月\t7月\t8月\t9月\t10月\t11月\t12月\n");

    let mut has_data = false;

    for r in 1..=4 {
        let row_data = match rows.get(r) {
            Some(row) => row,
            None => continue,
        };

        // C 列 (col 2): 指标名称
        let label = cell_text(Some(row_data), 2);
        if label.is_empty() || label.parse::<f64>().is_ok() {
            continue;
        }

        // D 列 (col 3): 年度目标
        let target = cell_number(row_data, 3);

        // E 列 (col 4): 实际达成 (YTD)
        let ytd_actual = cell_number(row_data, 4);

        // F 列 (col 5): 年度目标达成率 — 读原始文本保留百分号格式
        let rate_str = cell_text(Some(row_data), 5);
        let rate_display = if rate_str.is_empty() || rate_str == "0" {
            "-".into()
        } else if rate_str.contains('%') {
            rate_str
        } else {
            // 纯数字，尝试格式化为百分比
            if let Ok(n) = rate_str.parse::<f64>() {
                if n.is_finite() && n != 0.0 {
                    format!("{:.2}%", if n <= 1.0 { n * 100.0 } else { n })
                } else { "-".into() }
            } else { "-".into() }
        };

        // G-R 列 (cols 6-17): 1~12月
        let mut month_vals: Vec<String> = Vec::with_capacity(12);
        for ci in 6..=17 {
            if ci < row_data.len() {
                let val = extract_number_financial(&row_data[ci].to_string());
                if val.is_finite() && val != 0.0 {
                    month_vals.push(format!("{:.2}", val));
                } else {
                    month_vals.push("-".into());
                }
            } else {
                month_vals.push("-".into());
            }
        }

        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            label,
            if target.is_finite() && target != 0.0 { format!("{:.2}", target) } else { "-".into() },
            if ytd_actual.is_finite() && ytd_actual != 0.0 { format!("{:.2}", ytd_actual) } else { "-".into() },
            rate_display,
            month_vals.join("\t"),
        ));

        if target != 0.0 || ytd_actual != 0.0 {
            has_data = true;
        }
    }

    if !has_data { return None; }

    tracing::info!("[分析] 从汇总表 C1:R5 读取 {} 数据:\n{}", company_name, out);
    Some(out)
}

/// 读取单元格文本（空→""）
fn cell_text(row: Option<&&[calamine::Data]>, col: usize) -> String {
    row.and_then(|r| r.get(col))
        .map(|c| c.to_string().trim().to_string())
        .unwrap_or_default()
}

/// 检查格式化数据是否包含指定月份的数据
fn has_month_data(text: &str, month: u32) -> bool {
    // 格式: 指标\t年度目标\t实际达成\t达成率\t1月\t2月\t...
    // 月份 N 在第 3+N 列（0-based）
    let field_idx = (3 + month) as usize;
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() > field_idx {
            let val = fields[field_idx].trim();
            if val != "-" && !val.is_empty() { return true; }
        }
    }
    false
}

/// 读取单元格数值（空/非数字→0.0）
fn cell_number(row: &&[calamine::Data], col: usize) -> f64 {
    row.get(col)
        .map(|c| extract_number_financial(&c.to_string()))
        .unwrap_or(0.0)
}

/// 提取单元格数值（处理 calamine 的多种数据类型）
fn extract_number_financial(text: &str) -> f64 {
    let t = text.trim();
    if t.is_empty() { return 0.0; }
    // 先尝试直接解析
    if let Ok(n) = t.parse::<f64>() {
        return n;
    }
    // 使用 number_parser 处理中文格式
    crate::services::number_parser::extract_number(t).unwrap_or(0.0)
}

/// 按业态读取板块数据 — 与 AardMiner/VBA 原型精确一致
/// 商写: A1:G18, 保险: F1:H25 (含规模保费月份过滤), 酒店: 三区域合并
fn read_segment_data(
    output_path: &std::path::Path,
    bt: &BusinessType,
    current_month: u32,
) -> Option<String> {
    match bt {
        BusinessType::Commercial => {
            read_industry_segment_data(output_path, "商写类", (1, 18, 1, 7), current_month)
        }
        BusinessType::Insurance => {
            // 保险需同时读取: A1:D18 详细指标(人力/承保) + F1:H25 月度规模保费
            let indicators = read_industry_segment_data(output_path, "保险类", (1, 18, 1, 4), current_month);
            let monthly = read_industry_segment_data(output_path, "保险类", (1, 25, 6, 8), current_month);
            match (indicators, monthly) {
                (Some(ind), Some(mon)) => Some(format!("详细指标：\n{}\n月度规模保费：\n{}", ind, mon)),
                (Some(ind), None) => Some(format!("详细指标：\n{}", ind)),
                (None, Some(mon)) => Some(mon),
                (None, None) => None,
            }
        }
        BusinessType::Hotel => {
            read_hotel_segment_data(output_path, current_month)
        }
    }
}

/// 酒店业态三区域独立读取 (B1:D5 营销 / E1:G13 OTA / I1:K13 入住率)
/// 与 AardMiner sector_analysis.aardio runHotel 完全一致
fn read_hotel_segment_data(output_path: &std::path::Path, current_month: u32) -> Option<String> {
    if !output_path.exists() { return None; }

    let marketing = read_industry_segment_data(output_path, "酒店类", (1, 5, 2, 4), current_month)?;   // B1:D5
    let ota = read_industry_segment_data(output_path, "酒店类", (1, 13, 5, 7), current_month)?;        // E1:G13
    let occ = read_industry_segment_data(output_path, "酒店类", (1, 13, 9, 11), current_month)?;       // I1:K13

    Some(format!(
        "以下是两家酒店子公司的经营数据，包含三个独立区域：\n\n【区域一：营销活动】\n{}\n【区域二：OTA网络评价】\n{}\n【区域三：月均入住率】\n{}\n请根据系统提示词进行分析，并按指定输出格式给出分析结果。",
        marketing, ota, occ
    ))
}

/// 从汇总表读取业态板块 Sheet 数据，格式化为 AI prompt 文本
/// 与 VBA 原型逻辑一致：直接读取输出文件中行业 Sheet 的单元格数据
fn read_industry_segment_data(
    output_path: &std::path::Path,
    sheet_name: &str,
    range: (u32, u32, u32, u32), // (start_row, end_row, start_col, end_col) 1-based
    current_month: u32,
) -> Option<String> {
    if !output_path.exists() { return None; }

    let mut workbook: calamine::Xlsx<_> = calamine::open_workbook(output_path).ok()?;
    let range_data = workbook.worksheet_range(sheet_name).ok()?;
    let rows: Vec<&[calamine::Data]> = range_data.rows().collect();

    let (r1, r2, c1, c2) = range;
    let start_row = (r1 as usize).saturating_sub(1);
    let end_row = (r2 as usize).min(rows.len());
    let start_col = (c1 as usize).saturating_sub(1);
    let end_col = c2 as usize;

    let mut out = String::new();
    let mut has_data = false;

    // 检测是否包含"规模保费"行（保险业态特有：过滤未来月份）
    let mut in_scale_section = false;

    for ri in start_row..end_row {
        let row_data = match rows.get(ri) { Some(r) => r, None => continue };

        // 检测行首标签
        let first_label = row_data.get(start_col).map(|c| c.to_string().trim().to_string()).unwrap_or_default();

        if first_label.contains("规模保费") {
            in_scale_section = true;
        } else if first_label.contains("项目") || first_label.contains("合计") {
            in_scale_section = false;
        }

        // 规模保费区域内，跳过超过当前月份的行
        if in_scale_section && ri > start_row {
            let month_num = parse_month_from_cell(&first_label);
            if month_num > 0 && current_month > 0 && month_num > current_month {
                continue;
            }
        }

        let mut row_vals: Vec<String> = Vec::new();
        for ci in start_col..end_col {
            if ci < row_data.len() {
                let text = row_data[ci].to_string().trim().to_string();
                if !text.is_empty() && text != "0" { has_data = true; }
                // 空单元格显示为空
                if text.is_empty() {
                    row_vals.push(String::new());
                } else if let Ok(n) = text.parse::<f64>() {
                    if n.is_finite() && n != 0.0 {
                        row_vals.push(format!("{:.2}", n));
                    } else {
                        row_vals.push(String::new());
                    }
                } else {
                    row_vals.push(text);
                }
            } else {
                row_vals.push(String::new());
            }
        }

        // 跳过全空行
        if row_vals.iter().all(|s| s.is_empty()) { continue; }

        out.push_str(&format!("| {}\n", row_vals.join(" | ")));
    }

    if !has_data && out.len() < 50 { return None; }

    // 日志输出取到的数据前500字符（安全截断，对齐 UTF-8 字符边界）
    let preview = if out.len() > 500 {
        let end = out.char_indices()
            .nth(500)
            .map(|(i, _)| i)
            .unwrap_or(out.len());
        &out[..end]
    } else {
        out.as_str()
    };
    tracing::info!(
        "[分析] 从汇总表 {}/{} 读取 {}rows, 数据预览:\n{}",
        sheet_name, format_range(r1,r2,c1,c2),
        end_row.saturating_sub(start_row),
        preview
    );
    Some(out)
}

fn format_range(r1: u32, r2: u32, c1: u32, c2: u32) -> String {
    format!("{}{}:{}{}", col_letter_analysis(c1), r1, col_letter_analysis(c2), r2)
}

fn col_letter_analysis(col: u32) -> String {
    let mut n = col.saturating_sub(1);
    let mut v = Vec::new();
    loop {
        v.push((b'A' + (n % 26) as u8) as char);
        if n < 26 { break; }
        n = n / 26 - 1;
    }
    v.reverse();
    v.into_iter().collect()
}

fn parse_month_from_cell(text: &str) -> u32 {
    let t = text.trim();
    let t = t.trim_end_matches('月');
    t.parse::<u32>().unwrap_or(0)
}

/// 业态 → 引擎名称映射（回退用）
fn engine_name_for_business_type(bt: &BusinessType) -> &'static str {
    match bt {
        BusinessType::Insurance => "保险数据汇总",
        BusinessType::Commercial => "商写数据汇总",
        BusinessType::Hotel => "酒店数据汇总",
    }
}

/// 用 calamine 从汇总表模板读取各公司 Sheet 的 B2:B5 指标名
fn read_indicator_names_from_template(
    template_path: &std::path::Path,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    if !template_path.exists() { return Ok(map); }

    let mut workbook: calamine::Xlsx<_> = calamine::open_workbook(template_path)
        .map_err(|e| AppError::Other(format!("无法打开汇总表: {}", e)))?;

    for sheet_name in workbook.sheet_names().to_vec() {
        // 只处理公司 Sheet（排除 填写页/保险类/商写类/酒店类/AI分析结果）
        if matches!(sheet_name.as_str(), "填写页" | "保险类" | "商写类" | "酒店类" | "AI分析结果") {
            continue;
        }
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            let all_rows: Vec<&[calamine::Data]> = range.rows().collect();
            let mut names = Vec::new();
            // C2:C5 → row 1-4 (0-based), col 2 (C列=指标名)
            for r in 1..=4 {
                if let Some(row) = all_rows.get(r) {
                    if let Some(cell) = row.get(2) {
                        let s = cell.to_string().trim().to_string();
                        // 只取文本（非纯数字、非空、非公式数值）
                        if !s.is_empty() && s.parse::<f64>().is_err() {
                            names.push(s);
                        }
                    }
                }
            }
            if !names.is_empty() {
                map.insert(sheet_name, names);
            }
        }
    }
    Ok(map)
}

/// 用模板 B 列读取的指标名替换 prompt 中的硬编码标签
fn apply_indicator_names(data_text: &str, names: Option<&Vec<String>>) -> String {
    let Some(names) = names else { return data_text.to_string(); };
    if names.is_empty() { return data_text.to_string(); }

    let old_names = ["营业收入", "EBITDA", "经营活动净现金流", "经营支出"];
    let mut result = data_text.to_string();
    for (i, old) in old_names.iter().enumerate() {
        let new = &names[i];
        if new != *old {
            // 替换行首的指标名（\n 后紧跟的标签）
            result = result.replace(&format!("\n{}\t", old), &format!("\n{}\t", new));
            // 也替换表头行中的（如果有）
            result = result.replace(&format!("{}\t年度目标", old), &format!("{}\t年度目标", new));
        }
    }
    result
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

/// 从 ~/.dskey 文件读取指定分组的 API Key
#[tauri::command]
pub fn read_dskey(section: &str) -> Result<Option<String>, AppError> {
    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".dskey");

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Io(e.to_string())),
    };

    let prefix = format!("{}=", section);
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with(&prefix) {
            let value = trimmed[prefix.len()..].trim().to_string();
            return Ok(Some(value));
        }
    }

    Ok(None)
}
