//! 经营报表汇总引擎
//!
//! 数据源: 经营报表/ 文件夹下各子公司 .xlsx
//! 使用模式: 从"填写页"C2:C10读取公司列表，逐公司复制"指标统计"Sheet的D4:O20→G2:R18
//! 对应 VBA: 经营报表汇总.bas 的 ProcessSingleCompany 函数

use crate::error::AppResult;
use crate::models::analysis::{AggregationResult, PreviewData};
use crate::models::project::{BusinessType, Company, Project};
use crate::services::company_registry::company_registry;
use crate::services::excel_reader::ExcelReader;
use crate::services::number_parser::extract_number;
use crate::services::data_aggregator::{AggregationEngine, EngineType};

pub struct FinancialAggregator;

impl AggregationEngine for FinancialAggregator {
    fn engine_type(&self) -> EngineType {
        EngineType::Financial
    }

    fn name(&self) -> &str {
        "经营报表汇总"
    }

    fn preview(&self, project: &Project) -> AppResult<PreviewData> {
        // 检查经营报表文件夹下各子公司文件
        let mut files_found = Vec::new();
        let report_folder = project.data_folder.join("经营报表");

        for company in &project.companies {
            let file_path = report_folder.join(format!("{}.xlsx", company.name));
            if file_path.exists() {
                files_found.push(file_path.to_string_lossy().to_string());
            }
        }

        let is_empty = files_found.is_empty();
        Ok(PreviewData {
            engine_name: self.name().into(),
            files_found,
            sheets_detected: vec!["指标统计".into()],
            companies_detected: project.companies.iter().map(|c| c.name.clone()).collect(),
            available_indicators: vec![
                "营业收入".into(), "营业成本".into(), "管理费用".into(),
                "销售费用".into(), "净利润".into(), "EBITDA".into(),
                "经营活动净现金流".into(), "经营支出".into(),
            ],
            warnings: if is_empty {
                vec!["经营报表文件夹为空".into()]
            } else {
                vec![]
            },
        })
    }

    fn execute(&self, project: &Project) -> AppResult<AggregationResult> {
        let folder = project.data_folder.join("经营报表");
        let mut warnings = Vec::new();
        let mut results: Vec<serde_json::Value> = Vec::new();

        // 优先使用项目配置的公司列表，为空时从注册表加载
        let companies: Vec<Company> = if !project.companies.is_empty() {
            project.companies.clone()
        } else {
            let registry = company_registry();
            let mut cs = Vec::new();
            for c in &registry.insurance { cs.push(Company { name: c.name.clone(), business_type: BusinessType::Insurance, regions: vec![] }); }
            for c in &registry.commercial { cs.push(Company { name: c.name.clone(), business_type: BusinessType::Commercial, regions: vec![] }); }
            for h in &registry.hotel { cs.push(Company { name: h.name.clone(), business_type: BusinessType::Hotel, regions: vec![] }); }
            cs
        };

        // 按照 VBA: 逐公司打开经营报表→读取"指标统计"Sheet
        // 源范围 D4:O20(16行x12列) → 复制到目标 G2:R18
        for company in &companies {
            let mut targets_missing_warned = false; // 每个公司只警告一次
            let path = folder.join(format!("{}.xlsx", company.name));
            let mut reader = match ExcelReader::open(&path) {
                Ok(r) => r, Err(e) => {
                    warnings.push(format!("{}: 打开失败 - {}", company.name, e)); continue;
                }
            };
            let data = match reader.read_sheet("指标统计") {
                Ok(d) => d, Err(e) => {
                    warnings.push(format!("{}: 读取失败 - {}", company.name, e)); continue;
                }
            };

            // 提取 D4:O20 区域 (0-based: rows[3..20], cols[3..15])
            // 同时读取 A列(标签) 和 C列(年度目标)
            let num_months = project.ytd_months.max(1).min(12) as usize;
            // 保存原始 16行×12列 网格 (对应 VBA 纯值复制 D4:O20 → G2:R18)
            let raw_grid: Vec<Vec<String>> = (3..20).map(|ri| {
                if let Some(row_data) = data.rows.get(ri) {
                    (3..15).map(|ci| {
                        row_data.get(ci).cloned().unwrap_or_default().to_string()
                    }).collect()
                } else {
                    vec!["".to_string(); 12]
                }
            }).collect();
            let mut indicators: Vec<serde_json::Value> = Vec::new();
            // 源数据 A 列是 section header（"经营指标""财务指标""业务指标"），
            // 真正指标名需按 section 内位置映射
            let mut section: String = String::new();
            let mut idx: usize = 0;
            for (ri, row_data) in data.rows.iter().enumerate() {
                if ri < 3 || ri >= 20 { continue; }
                let raw_label = row_data.first().cloned().unwrap_or_default();
                let raw_str: String = raw_label.trim().to_string();
                let is_empty = raw_str.is_empty();
                // 非空标签 = 新的 section header
                if !is_empty {
                    section = raw_str.clone();
                    idx = 0;
                }
                // 按 section + 位置生成指标名
                let label: String = match section.as_str() {
                    "经营指标" => match idx {
                        0 => "营业收入".into(),
                        1 => "EBITDA".into(),
                        2 => "经营活动净现金流".into(),
                        _ => format!("经营指标_{}", idx),
                    },
                    "财务指标" => match idx {
                        0 => "经营支出".into(),
                        _ => format!("财务指标_{}", idx),
                    },
                    _ => {
                        if is_empty { format!("{}_{}", section, idx) }
                        else { raw_str }
                    }
                };
                idx += 1;
                // 尝试读取 C 列（col 2, 0-based）作为年度目标
                let annual_target: Option<f64> = if row_data.len() > 2 {
                    extract_number(&row_data[2])
                } else { None };
                // C列有文本但无法解析为数字时发出警告（源数据模板可能用了指标名而非公式引用）
                if annual_target.is_none() && !targets_missing_warned && row_data.len() > 2 {
                    let c_val = row_data[2].trim().to_string();
                    if !c_val.is_empty() {
                        targets_missing_warned = true;
                        warnings.push(format!(
                            "{}: 年度目标列为文本(如\"{}\")，年度任务、达成率等将无法计算",
                            company.name, c_val
                        ));
                    }
                }
                // D-O 列月度值 (cols 3..15, 0-based)
                let vals: Vec<f64> = (3..15).map(|ci| {
                    if ci < row_data.len() {
                        extract_number(&row_data[ci]).unwrap_or(0.0)
                    } else { 0.0 }
                }).collect();
                // 计算 YTD 合计
                let ytd: f64 = vals.iter().take(num_months).sum();
                // 计算年度达成率
                let rate: Option<f64> = annual_target.and_then(|t| {
                    if t != 0.0 { Some(ytd / t * 100.0) } else { None }
                });
                indicators.push(serde_json::json!({
                    "label": label,
                    "target": annual_target,
                    "ytd": ytd,
                    "rate": rate,
                    "values": vals,
                }));
            }

            results.push(serde_json::json!({
                "company": company.name,
                "indicators": indicators,
                "raw_grid": raw_grid,
            }));
        }

        Ok(AggregationResult {
            engine_name: self.name().into(),
            companies_processed: results.len(),
            indicators_collected: 8,
            warnings,
            summary_data: serde_json::to_string(&results).unwrap_or_default(),
        })
    }
}
