//! AI 分析命令

use tauri::{Emitter, State, Window};

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

    let agg_results = state.aggregation_results.lock().await;

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
            continue;
        }

        let segment_name = format!("{}板块", bt);
        // ✅ 仅提取该业态引擎的汇总数据（不混入经营报表）
        let engine_name = engine_name_for_business_type(&bt);
        let agg_data_map = build_company_data_map_filtered(&agg_results, Some(engine_name));

        // 合并所有公司数据
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
        let user_prompt = format!(
            "请分析以下{}子公司的经营数据：\n\n{}",
            bt,
            combined_data.join("\n\n---\n\n")
        );

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
            step: format!("板块分析: {} (第{}/{})", segment_name, type_idx + 1, total_types),
            progress: (type_idx as f64) / (total_types as f64),
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
                progress: (type_idx as f64) / (total_types as f64),
                status: ProgressStatus::Error,
                company: Some(segment_name),
            });
        }

        all_results.push(result);
    }

    let _ = window.emit("analysis-progress", ProgressUpdate {
        step: "板块分析完成".into(),
        progress: 1.0,
        status: ProgressStatus::Done,
        company: None,
    });

    // 存储到 AppState（只保留 segment 类结果）
    let mut stored = state.analysis_results.lock().await;
    stored.retain(|r: &AnalysisResult| r.analysis_category != "segment");
    stored.extend(all_results.clone());

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

    let agg_results = state.aggregation_results.lock().await;
    // ✅ 仅提取经营报表引擎的数据
    let agg_data_map = build_company_data_map_filtered(&agg_results, Some("经营报表汇总"));

    // 从汇总表模板读取各公司的 B 列指标名（C1:R5 第一列）
    let indicator_names = read_indicator_names_from_template(&project.output_file)
        .unwrap_or_default();

    let analyzer = AIAnalyzer::new(project.ai_config.clone())?;
    let financial_prompt = analyzer.load_system_prompt(None)?;
    let mut all_results = Vec::new();
    let total = project.companies.len();
    let progress_pct = project.month as f64 / 12.0 * 100.0;

    for (i, company) in project.companies.iter().enumerate() {
        let data_text = agg_data_map
            .get(&company.name)
            .cloned()
            .unwrap_or_default();

        // 用 B 列读取的指标名替换硬编码名
        let names = indicator_names.get(&company.name);
        let data_text = apply_indicator_names(&data_text, names);

        let user_prompt = format!(
            "公司名称：{}\n年份：{}\n当前月份：{}月\n序时进度：{:.2}%\n数据单位：万元\n请按系统提示词要求输出指定格式。\n\n{}",
            company.name, project.year, project.month, progress_pct, data_text
        );

        let _ = window.emit("analysis-progress", ProgressUpdate {
            step: format!("经营指标分析: {} (第{}/{})", company.name, i + 1, total),
            progress: (i as f64) / (total as f64),
            status: ProgressStatus::Running,
            company: Some(company.name.clone()),
        });

        let result = analyzer
            .analyze_single(
                &financial_prompt,
                &user_prompt,
                &company.name,
                "经营指标",
                None,
                "company",
            )
            .await;

        if !result.success {
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("{} 分析失败: {}", company.name, result.error_message.as_deref().unwrap_or("未知错误")),
                progress: (i as f64) / (total as f64),
                status: ProgressStatus::Error,
                company: Some(company.name.clone()),
            });
        }

        all_results.push(result);
    }

    let _ = window.emit("analysis-progress", ProgressUpdate {
        step: "经营指标分析完成".into(),
        progress: 1.0,
        status: ProgressStatus::Done,
        company: None,
    });

    // 存储到 AppState（合并已有 segment 结果 + 新的 company 结果）
    let mut stored = state.analysis_results.lock().await;
    stored.retain(|r: &AnalysisResult| r.analysis_category != "company");
    stored.extend(all_results.clone());

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

    // 从状态中读取汇总数据
    let agg_results = state.aggregation_results.lock().await;
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
        // ✅ 仅提取该业态引擎的汇总数据
        let engine_name = engine_name_for_business_type(&bt);
        let agg_data_map = build_company_data_map_filtered(&agg_results, Some(engine_name));

        // 合并所有公司数据
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
        let user_prompt = format!(
            "请分析以下{}子公司的经营数据：\n\n{}",
            bt,
            combined_data.join("\n\n---\n\n")
        );

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
    // 阶段二：子公司经营指标分析（每家公司独立用财务分析师提示词，仅用经营报表数据）
    // ====================
    let financial_prompt = analyzer.load_system_prompt(None)?;
    // ✅ 仅提取经营报表引擎的数据
    let financial_data_map = build_company_data_map_filtered(&agg_results, Some("经营报表汇总"));
    let indicator_names = read_indicator_names_from_template(&project.output_file)
        .unwrap_or_default();
    let progress_pct = project.month as f64 / 12.0 * 100.0;

    for company in &all_companies {
        let data_text = financial_data_map
            .get(&company.name)
            .cloned()
            .unwrap_or_default();

        let names = indicator_names.get(&company.name);
        let data_text = apply_indicator_names(&data_text, names);

        let user_prompt = format!(
            "公司名称：{}\n年份：{}\n当前月份：{}月\n序时进度：{:.2}%\n数据单位：万元\n请按系统提示词要求输出指定格式。\n\n{}",
            company.name, project.year, project.month, progress_pct, data_text
        );

        step_idx += 1;
        let _ = window.emit("analysis-progress", ProgressUpdate {
            step: format!("经营指标分析: {} (第{}/{})", company.name, step_idx, total_steps),
            progress: step_idx as f64 / total_steps as f64,
            status: ProgressStatus::Running,
            company: Some(company.name.clone()),
        });

        let result = analyzer
            .analyze_single(
                &financial_prompt,
                &user_prompt,
                &company.name,
                "经营指标",
                None,
                "company",
            )
            .await;

        if !result.success {
            let _ = window.emit("analysis-progress", ProgressUpdate {
                step: format!("{} 分析失败: {}", company.name, result.error_message.as_deref().unwrap_or("未知错误")),
                progress: step_idx as f64 / total_steps as f64,
                status: ProgressStatus::Error,
                company: Some(company.name.clone()),
            });
        }

        all_results.push(result);
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

/// 业态 → 引擎名称映射
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
